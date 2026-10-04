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
    let mut child = Command::new(executable)
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
    let result = loop {
        let m = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(m) => m,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
                    let _ = child.kill();
                    break Err(LinkError::cancelled());
                }
                continue;
            }
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
    let _ = child.wait();
    result
}

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
