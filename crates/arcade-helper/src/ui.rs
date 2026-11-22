//! The helper's two windows, each its own short-lived process: the picker (`arcade-helper menu <files>`, the
//! file menus' "Arcade…" entry) and the progress window (`arcade-helper progress`, started by the resident).
//! Both draw with Link's design kit, like Find and Lens.

use eframe::egui::{self, Ui};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arcade_link::catalog::Catalog;
use arcade_link::design::egui::components::{ButtonKind, Kit, Tone};
use arcade_link::form::{Control, Form};
use arcade_link::offers::{self, Item, Offer, Prefs};
use arcade_link::registry::Registry;
use arcade_link::Locations;

use crate::client::HelperClient;
use crate::ids;
use crate::resident::{self, JobView, RunSpec};

const KIT_ID: &str = "arcade.tools";

fn options(title: &str, size: [f32; 2]) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(title).with_app_id("dev.arcade.helper").with_inner_size(size).with_min_inner_size([320.0, 200.0]),
        ..Default::default()
    }
}

fn kit(ui: &Ui) -> Kit {
    Kit::new(ui, KIT_ID)
}

// ---- Picker ---------------------------------------------------------------------------------------------------

enum Page {
    List,
    Form { offer: usize, form: Form, values: Map<String, Value>, text: Map<String, Value>, error: Option<String> },
    Sending,
    Failed(String),
}

struct Picker {
    locations: Locations,
    exe: PathBuf,
    files: Vec<String>,
    offers: Vec<Offer>,
    names: Vec<String>,
    forms: Vec<Option<Form>>,
    query: String,
    page: Page,
    result: Arc<Mutex<Option<Result<(), String>>>>,
    focus: bool,
}

impl Picker {
    fn new(locations: Locations, exe: PathBuf, files: Vec<String>) -> Picker {
        let registry = Registry::load(&locations);
        let catalog = Catalog::load(&locations);
        let prefs = Prefs::load(&locations);
        let items: Vec<Item> = files
            .iter()
            .map(|f| {
                let c = crate::jobs::file_content(Path::new(f));
                Item { kind: c.kind, size: c.size, hints: Vec::new() }
            })
            .collect();
        let offers: Vec<Offer> = offers::offers(&items, "", registry.apps(), &catalog.rules, &prefs).into_iter().filter(|o| o.app != ids::HELPER_ID).collect();
        let names = offers.iter().map(|o| catalog.name(&o.app)).collect();
        let forms = offers.iter().map(|o| registry.get(&o.app).and_then(|m| resident::action_of(m, &o.reference())).and_then(|a| a.options.clone())).collect();
        Picker { locations, exe, files, offers, names, forms, query: String::new(), page: Page::List, result: Arc::new(Mutex::new(None)), focus: true }
    }

    fn send(&mut self, i: usize, values: Map<String, Value>) {
        let spec = RunSpec { reference: self.offers[i].reference(), paths: self.files.clone(), values, ..RunSpec::default() };
        let (locations, exe, result) = (self.locations.clone(), self.exe.clone(), self.result.clone());
        self.page = Page::Sending;
        std::thread::spawn(move || {
            let r =
                HelperClient::connect(&locations, &exe, "picker").and_then(|mut c| c.run(&spec)).map(|_| ()).map_err(|e| e.user_message("The Arcade helper"));
            *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
        });
    }

    fn choose(&mut self, i: usize) {
        let o = &self.offers[i];
        if let Some(install) = o.install.clone() {
            install_requirement(&self.locations, &install, &o.app);
            return;
        }
        match self.forms[i].clone().filter(|f| !f.controls.is_empty() && !o.one_click) {
            Some(form) => {
                let values = form.defaults();
                let text = values.iter().map(|(k, v)| (k.clone(), Value::String(as_text(v)))).collect();
                self.page = Page::Form { offer: i, form, values, text, error: None };
            }
            None => self.send(i, Map::new()),
        }
    }

    fn list(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let k = kit(ui);
        let what = if self.files.len() == 1 {
            Path::new(&self.files[0]).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            format!("{} items", self.files.len())
        };
        k.page_head(ui, "What do you want to do?", &what);
        let search = k.text_input(ui, &mut self.query, "Search actions", None);
        if self.focus {
            search.request_focus();
            self.focus = false;
        }
        ui.add_space(8.0);
        let q = self.query.to_lowercase();
        let shown: Vec<usize> = (0..self.offers.len())
            .filter(|&i| q.is_empty() || self.offers[i].title.to_lowercase().contains(&q) || self.names[i].to_lowercase().contains(&q))
            .collect();
        if shown.is_empty() {
            k.empty_state(
                ui,
                "search",
                "Nothing here",
                if self.offers.is_empty() {
                    "No installed Arcade app works with this. Arcade Tools can add more apps."
                } else {
                    "No action matches your search."
                },
                |_| {},
            );
        }
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let mut picked = None;
        let mut reload = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut other_shown = false;
            for (n, &i) in shown.iter().enumerate() {
                let o = &self.offers[i];
                if o.other && !other_shown {
                    k.section_head(ui, "Other apps");
                    other_shown = true;
                }
                let description = match (&o.install, &o.reason) {
                    (Some(_), _) => format!("Install what it needs · {}", self.names[i]),
                    (None, Some(r)) => format!("{r} · {}", self.names[i]),
                    _ => self.names[i].clone(),
                };
                let usable = o.enabled || o.install.is_some();
                // The whole row is the button; "Always use this" inside it stays its own button.
                let sense = if usable { egui::Sense::click() } else { egui::Sense::hover() };
                let row = ui.scope_builder(egui::UiBuilder::new().sense(sense), |ui| {
                    if !usable {
                        ui.disable();
                    }
                    k.row(ui, &o.title, &description, |ui| {
                        (o.choose && k.button_state(ui, "Always use this", ButtonKind::Ghost, true, false, true).clicked()).then_some(true)
                    })
                    .inner
                });
                if row.inner == Some(true) {
                    if let Some(job) = &o.job {
                        let mut prefs = Prefs::load(&self.locations);
                        prefs.defaults.insert(job.clone(), o.reference());
                        prefs.save(&self.locations).ok();
                        reload = true;
                    }
                } else if usable && (row.response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() || (enter && n == 0)) {
                    picked = Some(i);
                }
            }
        });
        if reload {
            // The default changed which entries are preferred and which go under "Other apps".
            let query = std::mem::take(&mut self.query);
            *self = Picker::new(self.locations.clone(), self.exe.clone(), std::mem::take(&mut self.files));
            self.query = query;
            self.focus = false;
        }
        if let Some(i) = picked {
            self.choose(i);
            ctx.request_repaint();
        }
    }

    fn form(&mut self, ui: &mut Ui) {
        let k = kit(ui);
        let Page::Form { offer, form, values, text, error } = &mut self.page else { return };
        let o = &self.offers[*offer];
        k.page_head(ui, &o.title, &self.names[*offer]);
        let snapshot = values.clone();
        let (basic, advanced): (Vec<&Control>, Vec<&Control>) = form.controls.iter().filter(|c| c.visible(&snapshot)).partition(|c| !c.advanced);
        for c in basic {
            control(ui, &k, c, values, text);
        }
        if !advanced.is_empty() {
            k.disclosure(ui, "advanced", "More options", |ui| {
                for c in &advanced {
                    control(ui, &k, c, values, text);
                }
            });
        }
        if let Some(e) = error {
            k.notice(ui, e, Tone::Danger);
        }
        let (mut run, mut back) = (false, false);
        k.dialog_actions(ui, |ui| {
            back = k.button(ui, "Back", ButtonKind::Default).clicked();
            run = k.button(ui, "Run", ButtonKind::Primary).clicked();
        });
        if back {
            self.page = Page::List;
        } else if run {
            let mut merged = values.clone();
            for (key, v) in text.iter() {
                merged.insert(key.clone(), v.clone());
            }
            match form.resolve(&merged) {
                Ok(_) => {
                    let i = *offer;
                    self.send(i, merged);
                }
                Err(e) => *error = Some(e),
            }
        }
    }
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// One form control. Text-like values live in `text` while typed; `values` holds the rest.
fn control(ui: &mut Ui, k: &Kit, c: &Control, values: &mut Map<String, Value>, text: &mut Map<String, Value>) {
    match c.kind.as_str() {
        "toggle" => {
            let mut on = values.get(&c.key).and_then(Value::as_bool).unwrap_or(false);
            if k.switch_row(ui, &mut on, &c.label, c.help.as_deref().unwrap_or("")).changed() {
                values.insert(c.key.clone(), Value::Bool(on));
            }
        }
        "select" => {
            let choices: Vec<(Value, &str)> = c.choices.iter().map(|ch| (ch.value.clone(), ch.label.as_str())).collect();
            let mut v = values.get(&c.key).cloned().unwrap_or(Value::Null);
            k.row(ui, &c.label, c.help.as_deref().unwrap_or(""), |ui| {
                if k.select(ui, &c.key, &mut v, &choices).changed() {
                    values.insert(c.key.clone(), v.clone());
                }
            });
        }
        "number" | "text" | "password" | "directory" | "file" | "color" => {
            let mut s = text.get(&c.key).map(as_text).unwrap_or_default();
            k.row(ui, &c.label, c.help.as_deref().unwrap_or(""), |ui| {
                let edit =
                    egui::TextEdit::singleline(&mut s).password(c.kind == "password").hint_text(c.placeholder.as_deref().unwrap_or("")).desired_width(180.0);
                if ui.add(edit).changed() {
                    let v = if c.kind == "number" {
                        s.trim().parse::<f64>().ok().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::String(s.clone()))
                    } else {
                        Value::String(s.clone())
                    };
                    values.insert(c.key.clone(), v.clone());
                    text.insert(c.key.clone(), v);
                }
            });
        }
        // Unknown control types keep the app's default (SPEC §3).
        _ => {}
    }
}

/// "Install …": an app through Arcade Tools, an engine through the app that needs it (its own settings install it).
fn install_requirement(locations: &Locations, need: &str, owner: &str) {
    let me = arcade_link::wire::PeerInfo { id: ids::HELPER_ID.into(), version: env!("CARGO_PKG_VERSION").into() };
    let app = need.strip_prefix("app:").unwrap_or(owner);
    if arcade_link::client::request_install(locations, &me, app).is_err() {
        open::that_detached(Catalog::load(locations).releases_url(app)).ok();
    }
}

impl eframe::App for Picker {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if let Some(r) = self.result.lock().unwrap_or_else(|e| e.into_inner()).take() {
            match r {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => self.page = Page::Failed(e),
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| match &self.page {
            Page::List => self.list(ui, ctx),
            Page::Form { .. } => self.form(ui),
            Page::Sending => {
                ui.centered_and_justified(|ui| ui.spinner());
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Page::Failed(e) => {
                let k = kit(ui);
                k.notice(ui, e, Tone::Danger);
                if k.button(ui, "Close", ButtonKind::Default).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        });
    }
}

/// Runs the picker; the process exit code.
pub fn picker(locations: Locations, exe: PathBuf, files: Vec<String>) -> i32 {
    let app = Picker::new(locations, exe, files);
    let r = eframe::run_native(
        "Arcade",
        options("Arcade", [440.0, 520.0]),
        Box::new(|cc| {
            arcade_link::design::egui::install_inter(&cc.egui_ctx);
            arcade_link::design::egui::apply(&cc.egui_ctx, "system", KIT_ID);
            cc.egui_ctx.style_mut(|s| s.interaction.selectable_labels = false);
            Ok(Box::new(app))
        }),
    );
    i32::from(r.is_err())
}

// ---- Progress window ------------------------------------------------------------------------------------------

#[derive(Default)]
struct Shared {
    jobs: Vec<JobView>,
    gone: bool,
}

struct Progress {
    locations: Locations,
    exe: PathBuf,
    shared: Arc<Mutex<Shared>>,
    folders: std::collections::HashMap<u64, String>,
    calm_since: Option<Instant>,
    seen_done: Vec<u64>,
}

impl Progress {
    fn act(&self, f: impl FnOnce(&mut HelperClient) + Send + 'static) {
        let (locations, exe) = (self.locations.clone(), self.exe.clone());
        std::thread::spawn(move || {
            if let Ok(mut c) = HelperClient::connect(&locations, &exe, "progress") {
                f(&mut c);
            }
        });
    }

    fn job(&mut self, ui: &mut Ui, j: &JobView) {
        let k = kit(ui);
        let count = if j.inputs > 1 { format!(" · {} items", j.inputs) } else { String::new() };
        let status = match j.status.as_str() {
            "queued" => "Waiting".to_string(),
            "ask" => "Where should the result go?".to_string(),
            "running" if !j.message.is_empty() => j.message.clone(),
            "running" => "Working…".to_string(),
            "done" if j.files.len() == 1 => {
                format!("Saved {}", Path::new(&j.files[0]).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
            "done" if j.files.len() > 1 => format!("Saved {} files", j.files.len()),
            "done" => "Done".to_string(),
            "failed" => j.message.clone(),
            _ => "Cancelled".to_string(),
        };
        k.row(ui, &j.title, &format!("{}{count} · {status}", j.app), |ui| {
            let id = j.id;
            match j.status.as_str() {
                "queued" | "running" => {
                    if k.button_state(ui, "Cancel", ButtonKind::Ghost, true, false, true).clicked() {
                        self.act(move |c| {
                            c.cancel(id).ok();
                        });
                    }
                }
                "done" => {
                    if let Some(first) = j.files.first().cloned() {
                        if k.button_state(ui, "Open", ButtonKind::Ghost, true, false, true).clicked() {
                            open::that_detached(&first).ok();
                        }
                        if k.button_state(ui, "Show in folder", ButtonKind::Ghost, true, false, true).clicked() {
                            if let Some(dir) = Path::new(&first).parent() {
                                open::that_detached(dir).ok();
                            }
                        }
                    }
                    if !j.texts.is_empty() && k.button_state(ui, "Copy", ButtonKind::Ghost, true, false, true).clicked() {
                        ui.ctx().copy_text(j.texts.join("\n"));
                    }
                }
                "failed" => {
                    k.badge(ui, "Failed", Tone::Danger);
                }
                _ => {}
            }
        });
        if j.status == "running" {
            ui.add(egui::ProgressBar::new(j.fraction.unwrap_or(0.0)).animate(j.fraction.is_none()));
        }
        if j.status == "ask" {
            let id = j.id;
            let folder = self.folders.entry(id).or_insert_with(|| crate::jobs::downloads_dir().to_string_lossy().into_owned());
            k.text_input(ui, folder, "Folder", None);
            let chosen = folder.clone();
            let (mut save, mut cancel) = (false, false);
            ui.horizontal(|ui| {
                cancel = k.button(ui, "Cancel", ButtonKind::Default).clicked();
                save = k.button(ui, "Save here", ButtonKind::Primary).clicked();
            });
            if save {
                self.act(move |c| {
                    c.answer(id, Some(Path::new(&chosen))).ok();
                });
            } else if cancel {
                self.act(move |c| {
                    c.answer(id, None).ok();
                });
            }
        }
    }
}

impl eframe::App for Progress {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (jobs, gone) = {
            let s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (s.jobs.clone(), s.gone)
        };
        // A system notification for each job that finishes while the window isn't in front.
        let focused = ctx.input(|i| i.focused);
        let fresh: Vec<&JobView> = jobs.iter().filter(|j| j.finished() && !self.seen_done.contains(&j.id)).collect();
        for j in fresh {
            self.seen_done.push(j.id);
            if !focused && j.status != "cancelled" {
                let body = if j.status == "done" { format!("{} finished.", j.title) } else { j.message.clone() };
                notify_rust::Notification::new().appname("Arcade").summary(&j.title).body(&body).show().ok();
            }
        }
        let busy = jobs.iter().any(|j| !j.finished());
        let failed = jobs.iter().any(|j| j.status == "failed");
        let hovered = ctx.input(|i| i.pointer.has_pointer());
        // Close a few seconds after everything finished well, unless the pointer is on the window.
        if gone || (!busy && !failed && !hovered) {
            let since = *self.calm_since.get_or_insert_with(Instant::now);
            if gone || since.elapsed() > Duration::from_secs(if jobs.is_empty() { 2 } else { 6 }) {
                self.act(|c| {
                    c.dismiss(None).ok();
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(500));
        } else {
            self.calm_since = None;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            let k = kit(ui);
            let mut clear = false;
            k.page_head_with(ui, "Arcade", if busy { "Working on your files" } else { "Finished" }, |ui| {
                // Always drawn, so the heading's measured layout never changes size.
                let any_finished = jobs.iter().any(JobView::finished);
                clear = k.button_state(ui, "Clear finished", ButtonKind::Ghost, any_finished, false, true).clicked();
            });
            if clear {
                self.act(|c| {
                    c.dismiss(None).ok();
                });
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for j in &jobs {
                    self.job(ui, j);
                }
            });
        });
    }
}

/// Runs the progress window until the jobs are done and it closes; the process exit code.
pub fn progress(locations: Locations, exe: PathBuf) -> i32 {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let app = Progress {
        locations: locations.clone(),
        exe: exe.clone(),
        shared: shared.clone(),
        folders: Default::default(),
        calm_since: None,
        seen_done: Vec::new(),
    };
    let r = eframe::run_native(
        "Arcade",
        options("Arcade", [400.0, 300.0]),
        Box::new(move |cc| {
            arcade_link::design::egui::install_inter(&cc.egui_ctx);
            arcade_link::design::egui::apply(&cc.egui_ctx, "system", KIT_ID);
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || follow(locations, exe, shared, ctx));
            Ok(Box::new(app))
        }),
    );
    i32::from(r.is_err())
}

/// Follows the resident's jobs (long-polling; the resident answers as soon as something changes).
fn follow(locations: Locations, exe: PathBuf, shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    let mut since = 0;
    let mut failures = 0;
    loop {
        match HelperClient::connect(&locations, &exe, "progress").and_then(|mut c| loop {
            let (rev, jobs) = c.jobs(since, Duration::from_secs(20), true)?;
            since = rev;
            shared.lock().unwrap_or_else(|e| e.into_inner()).jobs = jobs;
            ctx.request_repaint();
        }) {
            Ok(()) => {}
            Err(_) => {
                failures += 1;
                if failures > 3 {
                    shared.lock().unwrap_or_else(|e| e.into_inner()).gone = true;
                    ctx.request_repaint();
                    return;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}
