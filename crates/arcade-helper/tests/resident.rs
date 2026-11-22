//! The resident helper against a real Link server standing in for an app: jobs run, results are placed next to the
//! input without overwriting, quick clicks are batched, and asking for a folder waits for the answer.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arcade_helper::integrations::Home;
use arcade_helper::resident::{Resident, RunSpec};
use arcade_link::content::Content;
use arcade_link::manifest::{write_manifest, Action, Manifest};
use arcade_link::server::{Handler, InvokeContext, Reply, Server, ServerConfig};
use arcade_link::wire::{InvokeRequest, InvokeResult, PeerInfo};
use arcade_link::{LinkError, Locations};

/// A fake "Arcade Paint" that writes `<input>.out` into its own folder and records what it was asked.
struct Paint {
    out: PathBuf,
    seen: Mutex<Vec<InvokeRequest>>,
}

impl Handler for Paint {
    fn describe(&self) -> Vec<Action> {
        vec![paint_action()]
    }
    fn invoke(&self, request: InvokeRequest, _ctx: &InvokeContext) -> Result<Reply, LinkError> {
        self.seen.lock().unwrap().push(request.clone());
        let mut outputs = Vec::new();
        for c in &request.inputs {
            for p in c.path.iter().chain(&c.paths) {
                let name = format!("{}.out", Path::new(p).file_name().unwrap().to_string_lossy());
                let dest = self.out.join(name);
                std::fs::write(&dest, b"painted").unwrap();
                outputs.push(Content::file(&dest));
            }
        }
        Ok(Reply::Done(InvokeResult { outputs, message: None, data: None }))
    }
}

fn paint_action() -> Action {
    serde_json::from_value(json!({"id": "paint.invert", "title": "Invert colors", "accepts": ["file/image", "file/image[]"], "oneClick": true,
        "options": {"version": 1, "controls": [{"key": "strength", "label": "Strength", "type": "number", "default": 5}]}}))
    .unwrap()
}

struct World {
    root: PathBuf,
    locations: Locations,
    resident: Arc<Resident>,
    paint: Arc<Paint>,
    _server: Server,
}

fn world(name: &str) -> World {
    let root = std::env::temp_dir().join(format!("arcade-helper-e2e-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let locations = Locations::under(&root);
    for d in [&locations.registry, &locations.runtime, &locations.handoff] {
        std::fs::create_dir_all(d).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let out = root.join("paint-output");
    std::fs::create_dir_all(&out).unwrap();
    let paint = Arc::new(Paint { out, seen: Mutex::new(Vec::new()) });
    // The registry only lists apps whose program exists; the test's own binary stands in for it.
    let exe = std::env::current_exe().unwrap();
    let mut m = Manifest::new("arcade.paint", "1.0.0", &exe.to_string_lossy());
    m.name = "Arcade Paint".into();
    m.actions = vec![paint_action()];
    write_manifest(&locations, &m).unwrap();
    let server =
        Server::start(ServerConfig { app: PeerInfo { id: "arcade.paint".into(), version: "1.0.0".into() }, locations: locations.clone() }, paint.clone())
            .unwrap();
    let resident = Resident::new(locations.clone(), Home::under(&root.join("home")), root.join("arcade-helper"), "1.1.0");
    resident.spawn_windows.store(false, Ordering::Relaxed);
    let worker = resident.clone();
    std::thread::spawn(move || worker.work_forever());
    World { root, locations, resident, paint, _server: server }
}

fn wait_finished(r: &Resident, id: u64) -> arcade_helper::resident::JobView {
    let start = Instant::now();
    let mut since = 0;
    loop {
        let (rev, jobs) = r.jobs(since, Duration::from_millis(200), false);
        since = rev;
        if let Some(j) = jobs.iter().find(|j| j.id == id && j.finished()) {
            return j.clone();
        }
        assert!(start.elapsed() < Duration::from_secs(10), "job {id} didn't finish: {jobs:?}");
    }
}

fn input(w: &World, name: &str) -> String {
    let dir = w.root.join("Pictures");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, b"png").unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn a_menu_click_runs_and_places_the_result_beside_the_input() {
    let w = world("place");
    let a = input(&w, "a.png");
    std::fs::write(w.root.join("Pictures").join("a.png.out"), b"keep me").unwrap();
    let id = w.resident.queue(RunSpec { reference: "arcade.paint/paint.invert".into(), paths: vec![a], ..RunSpec::default() }).unwrap();
    let job = wait_finished(&w.resident, id);
    assert_eq!(job.status, "done", "{job:?}");
    assert_eq!((job.title.as_str(), job.app.as_str()), ("Invert colors", "arcade.paint"));
    assert_eq!(job.files, [w.root.join("Pictures").join("a.png (1).out").to_string_lossy()]);
    assert_eq!(std::fs::read(w.root.join("Pictures").join("a.png.out")).unwrap(), b"keep me");
    let seen = w.paint.seen.lock().unwrap();
    assert_eq!(seen[0].options, json!({"strength": 5}), "the form's defaults");
    assert_eq!(seen[0].context.source, "file-manager");
    std::fs::remove_dir_all(&w.root).ok();
}

#[test]
fn quick_clicks_on_one_action_become_one_job() {
    let w = world("batch");
    let (a, b) = (input(&w, "a.png"), input(&w, "b.png"));
    let spec = |p: String| RunSpec { reference: "arcade.paint/paint.invert".into(), paths: vec![p], ..RunSpec::default() };
    let first = w.resident.queue(spec(a)).unwrap();
    let second = w.resident.queue(spec(b)).unwrap();
    assert_eq!(first, second);
    let job = wait_finished(&w.resident, first);
    assert_eq!((job.status.as_str(), job.inputs, job.files.len()), ("done", 2, 2));
    let seen = w.paint.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "one request");
    assert_eq!(seen[0].inputs[0].kind, "file/image[]");
    std::fs::remove_dir_all(&w.root).ok();
}

#[test]
fn missing_apps_and_actions_fail_with_a_sentence() {
    let w = world("missing");
    let a = input(&w, "a.png");
    let id = w.resident.queue(RunSpec { reference: "arcade.box/box:x".into(), paths: vec![a.clone()], ..RunSpec::default() }).unwrap();
    let job = wait_finished(&w.resident, id);
    assert_eq!((job.status.as_str(), job.message.as_str()), ("failed", "Arcade Box isn't installed."));
    let id = w.resident.queue(RunSpec { reference: "arcade.paint/paint.gone".into(), paths: vec![a], ..RunSpec::default() }).unwrap();
    assert!(wait_finished(&w.resident, id).message.contains("no longer offers"));
    assert!(w.resident.queue(RunSpec { reference: "arcade.paint/paint.invert".into(), ..RunSpec::default() }).is_err(), "nothing to run on");
    std::fs::remove_dir_all(&w.root).ok();
}

#[test]
fn asking_for_a_folder_waits_and_cancel_ends_it() {
    let w = world("ask");
    arcade_link::offers::Prefs { ask_where: true, ..Default::default() }.save(&w.locations).unwrap();
    let a = input(&w, "a.png");
    let spec = RunSpec { reference: "arcade.paint/paint.invert".into(), paths: vec![a], origin: "browser".into(), ..RunSpec::default() };
    let id = w.resident.queue(spec.clone()).unwrap();
    let start = Instant::now();
    while !w.resident.jobs(0, Duration::ZERO, false).1.iter().any(|j| j.id == id && j.status == "ask") {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(20));
    }
    // While a job waits, the status tells Arcade Tools not to replace the helper.
    let status = arcade_link::server::Handler::status(&arcade_helper::resident::ResidentHandler(w.resident.clone()));
    assert_eq!((status["busy"].as_bool(), status["jobs"].as_u64()), (Some(true), Some(1)), "{status}");
    let chosen = w.root.join("Chosen");
    assert!(w.resident.answer(id, Some(chosen.clone())));
    let job = wait_finished(&w.resident, id);
    assert_eq!(job.files, [chosen.join("a.png.out").to_string_lossy()]);
    let id = w.resident.queue(spec).unwrap();
    while !w.resident.jobs(0, Duration::ZERO, false).1.iter().any(|j| j.id == id && j.status == "ask") {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(w.resident.cancel(id));
    assert_eq!(wait_finished(&w.resident, id).status, "cancelled");
    w.resident.dismiss(None);
    assert!(w.resident.jobs(0, Duration::ZERO, false).1.is_empty());
    let _: Value = json!(null);
    std::fs::remove_dir_all(&w.root).ok();
}
