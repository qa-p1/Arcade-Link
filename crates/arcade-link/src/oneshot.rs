//! One-shot mode: `<exe> --arcade-invoke` reads one `invoke` request from
//! stdin, writes `job.progress` lines and a final response to stdout, and
//! exits. It must not start any UI, tray, shortcut or listener (SPEC §4.4).

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::error::{ErrorCode, LinkError};
use crate::server::{Handler, InvokeContext, Jobs, Reply, Sink};
use crate::wire::{self, method, InvokeRequest, InvokeResult, JobDone, JobProgress, LineReader, Message, PeerInfo};

/// The flag every app uses for one-shot mode.
pub const FLAG: &str = "--arcade-invoke";

/// Consumer side: runs `executable args…`, sends `request`, relays progress,
/// returns the final result. Setting `cancel` kills the process.
pub fn run(
    executable: &str,
    args: &[String],
    request: &InvokeRequest,
    on_progress: &mut dyn FnMut(&JobProgress),
    cancel: Option<&AtomicBool>,
) -> Result<InvokeResult, LinkError> {
    crate::childenv::release_mount_handles();
    let mut command = Command::new(executable);
    let mut child = crate::childenv::detach_from_image(&mut command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| LinkError::new(ErrorCode::LaunchFailed, format!("could not start {executable}: {e}")))?;
    let line = Message::request(1, method::INVOKE, serde_json::to_value(request).map_err(|e| LinkError::internal(e.to_string()))?).to_line();
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(line.as_bytes());
    }
    let stdout = child.stdout.take().ok_or_else(|| LinkError::internal("no stdout"))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut r = LineReader::new(stdout);
        loop {
            let m = r.read_message();
            let end = !matches!(m, Ok(Some(_)));
            if tx.send(m).is_err() || end {
                break;
            }
        }
    });
    let cancelled = || cancel.is_some_and(|c| c.load(Ordering::SeqCst));
    let result = loop {
        // Checked before every message, not only when the provider is quiet:
        // a provider streaming progress must still stop when asked.
        if cancelled() {
            let _ = child.kill();
            break Err(LinkError::cancelled());
        }
        let m = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break Err(LinkError::internal("one-shot process ended without a result")),
        };
        match m {
            Ok(Some(m)) if m.kind() == wire::Kind::Response => {
                break match (m.result, m.error) {
                    (_, Some(e)) => Err(e),
                    (Some(r), None) => serde_json::from_value(r).map_err(|e| LinkError::internal(e.to_string())),
                    _ => Err(LinkError::internal("empty response")),
                };
            }
            Ok(Some(m)) if m.method.as_deref() == Some(method::JOB_PROGRESS) => {
                if let Ok(p) = serde_json::from_value::<JobProgress>(m.params().clone()) {
                    on_progress(&p);
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break Err(LinkError::internal("one-shot process ended without a result")),
            Err(e) => break Err(e),
        }
    };
    // The answer is in: give the provider a moment to exit on its own, but
    // never let a lingering process (or a cancel) hold up the caller.
    let deadline = std::time::Instant::now() + EXIT_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if cancelled() || std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    result
}

/// How long a provider may keep running after its final response.
const EXIT_GRACE: Duration = Duration::from_millis(250);

/// Writes progress to stdout and keeps `job.done` for the final response.
struct StdoutSink {
    done: Mutex<Option<JobDone>>,
    finished: Condvar,
}

impl Sink for StdoutSink {
    fn send(&self, m: &Message) -> bool {
        if m.method.as_deref() == Some(method::JOB_DONE) {
            if let Ok(d) = serde_json::from_value::<JobDone>(m.params().clone()) {
                *self.done.lock().unwrap_or_else(|e| e.into_inner()) = Some(d);
                self.finished.notify_all();
            }
            return true;
        }
        let mut out = std::io::stdout().lock();
        wire::write_message(&mut out, m).is_ok()
    }
}

/// Provider side: serves one request from stdin with `handler` and returns
/// the process exit code (0 on success).
pub fn serve(handler: &dyn Handler) -> i32 {
    // One line is enough; don't wait for the caller to close stdin.
    let mut first = String::new();
    let mut stdin = std::io::BufReader::new(std::io::stdin().lock().take(wire::MAX_LINE as u64 + 1));
    while first.trim().is_empty() {
        first.clear();
        if std::io::BufRead::read_line(&mut stdin, &mut first).unwrap_or(0) == 0 {
            break;
        }
    }
    let first = first.as_str();
    let (id, outcome) = match Message::parse(first) {
        Ok(m) if m.method.as_deref() == Some(method::INVOKE) => {
            let id = m.id.unwrap_or(1);
            match serde_json::from_value::<InvokeRequest>(m.params().clone()) {
                Ok(req) => (id, run_handler(handler, req)),
                Err(e) => (id, Err(LinkError::unsupported(format!("invalid invoke: {e}")))),
            }
        }
        Ok(m) => (m.id.unwrap_or(1), Err(LinkError::internal("one-shot mode only accepts invoke"))),
        Err(e) => (1, Err(e)),
    };
    let ok = outcome.is_ok();
    let response = Message::response(id, outcome.map(|r| serde_json::to_value(r).unwrap_or(Value::Null)));
    let mut out = std::io::stdout().lock();
    let _ = wire::write_message(&mut out, &response);
    if ok {
        0
    } else {
        1
    }
}

fn run_handler(handler: &dyn Handler, request: InvokeRequest) -> Result<InvokeResult, LinkError> {
    let sink = Arc::new(StdoutSink { done: Mutex::new(None), finished: Condvar::new() });
    let ctx = InvokeContext { sink: sink.clone(), jobs: Arc::new(Jobs::default()), peer: PeerInfo::default(), started: Mutex::new(Vec::new()) };
    match handler.invoke(request, &ctx)? {
        Reply::Done(r) => Ok(r),
        Reply::Job(ticket) => {
            ticket.open();
            let mut done = sink.done.lock().unwrap_or_else(|e| e.into_inner());
            while done.is_none() {
                done = sink.finished.wait(done).unwrap_or_else(|e| e.into_inner());
            }
            done.take().map_or_else(|| Err(LinkError::internal("no result")), JobDone::into_result)
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Instant;

    fn request() -> InvokeRequest {
        serde_json::from_value(serde_json::json!({ "action": "test" })).unwrap()
    }

    /// A provider that reads the request, then runs `body` with $P (one
    /// progress line) and $R (the final response line) in its environment.
    fn provider(body: &str) -> (String, Vec<String>) {
        let progress = Message::notification(method::JOB_PROGRESS, serde_json::json!({ "job": "j", "message": "working" })).to_line();
        let response = Message::response(1, Ok(serde_json::to_value(InvokeResult::message("done")).unwrap())).to_line();
        let script = format!("P='{}'; R='{}'; read line; {body}", progress.trim_end(), response.trim_end());
        ("/bin/sh".into(), vec!["-c".into(), script])
    }

    #[test]
    fn cancel_stops_a_provider_that_keeps_reporting_progress() {
        let (sh, args) = provider(r#"for i in $(seq 30); do printf '%s\n' "$P"; sleep 0.02; done; printf '%s\n' "$R""#);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let mut seen = 0;
        let result = run(&sh, &args, &request(), &mut |_| seen += 1, Some(&cancel));
        assert_eq!(result.unwrap_err().code, ErrorCode::Cancelled);
        assert!(started.elapsed() < Duration::from_millis(400), "took {:?}", started.elapsed());
        assert!(seen < 30, "all progress was relayed after cancel");
    }

    #[test]
    fn a_provider_lingering_after_its_answer_does_not_hold_the_caller() {
        let (sh, args) = provider(r#"printf '%s\n' "$R"; sleep 2"#);
        let started = Instant::now();
        let result = run(&sh, &args, &request(), &mut |_| {}, None).unwrap();
        assert_eq!(result.message.as_deref(), Some("done"));
        assert!(started.elapsed() < Duration::from_millis(1000), "took {:?}", started.elapsed());
    }
}
