//! `arcade-helper`: see the crate docs. Arcade Tools installs it, starts `arcade-helper --background` at login and
//! runs `arcade-helper integrate --remove` before uninstalling it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use arcade_helper::bridge::{self, Bridge};
use arcade_helper::client::HelperClient;
use arcade_helper::integrations::{self, Home};
use arcade_helper::native::Caller;
use arcade_helper::resident::{self, Resident, ResidentHandler, RunSpec};
use arcade_helper::{ids, ui};
use arcade_link::catalog::Catalog;
use arcade_link::offers::{self, Item, Prefs};
use arcade_link::registry::Registry;
use arcade_link::server::{Server, ServerConfig};
use arcade_link::wire::PeerInfo;
use arcade_link::Locations;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "arcade-helper: Arcade's actions in your file manager and browser.

  arcade-helper --background            run the helper (Arcade Tools starts it at login)
  arcade-helper menu <files…>           choose an action for these files
  arcade-helper run <app/action> <files…>
  arcade-helper offers [--surface S] <files…>
  arcade-helper jobs [--wait-ms N]      the jobs, as JSON (waits up to N ms for a change)
  arcade-helper integrate [--remove]    write (or remove) the menu entries
  arcade-helper --version";

fn exe() -> PathBuf {
    // An AppImage runs from a temporary mount: menus must call the AppImage itself.
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("arcade-helper"))
}

fn fail(message: &str) -> ! {
    eprintln!("arcade-helper: {message}");
    std::process::exit(1);
}

fn paths(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| {
            let p = Path::new(a);
            // Some file managers hand over file:// URIs.
            let p = a.strip_prefix("file://").map(|s| PathBuf::from(percent_decode(s))).unwrap_or_else(|| p.to_path_buf());
            std::fs::canonicalize(&p).unwrap_or(p).to_string_lossy().into_owned()
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn background(locations: Locations) -> ! {
    let resident = Resident::new(locations.clone(), Home::discover(), exe(), VERSION);
    let config = ServerConfig { app: PeerInfo { id: ids::HELPER_ID.into(), version: VERSION.into() }, locations: locations.clone() };
    let server = match Server::start(config, Arc::new(ResidentHandler(resident.clone()))) {
        Ok(s) => s,
        // Another helper already runs for this user.
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => std::process::exit(0),
        Err(e) => fail(&format!("couldn't start: {e}")),
    };
    arcade_link::manifest::write_manifest(&locations, &resident.manifest()).ok();
    if let Err(e) = resident.refresh_integrations() {
        eprintln!("arcade-helper: menu entries: {e}");
    }
    resident::watch(resident.clone());
    let worker = resident.clone();
    std::thread::spawn(move || worker.work_forever());
    let _keep = server;
    loop {
        std::thread::park();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let locations = Locations::discover();
    if let Some(caller) = Caller::from_args(&args) {
        let bridge = Bridge { locations, exe: exe(), caller };
        let code = match bridge::serve(bridge, &mut std::io::stdin().lock(), std::io::stdout()) {
            Ok(()) => 0,
            Err(_) => 1,
        };
        std::process::exit(code);
    }
    match args.first().map(String::as_str) {
        Some("--background") => background(locations),
        Some("--version") => println!("arcade-helper {VERSION}"),
        Some("run") => {
            let reference = args.get(1).cloned().unwrap_or_else(|| fail("run needs an action"));
            let files = paths(&args[2..]);
            if files.is_empty() {
                fail("run needs files");
            }
            let mut c = HelperClient::connect(&locations, &exe(), "menu").unwrap_or_else(|e| fail(&e.message));
            c.run(&RunSpec { reference, paths: files, ..RunSpec::default() }).unwrap_or_else(|e| fail(&e.message));
        }
        Some("menu") => {
            let files = paths(&args[1..]);
            if files.is_empty() {
                fail("menu needs files");
            }
            std::process::exit(ui::picker(locations, exe(), files));
        }
        Some("progress") => std::process::exit(ui::progress(locations, exe())),
        Some("jobs") => {
            let wait = match (args.get(1).map(String::as_str), args.get(2)) {
                (Some("--wait-ms"), Some(n)) => n.parse().unwrap_or_else(|_| fail("--wait-ms needs a number")),
                (None, _) => 0,
                _ => fail("usage: jobs [--wait-ms N]"),
            };
            let mut c = HelperClient::connect(&locations, &exe(), "cli").unwrap_or_else(|e| fail(&e.message));
            // The first call reads the current revision; with a wait, the second waits for the next change.
            let (revision, jobs) = c.jobs(0, Duration::ZERO, false).unwrap_or_else(|e| fail(&e.message));
            let (revision, jobs) =
                if wait > 0 { c.jobs(revision, Duration::from_millis(wait), false).unwrap_or_else(|e| fail(&e.message)) } else { (revision, jobs) };
            println!("{}", serde_json::json!({"revision": revision, "jobs": jobs}));
        }
        Some("offers") => {
            let (surface, rest) = match (args.get(1).map(String::as_str), args.get(2)) {
                (Some("--surface"), Some(s)) => (s.clone(), &args[3..]),
                _ => (String::new(), &args[1..]),
            };
            let items: Vec<Item> = paths(rest)
                .iter()
                .map(|p| Item { size: std::fs::metadata(p).ok().map(|m| m.len()), ..Item::new(&arcade_helper::jobs::file_content(Path::new(p)).kind) })
                .collect();
            let catalog = Catalog::load(&locations);
            let list = offers::offers(&items, &surface, Registry::load(&locations).apps(), &catalog.rules, &Prefs::load(&locations));
            println!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
        }
        Some("integrate") => {
            let state = resident::state_path(&locations);
            let done = if args.get(1).map(String::as_str) == Some("--remove") {
                // Uninstall: the entries, then the helper's card.
                let done = integrations::remove_all(&state);
                std::fs::remove_file(locations.manifest(ids::HELPER_ID)).ok();
                done
            } else {
                let r = Resident::new(locations, Home::discover(), exe(), VERSION);
                r.refresh_integrations()
            };
            match done {
                Ok(d) => println!("{} written, {} removed", d.written, d.removed),
                Err(e) => fail(&e.to_string()),
            }
        }
        _ => {
            println!("{HELP}");
            if !args.is_empty() {
                std::process::exit(2);
            }
        }
    }
    // Give a detached child (a started resident) a moment before the menu's process ends.
    std::thread::sleep(Duration::from_millis(10));
}
