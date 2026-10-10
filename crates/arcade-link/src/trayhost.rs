//! Live tray hosting; no actions route through Tools. All watcher callbacks
//! run on a background worker. Marshal them to your toolkit's UI thread.
//!
//! Start before constructing the tray: wait asynchronously for Own (construct
//! or show) or Hosted (keep constructed but hidden). None is pending/no tray.
//! Registry/endpoint OS notifications trigger connections; hosted connections
//! use one blocking read. Only startup (300 ms) and restart (5 s) have deadlines.

use crate::{
    client::{Client, ConnectionControl},
    endpoint,
    manifest::ids,
    paths::Locations,
    registry::{Registry, SharedRegistry},
    server::Sink,
    wire::{method, Message, PeerInfo},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{mpsc, Arc, Mutex, Weak},
    time::{Duration, Instant},
};

pub const STARTUP_GRACE: Duration = Duration::from_millis(300);
pub const RESTART_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayState {
    Own,
    Hosted,
    None,
}
impl TrayState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Own => "own",
            Self::Hosted => "hosted",
            Self::None => "none",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostState {
    pub hosted: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restarting: bool,
}
struct Subscriber {
    app: String,
    sink: Weak<dyn Sink>,
}
struct HostInner {
    hosted: bool,
    restarting: bool,
    excluded: HashSet<String>,
    subscribers: Vec<Subscriber>,
}
/// Tools-side helper attached with Server::attach_tray_host. Calls may write
/// IPC; use a worker thread. `shutdown` sends hosted:false and closes every
/// subscription; before a restart use announce_restarting then stop the server.
#[derive(Clone)]
pub struct TrayHostServer {
    inner: Arc<Mutex<HostInner>>,
}
impl TrayHostServer {
    pub fn new(hosted: bool) -> Self {
        Self { inner: Arc::new(Mutex::new(HostInner { hosted, restarting: false, excluded: HashSet::new(), subscribers: Vec::new() })) }
    }
    pub(crate) fn subscribe(&self, peer: &PeerInfo, sink: Arc<dyn Sink>) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.subscribers.retain(|s| s.sink.strong_count() > 0);
        if !inner.subscribers.iter().any(|s| s.sink.ptr_eq(&Arc::downgrade(&sink))) {
            inner.subscribers.push(Subscriber { app: peer.id.clone(), sink: Arc::downgrade(&sink) });
        }
        let hosted = inner.hosted && !inner.excluded.contains(&peer.id);
        sink.send(&notification(HostState { hosted, restarting: hosted && inner.restarting }));
    }
    pub(crate) fn unsubscribe(&self, sink: Arc<dyn Sink>) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).subscribers.retain(|s| !s.sink.ptr_eq(&Arc::downgrade(&sink)));
    }
    fn update(&self, change: impl FnOnce(&mut HostInner)) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut inner);
        inner.subscribers.retain(|s| s.sink.strong_count() > 0);
        for sub in &inner.subscribers {
            if let Some(sink) = sub.sink.upgrade() {
                let hosted = inner.hosted && !inner.excluded.contains(&sub.app);
                sink.send(&notification(HostState { hosted, restarting: hosted && inner.restarting }));
            }
        }
    }
    pub fn set_hosted(&self, hosted: bool) {
        self.update(|s| {
            s.hosted = hosted;
            s.restarting = false;
        });
    }
    pub fn set_excluded(&self, ids: &[String]) {
        self.update(|s| s.excluded = ids.iter().cloned().collect());
    }
    pub fn announce_restarting(&self) {
        self.update(|s| s.restarting = true);
    }
    pub fn subscribers(&self) -> Vec<String> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).subscribers.iter().filter(|s| s.sink.strong_count() > 0).map(|s| s.app.clone()).collect()
    }
    pub fn shutdown(&self) {
        self.set_hosted(false);
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for sub in inner.subscribers.drain(..) {
            if let Some(sink) = sub.sink.upgrade() {
                sink.close();
            }
        }
    }
}
fn notification(state: HostState) -> Message {
    Message::notification(method::TRAY_HOST, serde_json::to_value(state).expect("host state"))
}

#[derive(Debug, Clone)]
pub struct WatcherConfig {
    pub locations: Locations,
    pub app: PeerInfo,
    pub link_enabled: bool,
    pub has_tray: bool,
}
enum Event {
    Registry(Registry),
    Connected(u64, ConnectionControl),
    Host(u64, HostState),
    Closed(u64),
    Link(bool),
    Stop,
}
/// A nonblocking handle. Drop/stop wakes the worker and restores Own; no UI
/// thread joins an IPC reader. State can be inserted in Handler::status().
pub struct TrayHostWatcher {
    tx: mpsc::Sender<Event>,
    state: Arc<Mutex<TrayState>>,
}
impl TrayHostWatcher {
    pub fn start(config: WatcherConfig, on_change: impl Fn(TrayState) + Send + 'static) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(TrayState::None));
        let worker_tx = tx.clone();
        let worker_state = state.clone();
        let started = Instant::now();
        std::thread::Builder::new().name("arcade-tray-watch".into()).spawn(move || watch(config, worker_tx, rx, worker_state, started, on_change))?;
        Ok(Self { tx, state })
    }
    pub fn state(&self) -> TrayState {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn set_link_enabled(&self, enabled: bool) {
        let _ = self.tx.send(Event::Link(enabled));
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Event::Stop);
    }
}
impl Drop for TrayHostWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn candidate(config: &WatcherConfig, registry: &Registry) -> Option<String> {
    if !config.link_enabled || !config.has_tray || config.app.id == ids::TOOLS {
        return None;
    }
    let tools = registry.get(ids::TOOLS)?;
    let settings = registry.additions(ids::TOOLS)?.tray_host.as_ref()?;
    if !tools.settings.link_enabled || !settings.enabled || settings.excluded.contains(&config.app.id) {
        return None;
    }
    endpoint::read(&config.locations, ids::TOOLS).ok().map(|e| e.token)
}
fn explicitly_disabled(config: &WatcherConfig, registry: &Registry) -> bool {
    !config.link_enabled
        || !config.has_tray
        || registry.get(ids::TOOLS).is_some_and(|m| !m.settings.link_enabled)
        || registry.additions(ids::TOOLS).and_then(|a| a.tray_host.as_ref()).is_some_and(|s| !s.enabled || s.excluded.contains(&config.app.id))
}
fn close(control: &mut Option<ConnectionControl>) {
    if let Some(c) = control.take() {
        c.close();
    }
}

fn connect(config: &WatcherConfig, tx: mpsc::Sender<Event>, generation: u64, startup: Option<Instant>) {
    let config = config.clone();
    std::thread::spawn(move || {
        let result = (|| {
            let mut client = Client::connect(&config.locations, ids::TOOLS, &config.app)?;
            let control = client.connection_control();
            if tx.send(Event::Connected(generation, control)).is_err() {
                return Ok(());
            }
            let timeout = startup.map_or(STARTUP_GRACE, |d| d.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)));
            client.set_timeout(timeout)?;
            client.subscribe(&[method::TRAY_HOST])?;
            let mut first = true;
            loop {
                let timeout = if first {
                    Some(startup.map_or(STARTUP_GRACE, |d| d.saturating_duration_since(Instant::now()).max(Duration::from_millis(1))))
                } else {
                    None
                };
                let message = client.next_notification(timeout)?;
                if message.method.as_deref() != Some(method::TRAY_HOST) {
                    continue;
                }
                let host: HostState = serde_json::from_value(message.params().clone()).map_err(|e| crate::LinkError::internal(e.to_string()))?;
                first = false;
                if tx.send(Event::Host(generation, host)).is_err() {
                    return Ok(());
                }
            }
        })();
        let _: Result<(), crate::LinkError> = result;
        let _ = tx.send(Event::Closed(generation));
    });
}

fn watch(
    mut config: WatcherConfig,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    state: Arc<Mutex<TrayState>>,
    started: Instant,
    callback: impl Fn(TrayState),
) {
    let shared = SharedRegistry::load(&config.locations);
    let changes = tx.clone();
    let watching = shared.watch(move |r| {
        let _ = changes.send(Event::Registry(r.clone()));
    });
    let mut registry = shared.snapshot();
    let mut generation = 0;
    let mut attempted = None;
    let mut active = false;
    let mut control = None;
    let mut startup = Some(started + STARTUP_GRACE);
    let mut grace = None;
    let emit = |new: TrayState| {
        let mut current = state.lock().unwrap_or_else(|e| e.into_inner());
        if *current != new {
            *current = new;
            drop(current);
            callback(new);
        }
    };
    // Failure to watch means own tray, not a hidden icon with no recovery path.
    if !watching {
        emit(if config.has_tray { TrayState::Own } else { TrayState::None });
        return;
    }
    let mut pending = Some(Event::Registry(registry.clone()));
    loop {
        let event = if let Some(event) = pending.take() {
            Some(event)
        } else {
            let deadline = startup.into_iter().chain(grace).min();
            match deadline {
                Some(d) => rx.recv_timeout(d.saturating_duration_since(Instant::now())).ok(),
                None => rx.recv().ok(),
            }
        };
        match event {
            Some(Event::Stop) => {
                close(&mut control);
                emit(if config.has_tray { TrayState::Own } else { TrayState::None });
                break;
            }
            Some(Event::Link(enabled)) => {
                config.link_enabled = enabled;
                pending = Some(Event::Registry(registry.clone()));
            }
            Some(Event::Registry(r)) => {
                registry = r;
                let token = candidate(&config, &registry);
                if explicitly_disabled(&config, &registry) {
                    generation += 1;
                    active = false;
                    attempted = None;
                    close(&mut control);
                    grace = None;
                    startup = None;
                    emit(if config.has_tray { TrayState::Own } else { TrayState::None });
                } else if let Some(token) = token {
                    if !active && attempted.as_ref() != Some(&token) {
                        generation += 1;
                        close(&mut control);
                        active = true;
                        attempted = Some(token);
                        connect(&config, tx.clone(), generation, startup);
                    }
                } else if !active && grace.is_none() {
                    startup = None;
                    emit(if config.has_tray { TrayState::Own } else { TrayState::None });
                }
            }
            Some(Event::Connected(g, c)) if g == generation => control = Some(c),
            Some(Event::Connected(_, c)) => c.close(),
            Some(Event::Host(g, host)) if g == generation => {
                startup = None;
                if host.hosted {
                    emit(TrayState::Hosted);
                    if host.restarting {
                        grace.get_or_insert_with(|| Instant::now() + RESTART_GRACE);
                    } else {
                        grace = None;
                    }
                } else {
                    grace = None;
                    emit(TrayState::Own);
                }
            }
            Some(Event::Closed(g)) if g == generation => {
                active = false;
                close(&mut control);
                startup = None;
                if grace.is_none() {
                    emit(if config.has_tray { TrayState::Own } else { TrayState::None });
                }
                // Endpoint replacement may have arrived before EOF. Process
                // every buffered restart notification first, then reconnect.
                if candidate(&config, &registry).is_some_and(|t| attempted.as_ref() != Some(&t)) {
                    pending = Some(Event::Registry(registry.clone()));
                }
            }
            _ => {}
        }
        let now = Instant::now();
        if startup.is_some_and(|d| now >= d) {
            startup = None;
            emit(TrayState::Own);
        }
        if grace.is_some_and(|d| now >= d) {
            grace = None;
            emit(TrayState::Own);
        }
    }
}
