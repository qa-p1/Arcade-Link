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
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arcade_link::server::{Handler, InvokeContext, Reply};
use arcade_link::{manifest, Action, ErrorCode, InvokeRequest, InvokeResult, LinkError, Locations, Manifest, Presence};
use serde_json::{json, Value};

struct Mock {
    actions: Vec<(Action, Value)>,
    busy: bool,
    document: Mutex<manifest::ManifestDocument>,
    locations: Locations,
    script: Value,
    methods: Vec<String>,
}

impl Mock {
    fn scripted(&self, method: &str) -> Result<Option<Value>, LinkError> {
        let script = &self.script[method];
        if let Some(error) = script.get("error") {
            return Err(serde_json::from_value(
                json!({"code": error, "message": script["reason"].as_str().unwrap_or("mock error"), "reason": script["reason"]}),
            )
            .unwrap_or_else(|_| LinkError::internal("invalid mock error")));
        }
        Ok(script.get("result").cloned())
    }
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
    fn methods(&self) -> Vec<String> {
        self.methods.clone()
    }
    fn settings(&self) -> Result<(), LinkError> {
        self.scripted("app.settings")?;
        Ok(())
    }
    fn restart(&self, _: &str) -> Result<(), LinkError> {
        self.scripted("app.restart")?;
        Ok(())
    }
    fn restart_ready(&self, mode: &str) -> Result<(), LinkError> {
        if self.busy && mode != "force" {
            return Err(LinkError::busy());
        }
        self.scripted("app.restart")?;
        Ok(())
    }
    fn menu(&self) -> Result<Vec<manifest::MenuItem>, LinkError> {
        if let Some(r) = self.scripted("app.menu")? {
            return serde_json::from_value(r["items"].clone()).map_err(|e| LinkError::internal(e.to_string()));
        }
        Ok(self.document.lock().unwrap().additions.menu.clone())
    }
    fn menu_invoke(&self, id: &str) -> Result<(), LinkError> {
        self.scripted("app.menu.invoke")?;
        let mut doc = self.document.lock().unwrap();
        let item = doc.additions.menu.iter_mut().find(|s| s.id == id).ok_or_else(|| LinkError::unavailable("unknown menu id"))?;
        if !item.enabled || item.kind == manifest::MenuKind::Separator {
            return Err(LinkError::denied("disabled_menu_item"));
        }
        if item.kind == manifest::MenuKind::Toggle {
            item.checked = Some(!item.checked.unwrap_or(false));
        }
        manifest::write_document(&self.locations, &doc)?;
        Ok(())
    }
    fn shortcuts_set(&self, id: &str, accelerator: &str) -> Result<arcade_link::app::ShortcutSetResult, LinkError> {
        let result = self.scripted("app.shortcuts.set")?.unwrap_or(json!({"applied": true, "via": "native"}));
        let result: arcade_link::app::ShortcutSetResult = serde_json::from_value(result).map_err(|e| LinkError::internal(e.to_string()))?;
        let mut doc = self.document.lock().unwrap();
        let shortcut = doc.manifest.shortcuts.iter_mut().find(|s| s.id == id).ok_or_else(|| LinkError::unavailable("unknown shortcut id"))?;
        if result.applied {
            shortcut.accelerator = accelerator.into();
            manifest::write_document(&self.locations, &doc)?;
        }
        Ok(result)
    }
    fn settings_export(&self) -> Result<Vec<arcade_link::Content>, LinkError> {
        if let Some(result) = self.scripted("app.settings.export")? {
            return serde_json::from_value(result["outputs"].clone()).map_err(|e| LinkError::internal(e.to_string()));
        }
        let handoff = arcade_link::Handoff::create(&self.locations, &self.document.lock().unwrap().manifest.id)?;
        let mut file = handoff.file("settings.json", b"{\"schema\":1,\"portable\":true}")?;
        file.kind = "file/any".into();
        handoff.keep();
        Ok(vec![file])
    }
    fn settings_import(&self, inputs: &[arcade_link::Content]) -> Result<arcade_link::app::ImportResult, LinkError> {
        if let Some(result) = self.scripted("app.settings.import")? {
            return serde_json::from_value(result).map_err(|e| LinkError::internal(e.to_string()));
        }
        let mut documents = Vec::new();
        for file in inputs {
            let path = file.path.as_deref().ok_or_else(|| LinkError::denied("invalid_inputs"))?;
            let text = std::fs::read_to_string(path)?;
            let doc: Value = serde_json::from_str(&text).map_err(|_| LinkError::denied("invalid_settings"))?;
            if doc["schema"] != json!(1) || !doc.is_object() {
                return Err(LinkError::denied("invalid_settings"));
            }
            documents.push(doc);
        }
        let dest = self.locations.handoff.join("mock-imported-settings.json");
        if dest.exists() {
            std::fs::copy(&dest, dest.with_extension("backup.json"))?;
        }
        arcade_link::paths::write_atomic(&dest, &serde_json::to_vec(&documents).map_err(|e| LinkError::internal(e.to_string()))?, true)?;
        Ok(arcade_link::app::ImportResult { imported: true, restart_required: false })
    }
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
        json!({ "mock": true, "tray": "own" })
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
    let mut document: manifest::ManifestDocument = m.into();
    document.additions = serde_json::from_value(fixture.clone()).map_err(|e| e.to_string())?;
    let tray_settings = fixture.get("settings").and_then(|v| v.get("trayHost"));
    if let Some(s) = tray_settings {
        document.additions.tray_host = Some(serde_json::from_value(s.clone()).map_err(|e| e.to_string())?);
    }
    let methods = fixture
        .get("methods")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| arcade_link::wire::method::OPTIONAL.iter().map(|s| (*s).into()).collect());
    let handler =
        Arc::new(Mock { actions, busy, document: Mutex::new(document.clone()), locations: loc.clone(), script: fixture["mockMethods"].clone(), methods });
    if super::flag(args, arcade_link::oneshot::FLAG) {
        // One-shot: no manifest, no listener; one request on stdin.
        std::process::exit(arcade_link::oneshot::serve(&*handler));
    }
    if let Some(settings) = document.additions.tray_host.clone() {
        let server = arcade_link::Server::start(
            arcade_link::ServerConfig { locations: loc.clone(), app: arcade_link::PeerInfo { id: id.clone(), version: document.manifest.version.clone() } },
            handler,
        )
        .map_err(|e| e.to_string())?;
        let host = arcade_link::trayhost::TrayHostServer::new(settings.enabled);
        host.set_excluded(&settings.excluded);
        server.attach_tray_host(host.clone());
        manifest::write_document(loc, &document).map_err(|e| e.to_string())?;
        eprintln!("mock {id} tray host ready");
        super::tray::commands(loc, &host, &mut document)?;
        host.shutdown();
        server.stop();
        return Ok(());
    }
    let presence = Presence::start_document(loc.clone(), document, handler);
    if let Some(e) = presence.last_error() {
        return Err(e);
    }
    eprintln!("mock {id} listening ({} in {})", Path::new(file).display(), loc.runtime.display());
    let _ = manifest::now_rfc3339();
    loop {
        std::thread::park();
    }
}
