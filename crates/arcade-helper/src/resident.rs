//! The resident helper (`arcade-helper --background`, started at login by Arcade Tools): a Link app with no
//! window. It owns every job started from a menu or the browser, keeps the menu entries current, and starts the
//! progress window when there is something to show. Windows run in their own short-lived processes, so the
//! resident stays small and a closed window never cancels a job.
//!
//! Its card's actions are internal: they accept `structured/arcade-helper`, which no selection ever is, so no
//! menu offers them. Callers use them through [`crate::client`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use arcade_link::catalog::Catalog;
use arcade_link::client::{self, CallOptions};
use arcade_link::content::Content;
use arcade_link::manifest::{Action, Manifest};
use arcade_link::offers::Prefs;
use arcade_link::registry::SharedRegistry;
use arcade_link::server::{Handler, InvokeContext, Reply};
use arcade_link::wire::{InvokeRequest, InvokeResult, PeerInfo};
use arcade_link::{LinkError, Locations};

use crate::ids;
use crate::integrations::{self, Home};
use crate::jobs::{self, Origin, Target};
use crate::pins;

/// File-menu clicks on the same action this close together become one job (Explorer starts one command per file).
pub const BATCH_WINDOW: Duration = Duration::from_millis(400);
/// The longest a `helper.jobs` call waits for a change.
pub const MAX_WAIT: Duration = Duration::from_secs(25);
/// The type the helper's own actions accept.
pub const INTERNAL_TYPE: &str = "structured/arcade-helper";

/// What one job is, as the progress window and the browser see it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub id: u64,
    pub reference: String,
    pub title: String,
    pub app: String,
    /// `queued`, `ask` (waiting for a folder), `running`, `done`, `failed`, `cancelled`.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fraction: Option<f32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    pub inputs: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    pub origin: String,
}

impl JobView {
    pub fn finished(&self) -> bool {
        matches!(self.status.as_str(), "done" | "failed" | "cancelled")
    }
}

/// A `helper.run` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunSpec {
    pub reference: String,
    pub paths: Vec<String>,
    /// Ready-made inputs (the browser's text, links and downloaded files), used instead of `paths`.
    pub inputs: Vec<Content>,
    pub values: Map<String, Value>,
    /// `file-menu` (default) or `browser`.
    pub origin: String,
}

impl RunSpec {
    fn origin(&self) -> Origin {
        if self.origin == "browser" {
            Origin::Browser
        } else {
            Origin::FileMenu
        }
    }
}

struct Record {
    view: JobView,
    spec: RunSpec,
    queued_at: Instant,
    cancel: Arc<AtomicBool>,
    answer: Option<Option<PathBuf>>,
}

#[derive(Default)]
struct Store {
    jobs: Vec<Record>,
    next: u64,
    revision: u64,
    /// Progress windows waiting in `helper.jobs` right now, and when one last asked.
    ui_waiting: usize,
    ui_seen: Option<Instant>,
}

/// Shared between the Link handler and the worker.
pub struct Resident {
    locations: Locations,
    home: Home,
    exe: PathBuf,
    version: String,
    registry: SharedRegistry,
    store: Mutex<Store>,
    changed: Condvar,
    work: Condvar,
    /// Started by tests without windows.
    pub spawn_windows: AtomicBool,
}

impl Resident {
    pub fn new(locations: Locations, home: Home, exe: PathBuf, version: &str) -> Arc<Resident> {
        Arc::new(Resident {
            registry: SharedRegistry::load(&locations),
            locations,
            home,
            exe,
            version: version.into(),
            store: Mutex::new(Store::default()),
            changed: Condvar::new(),
            work: Condvar::new(),
            spawn_windows: AtomicBool::new(true),
        })
    }

    fn me(&self) -> PeerInfo {
        PeerInfo { id: ids::HELPER_ID.into(), version: self.version.clone() }
    }

    /// The helper's own card.
    pub fn manifest(&self) -> Manifest {
        let mut m = Manifest::new(ids::HELPER_ID, &self.version, &self.exe.to_string_lossy());
        m.name = "Arcade helper".into();
        m.launch.background = vec!["--background".into()];
        m.actions = internal_actions();
        m
    }

    fn bump(&self, store: &mut Store) {
        store.revision += 1;
        self.changed.notify_all();
        self.work.notify_all();
    }

    /// The menu entries and browser host files for the current cards, catalog rules and preferences.
    pub fn refresh_integrations(&self) -> std::io::Result<integrations::Applied> {
        self.registry.refresh();
        let cards = self.registry.with(|r| r.apps().to_vec());
        let catalog = Catalog::load(&self.locations);
        let prefs = Prefs::load(&self.locations);
        let entries = pins::entries(&cards, &catalog.rules, &prefs);
        let plan = integrations::plan_for_this_os(&self.exe, &entries, &self.home);
        integrations::apply(&plan, &state_path(&self.locations))
    }

    /// Queues a run, or adds its files to a file-menu job on the same action queued a moment ago.
    pub fn queue(&self, spec: RunSpec) -> Result<u64, LinkError> {
        let (app, _, _) = jobs::parse_reference(&spec.reference).ok_or_else(|| LinkError::denied("not an action reference"))?;
        if spec.paths.is_empty() && spec.inputs.is_empty() {
            return Err(LinkError::denied("nothing to run on"));
        }
        let catalog = Catalog::load(&self.locations);
        let title =
            self.registry.with(|r| r.get(app).and_then(|m| action_of(m, &spec.reference)).map(|a| a.title.clone())).unwrap_or_else(|| spec.reference.clone());
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if spec.origin() == Origin::FileMenu && spec.inputs.is_empty() {
            if let Some(r) = store.jobs.iter_mut().find(|r| {
                r.view.status == "queued" && r.spec.reference == spec.reference && r.spec.origin() == Origin::FileMenu && r.queued_at.elapsed() < BATCH_WINDOW
            }) {
                r.spec.paths.extend(spec.paths);
                r.view.inputs = r.spec.paths.len();
                let id = r.view.id;
                self.bump(&mut store);
                return Ok(id);
            }
        }
        store.next += 1;
        let id = store.next;
        let view = JobView {
            id,
            reference: spec.reference.clone(),
            title,
            app: catalog.name(app),
            status: "queued".into(),
            inputs: spec.paths.len().max(spec.inputs.len()),
            origin: spec.origin().source().into(),
            ..JobView::default()
        };
        store.jobs.push(Record { view, spec, queued_at: Instant::now(), cancel: Arc::new(AtomicBool::new(false)), answer: None });
        let show = store.ui_waiting == 0 && store.ui_seen.is_none_or(|t| t.elapsed() > Duration::from_secs(3));
        self.bump(&mut store);
        drop(store);
        if show && self.spawn_windows.load(Ordering::Relaxed) {
            client::spawn_detached(&self.exe.to_string_lossy(), &["progress".to_string()]).ok();
        }
        Ok(id)
    }

    /// The jobs, after waiting up to `wait` for a revision newer than `since`. `ui`: the caller is the progress
    /// window (while one is waiting here, new jobs don't start another).
    pub fn jobs(&self, since: u64, wait: Duration, ui: bool) -> (u64, Vec<JobView>) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if ui {
            store.ui_waiting += 1;
        }
        let deadline = Instant::now() + wait.min(MAX_WAIT);
        while store.revision <= since {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            store = self.changed.wait_timeout(store, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        if ui {
            store.ui_waiting -= 1;
            store.ui_seen = Some(Instant::now());
        }
        (store.revision, store.jobs.iter().map(|r| r.view.clone()).collect())
    }

    pub fn cancel(&self, id: u64) -> bool {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let Some(r) = store.jobs.iter_mut().find(|r| r.view.id == id && !r.view.finished()) else { return false };
        r.cancel.store(true, Ordering::SeqCst);
        if matches!(r.view.status.as_str(), "queued" | "ask") {
            r.view.status = "cancelled".into();
            r.answer = Some(None);
        }
        self.bump(&mut store);
        true
    }

    /// The folder the user picked for a job that asked (`None`: cancel it).
    pub fn answer(&self, id: u64, folder: Option<PathBuf>) -> bool {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let Some(r) = store.jobs.iter_mut().find(|r| r.view.id == id && r.view.status == "ask") else { return false };
        r.answer = Some(folder);
        self.bump(&mut store);
        true
    }

    /// Forgets finished jobs (`None`: all of them).
    pub fn dismiss(&self, id: Option<u64>) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        store.jobs.retain(|r| !(r.view.finished() && id.is_none_or(|i| i == r.view.id)));
        self.bump(&mut store);
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut JobView)) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = store.jobs.iter_mut().find(|r| r.view.id == id) {
            f(&mut r.view);
        }
        self.bump(&mut store);
    }

    /// The next job to start: the oldest queued one whose batch window has passed.
    fn next_job(&self) -> (u64, RunSpec, Arc<AtomicBool>) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let ready =
                store.jobs.iter_mut().find(|r| r.view.status == "queued" && (r.spec.origin() != Origin::FileMenu || r.queued_at.elapsed() >= BATCH_WINDOW));
            if let Some(r) = ready {
                r.view.status = "running".into();
                let out = (r.view.id, r.spec.clone(), r.cancel.clone());
                self.bump(&mut store);
                return out;
            }
            let wait = if store.jobs.iter().any(|r| r.view.status == "queued") { BATCH_WINDOW / 4 } else { Duration::from_secs(3600) };
            store = self.work.wait_timeout(store, wait).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    fn wait_for_folder(&self, id: u64) -> Option<PathBuf> {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(r) = store.jobs.iter_mut().find(|r| r.view.id == id) {
            r.view.status = "ask".into();
        }
        self.bump(&mut store);
        loop {
            let r = store.jobs.iter_mut().find(|r| r.view.id == id)?;
            if let Some(answer) = r.answer.take() {
                if answer.is_some() {
                    r.view.status = "running".into();
                }
                self.bump(&mut store);
                return answer;
            }
            store = self.changed.wait(store).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Runs jobs one at a time, forever (the worker thread).
    pub fn work_forever(self: &Arc<Self>) {
        loop {
            let (id, spec, cancel) = self.next_job();
            let outcome = self.run(id, &spec, &cancel);
            self.update(id, |v| match outcome {
                Ok(placed) => {
                    v.status = "done".into();
                    v.fraction = Some(1.0);
                    v.files = placed.files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                    v.texts = placed.texts;
                }
                Err(e) if cancel.load(Ordering::SeqCst) || e.is_empty() => {
                    v.status = "cancelled".into();
                }
                Err(e) => {
                    v.status = "failed".into();
                    v.message = e;
                }
            });
        }
    }

    /// One job: every run it needs, results placed. `Err("")` means cancelled.
    pub fn run(&self, id: u64, spec: &RunSpec, cancel: &AtomicBool) -> Result<jobs::Placed, String> {
        let (app, _, _) = jobs::parse_reference(&spec.reference).ok_or("not an action reference")?;
        self.registry.refresh();
        let catalog = Catalog::load(&self.locations);
        let Some(card) = self.registry.with(|r| r.get(app).cloned()) else {
            return Err(format!("{} isn't installed.", catalog.name(app)));
        };
        let Some(action) = action_of(&card, &spec.reference).cloned() else {
            return Err(format!("{} no longer offers this. Update it from Arcade Tools.", catalog.name(app)));
        };
        let paths: Vec<PathBuf> = spec.paths.iter().map(PathBuf::from).collect();
        let runs = if spec.inputs.is_empty() { jobs::runs_for(&action, &paths) } else { vec![spec.inputs.clone()] };
        let prefs = Prefs::load(&self.locations);
        let dir = match jobs::target(&action, &prefs, spec.origin(), paths.first().map(PathBuf::as_path)) {
            Target::Folder(d) => Some(d),
            Target::Keep => None,
            Target::Ask => match self.wait_for_folder(id) {
                Some(d) => Some(d),
                None => return Err(String::new()),
            },
        };
        self.update(id, |v| v.folder = dir.as_ref().map(|d| d.to_string_lossy().into_owned()));
        let mut placed = jobs::Placed::default();
        let total = runs.len().max(1) as f32;
        for (i, inputs) in runs.into_iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                return Err(String::new());
            }
            let request = jobs::request(&spec.reference, &action, inputs, &spec.values, spec.origin())?;
            let mut on_progress = |p: &arcade_link::wire::JobProgress| {
                let f = (i as f32 + p.fraction.unwrap_or(0.0)) / total;
                let message = p.message.clone();
                self.update(id, |v| {
                    v.fraction = Some(f);
                    v.message = message;
                });
            };
            let result = client::invoke_action(
                &self.locations,
                &self.me(),
                &card,
                &request,
                CallOptions { on_progress: Some(&mut on_progress), cancel: Some(cancel), on_launching: None },
            )
            .map_err(|e| if cancel.load(Ordering::SeqCst) { String::new() } else { e.user_message(&catalog.name(app)) })?;
            let more = jobs::place(&result.outputs, dir.as_deref()).map_err(|e| format!("Couldn't save the result: {e}"))?;
            placed.files.extend(more.files);
            placed.texts.extend(more.texts);
            if let Some(m) = result.message {
                placed.texts.push(m);
            }
            self.update(id, |v| v.fraction = Some((i as f32 + 1.0) / total));
        }
        Ok(placed)
    }
}

/// Keeps the menu entries current: rewrites them when an app's card changes, or the catalog or the user's choices
/// do (both live next to the registry folder). Events are coalesced; nothing is polled.
///
/// When its own card goes, the helper was uninstalled (`integrate --remove` takes the entries away, then the card):
/// it stops instead of writing the entries back.
pub fn watch(r: Arc<Resident>) {
    use notify::Watcher as _;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let on_cards = tx.clone();
    r.registry.watch(move |_| {
        on_cards.send(()).ok();
    });
    let parent = r.locations.registry.parent().map(Path::to_path_buf);
    let watcher = parent.and_then(|dir| {
        let tx = tx.clone();
        let mut w = notify::recommended_watcher(move |e: notify::Result<notify::Event>| {
            if let Ok(e) = e {
                if e.paths.iter().any(|p| p.file_name().is_some_and(|n| n == "catalog.json" || n == "offers.json")) {
                    tx.send(()).ok();
                }
            }
        })
        .ok()?;
        w.watch(&dir, notify::RecursiveMode::NonRecursive).ok()?;
        Some(w)
    });
    std::thread::spawn(move || {
        let _keep = watcher;
        while rx.recv().is_ok() {
            std::thread::sleep(Duration::from_millis(300));
            while rx.try_recv().is_ok() {}
            if !r.locations.manifest(ids::HELPER_ID).exists() {
                leave(&r.locations);
            }
            if let Err(e) = r.refresh_integrations() {
                eprintln!("arcade-helper: menu entries: {e}");
            }
        }
    });
}

/// The card action a reference names (`box:x#webp` first as written, then without the preset).
pub fn action_of<'a>(card: &'a Manifest, reference: &str) -> Option<&'a Action> {
    let (_, action) = reference.split_once('/')?;
    card.action(action).or_else(|| card.action(action.split('#').next()?))
}

/// Where the helper remembers which menu entries it wrote.
pub fn state_path(locations: &Locations) -> PathBuf {
    let parent = locations.registry.parent().map(Path::to_path_buf).unwrap_or_else(|| locations.registry.join(".."));
    parent.join("helper").join("integrations.json")
}

fn internal_actions() -> Vec<Action> {
    ["helper.run", "helper.jobs", "helper.cancel", "helper.answer", "helper.dismiss", "helper.integrate"]
        .iter()
        .map(|id| {
            let mut a = Action::new(id, id, "");
            a.accepts = vec![INTERNAL_TYPE.into()];
            a.surfaces = vec!["internal".into()];
            a
        })
        .collect()
}

fn data(v: Value) -> Result<Reply, LinkError> {
    Ok(Reply::Done(InvokeResult { outputs: Vec::new(), message: None, data: Some(v) }))
}

/// The Link side of the resident.
pub struct ResidentHandler(pub Arc<Resident>);

impl Handler for ResidentHandler {
    fn describe(&self) -> Vec<Action> {
        internal_actions()
    }

    fn invoke(&self, request: InvokeRequest, _ctx: &InvokeContext) -> Result<Reply, LinkError> {
        let o = &request.options;
        let job = || o.get("job").and_then(Value::as_u64).ok_or_else(|| LinkError::denied("needs a job"));
        match request.action.as_str() {
            "helper.run" => {
                let spec: RunSpec = serde_json::from_value(o.clone()).map_err(|e| LinkError::denied(&e.to_string()))?;
                data(json!({"job": self.0.queue(spec)?}))
            }
            "helper.jobs" => {
                let since = o.get("since").and_then(Value::as_u64).unwrap_or(0);
                let wait = Duration::from_millis(o.get("waitMs").and_then(Value::as_u64).unwrap_or(0));
                let ui = o.get("ui").and_then(Value::as_bool).unwrap_or(false);
                let (revision, jobs) = self.0.jobs(since, wait, ui);
                data(json!({"revision": revision, "jobs": jobs}))
            }
            "helper.cancel" => data(json!({"cancelled": self.0.cancel(job()?)})),
            "helper.answer" => {
                let folder = o.get("folder").and_then(Value::as_str).filter(|f| !f.is_empty()).map(PathBuf::from);
                data(json!({"answered": self.0.answer(job()?, folder)}))
            }
            "helper.dismiss" => {
                self.0.dismiss(o.get("job").and_then(Value::as_u64));
                data(json!({}))
            }
            "helper.integrate" => {
                let done = self.0.refresh_integrations().map_err(|e| LinkError::internal(e.to_string()))?;
                data(json!({"written": done.written, "removed": done.removed}))
            }
            other => Err(LinkError::unavailable(format!("unknown helper action {other}"))),
        }
    }

    fn status(&self) -> Value {
        let store = self.0.store.lock().unwrap_or_else(|e| e.into_inner());
        let unfinished = store.jobs.iter().filter(|r| !r.view.finished()).count();
        // `busy`: Arcade Tools doesn't replace an app (or the helper) that says so.
        json!({"mode": "background", "jobs": unfinished, "busy": unfinished > 0})
    }

    /// Arcade Tools reads `busy` in the status before it replaces the helper, and quits it regardless when Tools
    /// itself is removed (a running program would keep its files from being deleted on Windows).
    fn quit(&self) -> Result<(), LinkError> {
        // The endpoint goes first, so whoever asked sees the helper gone at once on every OS.
        let locations = self.0.locations.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            leave(&locations);
        });
        Ok(())
    }
}

/// Takes this helper's endpoint away (not one a newer helper wrote meanwhile) and exits.
fn leave(locations: &Locations) -> ! {
    if let Ok(e) = arcade_link::endpoint::read(locations, ids::HELPER_ID) {
        if e.pid == std::process::id() {
            arcade_link::endpoint::remove_if_ours(locations, ids::HELPER_ID, &e.token);
        }
    }
    std::process::exit(0);
}
