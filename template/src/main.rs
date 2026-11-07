//! Arcade Template: the smallest app that meets ARCADE_APP.md. Replace the
//! window contents and the `echo` action with your app; keep the rest.

use std::process::ExitCode;
use std::sync::Arc;

use arcade_link::server::{Handler, InvokeContext, Reply, Server, ServerConfig};
use arcade_link::{runtime, Action, InvokeRequest, InvokeResult, LinkError, Locations, Manifest, PeerInfo};

const ID: &str = "arcade.template";
const NAME: &str = "Arcade Template";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const USAGE: &str = "Arcade Template

USAGE
  arcade-template                 Open the window (or bring the running one forward)
  arcade-template --status        One line of JSON; never opens a window
  arcade-template --version
  arcade-template --arcade-manifest
  arcade-template --help";

fn manifest() -> Manifest {
    let mut m = Manifest::new(ID, VERSION, &arcade_link::manifest::current_executable());
    m.name = NAME.into();
    m.actions = vec![Action::new("template.echo", "Echo", "echo").accepts(&["text/*"])];
    m
}

struct App;

impl Handler for App {
    fn describe(&self) -> Vec<Action> {
        manifest().actions
    }
    fn invoke(&self, req: InvokeRequest, _: &InvokeContext) -> Result<Reply, LinkError> {
        match req.action.as_str() {
            "template.echo" => Ok(Reply::Done(InvokeResult::outputs(req.inputs, "Echoed"))),
            _ => Err(LinkError::unsupported("no such action")),
        }
    }
}

fn instance_dir() -> std::path::PathBuf {
    arcade_link::paths::app_dirs(ID).cache.join("instance")
}

fn status() -> ExitCode {
    let running = matches!(runtime::acquire_instance(&instance_dir(), "template"), Ok(None));
    println!("{}", serde_json::json!({"app": ID, "version": VERSION, "running": running}));
    if running {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

fn run() -> ExitCode {
    runtime::install_crash_reports(NAME, VERSION, runtime::crash_dir(ID));
    runtime::wait_for_predecessor(std::time::Duration::from_secs(5));
    let Ok(Some(_lock)) = runtime::acquire_instance(&instance_dir(), "template") else {
        // Another instance runs: it has the window; ask it to come forward.
        let loc = Locations::discover();
        if let Ok(mut c) = arcade_link::Client::connect(&loc, ID, &PeerInfo { id: ID.into(), version: VERSION.into() }) {
            let _ = c.call("app.activate", serde_json::json!({}));
        }
        return ExitCode::SUCCESS;
    };
    let loc = Locations::discover();
    let _ = arcade_link::manifest::write_manifest(&loc, &manifest());
    let _server = Server::start(ServerConfig { app: PeerInfo { id: ID.into(), version: VERSION.into() }, locations: loc }, Arc::new(App));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_app_id("dev.arcade.Template").with_title(NAME).with_inner_size([420.0, 280.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        NAME,
        options,
        Box::new(|cc| {
            arcade_link::design::egui::apply(&cc.egui_ctx, "system", ID);
            runtime::trim();
            Ok(Box::new(Window::default()))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("arcade-template: {e}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Window {
    search: String,
}

impl eframe::App for Window {
    fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
        // Escape goes back one step: clear the search, then close (UI-2).
        if ctx.input(|i| i.key_pressed(eframe::egui::Key::Escape)) {
            if self.search.is_empty() {
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            } else {
                self.search.clear();
            }
        }
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(NAME);
            ui.text_edit_singleline(&mut self.search);
            ui.label(if self.search.is_empty() { "Esc closes" } else { "Esc clears the search" });
        });
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => match runtime::relaunch_unpacked(ID, VERSION, false) {
            Some(code) => ExitCode::from(code as u8),
            None => run(),
        },
        ["--status"] => status(),
        ["--version"] => {
            println!("{NAME} {VERSION}");
            ExitCode::SUCCESS
        }
        ["--arcade-manifest"] => {
            println!("{}", manifest().to_json());
            ExitCode::SUCCESS
        }
        ["--help" | "-h"] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("arcade-template: unknown arguments {args:?}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
