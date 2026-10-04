//! `arcade-link mock`: a scriptable fake app for consumer tests.
//!
//! The fixture is a manifest whose actions may carry a `mock` object:
//!
//! ```json
//! { "id": "arcade.box", "name": "Arcade Box", "version": "0.0.0-mock",
//!   "busy": false,
//!   "actions": [ { "id": "box:arcade.image.convert#webp", "title": "Convert to WebP",
//!                  "accepts": ["file/image"],
//!                  "mock": { "latencyMs": 20, "steps": 3, "stepMs": 50,
//!                            "error": "unavailable", "reason": "FFmpeg isn't installed",
//!                            "crashAfterMs": 100, "result": { "message": "Converted" } } } ] }
//! ```
//!
//! With `"oneshot": true` the manifest advertises `launch.invoke`, so
//! headless actions of a stopped mock run as one-shot processes.
//!
//! Without `result`, the inputs are echoed back as outputs. `crashAfterMs`
//! exits the whole process mid-job. Every invocation is appended to
//! `$ARCADE_MOCK_LOG` (one JSON line each) when that variable is set.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use arcade_link::server::{Handler, InvokeContext, Reply};
use arcade_link::{manifest, Action, ErrorCode, InvokeRequest, InvokeResult, LinkError, Locations, Manifest, Presence};
use serde_json::{json, Value};

struct Mock {
    actions: Vec<(Action, Value)>,
    busy: bool,
}

impl Mock {
    fn behavior(&self, req: &InvokeRequest) -> Option<(&Action, &Value)> {
        let with_preset = req.preset.as_ref().map(|p| format!("{}#{p}", req.action));
        self.actions
            .iter()
            .find(|(a, _)| Some(&a.id) == with_preset.as_ref())
            .or_else(|| self.actions.iter().find(|(a, _)| a.id == req.action))
            .map(|(a, b)| (a, b))
    }
}

fn log(req: &InvokeRequest) {
    if let Some(path) = std::env::var_os("ARCADE_MOCK_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}", serde_json::to_string(req).unwrap_or_default());
        }
    }
}

impl Handler for Mock {
    fn describe(&self) -> Vec<Action> {
        self.actions.iter().map(|(a, _)| a.clone()).collect()
    }

    fn invoke(&self, req: InvokeRequest, ctx: &InvokeContext) -> Result<Reply, LinkError> {
        log(&req);
        let (action, b) = self.behavior(&req).ok_or_else(|| LinkError::unavailable(format!("no action {}", req.action)))?;
        if !action.available {
            return Err(LinkError::unavailable(action.reason.clone().unwrap_or_default()));
        }
        if !req.inputs.is_empty() && !action.accepts.is_empty() && !req.inputs.iter().all(|c| arcade_link::content::accepts_content(&action.accepts, c)) {
            return Err(LinkError::unsupported("the mock does not accept this input"));
        }
        let latency = b.get("latencyMs").and_then(Value::as_u64).unwrap_or(0);
        std::thread::sleep(Duration::from_millis(latency));
        if let Some(code) = b.get("error").cloned() {
            let code: ErrorCode = serde_json::from_value(code).unwrap_or(ErrorCode::Internal);
            let mut e = LinkError::new(code, b.get("reason").and_then(Value::as_str).unwrap_or("mock error"));
            e.reason = b.get("reason").and_then(Value::as_str).map(String::from);
            e.limit = b.get("limit").and_then(Value::as_u64);
            return Err(e);
        }
        let result: InvokeResult = match b.get("result") {
            Some(r) => serde_json::from_value(r.clone()).unwrap_or_default(),
            None => InvokeResult { outputs: req.inputs.clone(), message: Some(format!("{} done", action.title)), data: None },
        };
        let steps = b.get("steps").and_then(Value::as_u64).unwrap_or(0);
        let crash = b.get("crashAfterMs").and_then(Value::as_u64);
        if steps == 0 && crash.is_none() {
            return Ok(Reply::Done(result));
        }
        let step_ms = b.get("stepMs").and_then(Value::as_u64).unwrap_or(50);
        let job = ctx.start_job();
        let ticket = job.ticket();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            for i in 0..steps.max(1) {
                if let Some(c) = crash {
                    if started.elapsed() >= Duration::from_millis(c) {
                        std::process::exit(3);
                    }
                }
                if job.is_cancelled() {
                    return job.finish(Err(LinkError::cancelled()));
                }
                job.progress(Some(i as f32 / steps.max(1) as f32), &format!("step {}", i + 1));
                std::thread::sleep(Duration::from_millis(step_ms));
            }
            if crash.is_some() {
                std::process::exit(3);
            }
            job.finish(Ok(result));
        });
        Ok(Reply::Job(ticket))
    }

    fn status(&self) -> Value {
        json!({ "mock": true })
    }

    fn activate(&self) -> Result<(), LinkError> {
        Ok(())
    }

    fn quit(&self) -> Result<(), LinkError> {
        if self.busy {
            return Err(LinkError::busy());
        }
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            std::process::exit(0);
        });
        Ok(())
    }
}

pub fn run(loc: &Locations, args: &[String]) -> Result<(), String> {
    let as_id = super::value(args, "--as").ok_or("mock needs --as <id>")?;
    let file = super::value(args, "--actions").ok_or("mock needs --actions <file.json>")?;
    let path = std::fs::canonicalize(file).map_err(|e| format!("{file}: {e}"))?;
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?).map_err(|e| format!("{file}: {e}"))?;
    let id = super::app_id(as_id);
    let mut actions = Vec::new();
    for a in fixture.get("actions").and_then(Value::as_array).cloned().unwrap_or_default() {
        let behavior = a.get("mock").cloned().unwrap_or(json!({}));
        let action: Action = serde_json::from_value(a).map_err(|e| format!("{file}: {e}"))?;
        actions.push((action, behavior));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut m = Manifest::new(&id, fixture.get("version").and_then(Value::as_str).unwrap_or("0.0.0-mock"), &exe.to_string_lossy());
    if let Some(n) = fixture.get("name").and_then(Value::as_str) {
        m.name = n.into();
    }
    let base = vec!["mock".to_string(), "--as".into(), id.clone(), "--actions".into(), path.to_string_lossy().into_owned()];
    m.launch.background = base.clone();
    if fixture.get("oneshot") == Some(&json!(true)) {
        m.launch.invoke = Some([base, vec![arcade_link::oneshot::FLAG.to_string()]].concat());
    }
    if let Some(s) = fixture.get("shortcuts") {
        m.shortcuts = serde_json::from_value(s.clone()).unwrap_or_default();
    }
    if fixture.get("linkEnabled") == Some(&json!(false)) {
        m.settings.link_enabled = false;
    }
    m.actions = actions.iter().map(|(a, _)| a.clone()).collect();
    let busy = fixture.get("busy").and_then(Value::as_bool).unwrap_or(false);
    let handler = Arc::new(Mock { actions, busy });
    if super::flag(args, arcade_link::oneshot::FLAG) {
        // One-shot: no manifest, no listener; one request on stdin.
        std::process::exit(arcade_link::oneshot::serve(&*handler));
    }
    let presence = Presence::start(loc.clone(), m, handler);
    if let Some(e) = presence.last_error() {
        return Err(e);
    }
    eprintln!("mock {id} listening ({} in {})", Path::new(file).display(), loc.runtime.display());
    let _ = manifest::now_rfc3339();
    loop {
        std::thread::park();
    }
}
