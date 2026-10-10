use arcade_link::{
    app::*,
    manifest::{MenuItem, MenuKind},
    wire::{method, Message},
    Action, Client, Content, ErrorCode, Handler, InvokeContext, InvokeRequest, InvokeResult, LinkError, Locations, PeerInfo, Reply, Server, ServerConfig,
};
use serde_json::json;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Default)]
struct App {
    new: bool,
    checked: Mutex<bool>,
    job: Mutex<Option<arcade_link::Job>>,
}
impl Handler for App {
    fn methods(&self) -> Vec<String> {
        if self.new {
            method::OPTIONAL.iter().map(|s| (*s).into()).collect()
        } else {
            vec![]
        }
    }
    fn describe(&self) -> Vec<Action> {
        vec![]
    }
    fn invoke(&self, _: InvokeRequest, ctx: &InvokeContext) -> Result<Reply, LinkError> {
        let job = ctx.start_job();
        let ticket = job.ticket();
        *self.job.lock().unwrap() = Some(job);
        Ok(Reply::Job(ticket))
    }
    fn settings(&self) -> Result<(), LinkError> {
        Ok(())
    }
    fn restart(&self, _: &str) -> Result<(), LinkError> {
        Ok(())
    }
    fn menu(&self) -> Result<Vec<MenuItem>, LinkError> {
        Ok(vec![MenuItem {
            id: "private".into(),
            title: "Private mode".into(),
            kind: MenuKind::Toggle,
            checked: Some(*self.checked.lock().unwrap()),
            enabled: true,
            effects: vec![],
        }])
    }
    fn menu_invoke(&self, id: &str) -> Result<(), LinkError> {
        if id != "private" {
            return Err(LinkError::unavailable("unknown id"));
        }
        let mut c = self.checked.lock().unwrap();
        *c = !*c;
        Ok(())
    }
    fn shortcuts_set(&self, id: &str, key: &str) -> Result<ShortcutSetResult, LinkError> {
        assert_eq!(key, "Ctrl+Shift+P");
        if id == "reserved" {
            return Err(LinkError::denied("reserved"));
        }
        if id == "missing" {
            return Err(LinkError::unavailable("no registrar"));
        }
        Ok(ShortcutSetResult { applied: true, via: ShortcutVia::Native, hint: None })
    }
    fn settings_export(&self) -> Result<Vec<Content>, LinkError> {
        Ok(vec![Content { kind: "file/any".into(), path: Some("/isolated/settings.json".into()), ..Default::default() }])
    }
    fn settings_import(&self, _: &[Content]) -> Result<ImportResult, LinkError> {
        Ok(ImportResult { imported: true, restart_required: false })
    }
}
#[test]
fn new_methods_opt_in_and_keep_old_helpers() {
    let root = std::env::temp_dir().join(format!("al-methods-{}", arcade_link::endpoint::new_token().unwrap()));
    let loc = Locations::under(&root);
    let app = Arc::new(App { new: true, ..Default::default() });
    let server =
        Server::start(ServerConfig { locations: loc.clone(), app: PeerInfo { id: "arcade.test".into(), version: "0.3".into() } }, app.clone()).unwrap();
    let mut c = Client::connect(&loc, "arcade.test", &PeerInfo::default()).unwrap();
    assert!(c.describe().unwrap().is_empty());
    let describe = c.describe_full().unwrap();
    for method in method::OPTIONAL {
        assert!(describe.supports(method));
    }
    c.settings().unwrap();
    c.restart(None).unwrap();
    c.subscribe(&["app.changed"]).unwrap();
    assert_eq!(c.menu().unwrap()[0].checked, Some(false));
    c.menu_invoke("private").unwrap();
    assert_eq!(c.menu().unwrap()[0].checked, Some(true));
    assert_eq!(c.next_notification(Some(Duration::from_secs(1))).unwrap().params()["menu"], true);
    assert!(c.shortcuts_set("toggle", "shift+Control+p").unwrap().applied);
    assert_eq!(c.shortcuts_set("reserved", "Ctrl+Shift+P").unwrap_err().code, ErrorCode::Denied);
    assert_eq!(c.shortcuts_set("missing", "Ctrl+Shift+P").unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(c.shortcuts_set("toggle", "Ctrl++").unwrap_err().code, ErrorCode::Denied);
    assert_eq!(c.restart(Some("invalid")).unwrap_err().code, ErrorCode::Denied);
    let files = c.settings_export().unwrap();
    assert!(c.settings_import(&files).unwrap().imported);
    assert_eq!(c.settings_import(&[Content::plain("not a file")]).unwrap_err().code, ErrorCode::Denied);
    c.call("invoke", json!(InvokeRequest::new("job", "test"))).unwrap();
    assert_eq!(c.restart(None).unwrap_err().code, ErrorCode::Busy);
    c.restart(Some("force")).unwrap();
    app.job.lock().unwrap().take().unwrap().finish(Ok(InvokeResult::default()));
    drop(c);
    drop(server);
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn an_old_handler_returns_clean_unsupported() {
    let root = std::env::temp_dir().join(format!("al-old-methods-{}", arcade_link::endpoint::new_token().unwrap()));
    let loc = Locations::under(&root);
    let server =
        Server::start(ServerConfig { locations: loc.clone(), app: PeerInfo { id: "arcade.old".into(), version: "0.2".into() } }, Arc::new(App::default()))
            .unwrap();
    let mut c = Client::connect(&loc, "arcade.old", &PeerInfo::default()).unwrap();
    assert!(c.describe_full().unwrap().methods.is_empty());
    for m in method::OPTIONAL {
        let error = c.call(m, json!({})).unwrap_err();
        assert!(error.is_unsupported());
        assert_eq!(serde_json::to_value(error).unwrap()["code"], "unsupported");
    }
    let old: LinkError = serde_json::from_value(json!({"code":"internal","message":"unknown method \"app.menu\""})).unwrap();
    assert!(old.is_unsupported());
    assert!(!LinkError::unsupported("unsupported input").is_unsupported());
    drop(c);
    drop(server);
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn method_vector_messages_round_trip() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors/methods.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for c in v["cases"].as_array().unwrap() {
        for k in ["request", "response"] {
            let m = Message::parse(&c[k].to_string()).unwrap();
            let parsed = Message::parse(&m.to_line()).unwrap();
            assert_eq!(parsed, m);
            if let Some(e) = parsed.error {
                if c[k]["error"]["code"] == "unsupported" {
                    assert!(e.is_unsupported())
                }
            }
        }
    }
}

#[test]
fn subscription_deadlines_and_close_before_or_during_read_are_bounded() {
    let root = std::env::temp_dir().join(format!("al-read-control-{}", arcade_link::endpoint::new_token().unwrap()));
    let loc = Locations::under(&root);
    let server =
        Server::start(ServerConfig { locations: loc.clone(), app: PeerInfo { id: "arcade.test".into(), version: "0.3".into() } }, Arc::new(App::default()))
            .unwrap();
    let mut c = Client::connect(&loc, "arcade.test", &PeerInfo::default()).unwrap();
    c.set_timeout(Duration::from_millis(50)).unwrap();
    assert_eq!(c.next_notification(Some(Duration::from_millis(20))).unwrap_err().code, ErrorCode::Timeout);
    assert!(c.describe().unwrap().is_empty(), "a timeout leaves the connection usable");
    c.connection_control().close();
    let before = std::time::Instant::now();
    assert!(c.next_notification(None).is_err(), "close before read must persist");
    assert!(before.elapsed() < Duration::from_secs(1));
    drop(c);
    let mut c = Client::connect(&loc, "arcade.test", &PeerInfo::default()).unwrap();
    let control = c.connection_control();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        tx.send(false).unwrap();
        tx.send(c.next_notification(None).is_err()).unwrap();
    });
    assert!(!rx.recv_timeout(Duration::from_secs(1)).unwrap());
    control.close();
    assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap());
    reader.join().unwrap();
    drop(control);
    drop(server);
    let _ = std::fs::remove_dir_all(root);
}
