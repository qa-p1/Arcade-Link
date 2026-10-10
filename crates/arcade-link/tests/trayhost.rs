#![cfg(feature = "trayhost")]
use arcade_link::{
    manifest::{self, ManifestDocument, TrayHostSettings},
    trayhost::*,
    Action, Client, Handler, InvokeContext, InvokeRequest, LinkError, Locations, Manifest, PeerInfo, Reply, Server, ServerConfig,
};
use std::{
    fs,
    path::PathBuf,
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("al-tray-{}", arcade_link::endpoint::new_token().unwrap()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct App;
impl Handler for App {
    fn describe(&self) -> Vec<Action> {
        vec![]
    }
    fn invoke(&self, _: InvokeRequest, _: &InvokeContext) -> Result<Reply, LinkError> {
        Err(LinkError::unsupported("no actions"))
    }
}
fn tools(loc: &Locations, attach: bool) -> (Server, TrayHostServer, ManifestDocument) {
    let s =
        Server::start(ServerConfig { app: PeerInfo { id: "arcade.tools".into(), version: "0.3.0".into() }, locations: loc.clone() }, Arc::new(App)).unwrap();
    let h = TrayHostServer::new(true);
    if attach {
        s.attach_tray_host(h.clone());
    }
    let mut m: ManifestDocument = Manifest::new("arcade.tools", "0.3.0", std::env::current_exe().unwrap().to_str().unwrap()).into();
    m.additions.tray_host = Some(TrayHostSettings { enabled: true, excluded: vec![] });
    manifest::write_document(loc, &m).unwrap();
    (s, h, m)
}
fn watcher(loc: &Locations, enabled: bool) -> (TrayHostWatcher, mpsc::Receiver<(TrayState, Instant)>) {
    let (tx, rx) = mpsc::channel();
    let w = TrayHostWatcher::start(
        WatcherConfig { locations: loc.clone(), app: PeerInfo { id: "arcade.find".into(), version: "0.3.0".into() }, link_enabled: enabled, has_tray: true },
        move |s| {
            let _ = tx.send((s, Instant::now()));
        },
    )
    .unwrap();
    (w, rx)
}
fn expect(rx: &mpsc::Receiver<(TrayState, Instant)>, state: TrayState, timeout: Duration) -> Instant {
    let (s, t) = rx.recv_timeout(timeout).unwrap();
    assert_eq!(s, state);
    t
}

#[test]
fn hosted_unhosted_excluded_link_off_and_host_started_later() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let (w, rx) = watcher(&loc, true);
    expect(&rx, TrayState::Own, Duration::from_secs(1));
    let (_server, host, mut doc) = tools(&loc, true);
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    host.set_hosted(false);
    expect(&rx, TrayState::Own, Duration::from_secs(1));
    host.set_hosted(true);
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    host.set_excluded(&["arcade.find".into()]);
    expect(&rx, TrayState::Own, Duration::from_secs(1));
    host.set_excluded(&[]);
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    doc.additions.tray_host.as_mut().unwrap().excluded = vec!["arcade.find".into()];
    manifest::write_document(&loc, &doc).unwrap();
    expect(&rx, TrayState::Own, Duration::from_secs(1));
    doc.additions.tray_host.as_mut().unwrap().excluded.clear();
    manifest::write_document(&loc, &doc).unwrap();
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    w.set_link_enabled(false);
    expect(&rx, TrayState::Own, Duration::from_secs(1));
    w.set_link_enabled(true);
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    host.shutdown();
    expect(&rx, TrayState::Own, Duration::from_secs(1));
}
#[test]
fn dropped_server_hands_back_within_one_second() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let (server, _host, _doc) = tools(&loc, true);
    let (_w, rx) = watcher(&loc, true);
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    let start = Instant::now();
    drop(server);
    let own = expect(&rx, TrayState::Own, Duration::from_secs(1));
    assert!(own.duration_since(start) < Duration::from_secs(1));
    println!("server drop → own: {:?}", own.duration_since(start));
}
#[test]
fn restarting_returns_without_a_flash() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let (server, host, _doc) = tools(&loc, true);
    let (_w, rx) = watcher(&loc, true);
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    host.announce_restarting();
    drop(server);
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    let (_successor, _host, _doc) = tools(&loc, true);
    assert!(rx.recv_timeout(Duration::from_millis(400)).is_err(), "no own/hosted flash during restart");
}
#[test]
fn restarting_expires_after_five_seconds() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let (server, host, _doc) = tools(&loc, true);
    let (_w, rx) = watcher(&loc, true);
    expect(&rx, TrayState::Hosted, Duration::from_secs(1));
    let start = Instant::now();
    host.announce_restarting();
    drop(server);
    assert!(rx.recv_timeout(Duration::from_millis(4700)).is_err());
    let own = expect(&rx, TrayState::Own, Duration::from_secs(1));
    let elapsed = own.duration_since(start);
    assert!((Duration::from_millis(4900)..Duration::from_millis(5500)).contains(&elapsed));
    println!("restart grace: {elapsed:?}");
}
#[test]
fn startup_gate_shows_by_three_hundred_ms() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let (_server, _host, _doc) = tools(&loc, false);
    let start = Instant::now();
    let (_w, rx) = watcher(&loc, true);
    let own = expect(&rx, TrayState::Own, Duration::from_secs(1));
    let elapsed = own.duration_since(start);
    // Deadline is exactly 300 ms; permit scheduler delivery jitter in CI.
    assert!(elapsed <= Duration::from_millis(350), "{elapsed:?}");
    println!("startup gate: {elapsed:?}");
}
#[test]
fn tools_already_hosting_emits_no_initial_own() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let (_server, _host, _doc) = tools(&loc, true);
    let start = Instant::now();
    let (_w, rx) = watcher(&loc, true);
    expect(&rx, TrayState::Hosted, Duration::from_millis(300));
    assert!(start.elapsed() < STARTUP_GRACE);
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
}
#[test]
fn excluded_subscriber_gets_false_immediately_and_unsubscribe_stops_events() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let (_server, host, _doc) = tools(&loc, true);
    host.set_excluded(&["arcade.find".into()]);
    let mut c = Client::connect(&loc, "arcade.tools", &PeerInfo { id: "arcade.find".into(), version: "1".into() }).unwrap();
    c.subscribe(&["tray.host"]).unwrap();
    let m = c.next_notification(Some(Duration::from_secs(1))).unwrap();
    assert_eq!(m.params()["hosted"], false);
    c.subscribe(&[]).unwrap();
    host.set_hosted(false);
    assert!(c.next_notification(Some(Duration::from_millis(50))).is_err());
    assert!(host.subscribers().is_empty());
}
#[test]
fn vector_deadlines_pin_the_contract() {
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors/trayhost.json")).unwrap()).unwrap();
    assert_eq!(v["startupCapMs"].as_u64().unwrap(), STARTUP_GRACE.as_millis() as u64);
    assert_eq!(v["restartGraceMs"].as_u64().unwrap(), RESTART_GRACE.as_millis() as u64);
    assert_eq!(v["sequences"].as_array().unwrap().len(), 7);
}
