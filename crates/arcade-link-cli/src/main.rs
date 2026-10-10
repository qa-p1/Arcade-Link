//! `arcade-link`: inspect and drive Arcade apps over the Link.
//!
//! Essential for development and for bug reports. Every command honors
//! `ARCADE_HOME`.

mod mock;
mod tray;

use std::process::ExitCode;
use std::sync::atomic::AtomicBool;

use arcade_link::client::{self, AppState, CallOptions, Client};
use arcade_link::{Content, InvokeRequest, Locations, Manifest, PeerInfo, Registry};
use serde_json::{json, Value};

const HELP: &str = "arcade-link: inspect and drive Arcade apps over the Link

USAGE
  arcade-link ls [--json]                     Installed apps, their state and action counts
  arcade-link describe <app> [--json]         Actions (live if running, else from the manifest)
  arcade-link invoke <app> <action> [INPUT…] [--preset P] [--option k=v]… [--json]
      INPUT: --file PATH (repeatable) | --text T | --url U | --type T --hint H
             --input-json '{\"type\":…}'
  arcade-link status <app>                    app.status of a running app
  arcade-link activate <app>                  Bring the app's window forward
  arcade-link quit <app> [--force]            app.quit (refused with busy while jobs run)
  arcade-link watch                           Print registry and app.changed events
  arcade-link mock --as <id> --actions <file.json>
                                              Run a scriptable fake app (see fixtures/)
  arcade-link check-manifest <file.json>      Validate a manifest
  arcade-link tray <app> [--link-off]          Watch tray hosting as an app
  arcade-link tray host                       Fake Tools host; commands on stdin
  arcade-link shortcuts                      Effective globals and conflicts (JSON)
  arcade-link shortcuts validate <file> [--manifest <file>]
  arcade-link shortcuts markdown <file>       Generate shortcuts.md
  arcade-link settings <app>                  Open settings
  arcade-link restart <app> [--force]          Restart a running app

<app> is a canonical ID (arcade.box) or its short form (box).";

fn me() -> PeerInfo {
    PeerInfo { id: "arcade.link-cli".into(), version: env!("CARGO_PKG_VERSION").into() }
}

fn app_id(arg: &str) -> String {
    if arg.contains('.') {
        arg.to_string()
    } else {
        format!("arcade.{arg}")
    }
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn values<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    args.windows(2).filter(|w| w[0] == name).map(|w| w[1].as_str()).collect()
}

fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    values(args, name).into_iter().next()
}

/// Set by the first Ctrl-C during `invoke`: the running job gets `job.cancel`.
static CANCEL: AtomicBool = AtomicBool::new(false);

fn cancel_on_interrupt() {
    #[cfg(unix)]
    {
        extern "C" fn on_interrupt(_: libc::c_int) {
            CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
            // A second Ctrl-C ends the CLI at once.
            unsafe {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
            }
        }
        unsafe {
            libc::signal(libc::SIGINT, on_interrupt as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
    }
}

fn main() -> ExitCode {
    // `arcade-link describe box | head` must end quietly, not panic.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let loc = Locations::discover();
    let r = match args.first().map(String::as_str) {
        Some("ls") => ls(&loc, &args),
        Some("describe") => describe(&loc, &args),
        Some("invoke") => invoke(&loc, &args),
        Some("status") => simple(&loc, &args, "app.status", json!({})),
        Some("activate") => simple(&loc, &args, "app.activate", json!({})),
        Some("quit") => simple(&loc, &args, "app.quit", json!({ "force": flag(&args, "--force") })),
        Some("watch") => watch(&loc),
        Some("tray") => tray::run(&loc, &args),
        Some("shortcuts") => shortcuts(&loc, &args),
        Some("settings") => simple(&loc, &args, "app.settings", json!({})),
        Some("restart") => simple(&loc, &args, "app.restart", json!({"mode": if flag(&args,"--force") {"force"} else {"normal"}})),
        Some("mock") => mock::run(&loc, &args),
        Some("check-manifest") => check_manifest(&args),
        Some("-V" | "--version") => {
            println!("arcade-link {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => {
            println!("{HELP}");
            Ok(())
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("arcade-link: {e}");
            ExitCode::FAILURE
        }
    }
}

fn ls(loc: &Locations, args: &[String]) -> Result<(), String> {
    let reg = Registry::load(loc);
    let rows: Vec<Value> = reg
        .apps()
        .iter()
        .map(|m| {
            let state = match client::app_state(loc, &reg, &m.id, &me()) {
                AppState::Running { version } => format!("running {version}"),
                AppState::Installed { .. } => "installed".into(),
                AppState::NotInstalled => "not installed".into(),
            };
            let install =
                arcade_link::receipt::Store::new(loc).read(&m.id).ok().flatten().map(|r| r.method).or_else(|| reg.additions(&m.id).and_then(|a| a.install));
            let tray = Client::connect(loc, &m.id, &me())
                .ok()
                .and_then(|mut c| {
                    c.set_timeout(client::HELLO_TIMEOUT).ok()?;
                    c.status().ok()
                })
                .and_then(|s| s["status"]["tray"].as_str().map(String::from))
                .unwrap_or_else(|| if state.starts_with("running") { "unknown".into() } else { "none".into() });
            json!({
                "id": m.id, "name": m.name, "version": m.version, "state": state,
                "linkEnabled": m.settings.link_enabled, "actions": m.actions.len(),
                "available": m.usable_actions().count(), "executable": m.executable,
                "install": install, "tray": tray,
            })
        })
        .collect();
    if flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
        return Ok(());
    }
    println!("registry: {}", loc.registry.display());
    if rows.is_empty() {
        println!("(no Arcade apps registered)");
    }
    for r in rows {
        println!(
            "{:<18} {:<18} {:<10} {:<18} {:>3} actions ({} available)  install={} tray={}{}",
            r["id"].as_str().unwrap_or(""),
            r["name"].as_str().unwrap_or(""),
            r["version"].as_str().unwrap_or(""),
            r["state"].as_str().unwrap_or(""),
            r["actions"],
            r["available"],
            r["install"].as_str().unwrap_or("unknown"),
            r["tray"].as_str().unwrap_or("unknown"),
            if r["linkEnabled"] == json!(false) { "  [Link off]" } else { "" }
        );
    }
    Ok(())
}

fn shortcuts(loc: &Locations, args: &[String]) -> Result<(), String> {
    if let Some(command @ ("validate" | "markdown")) = args.get(1).map(String::as_str) {
        let file = args.get(2).ok_or("shortcuts needs a file")?;
        let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let markdown = if v.get("app").is_some() {
            let doc = arcade_link::shortcuts::Document::from_json(&text).map_err(|e| e.to_string())?;
            if let Some(file) = value(args, "--manifest") {
                let m = Manifest::from_json(&std::fs::read_to_string(file).map_err(|e| e.to_string())?)?;
                doc.validate(Some(&m)).map_err(|e| e.to_string())?;
            }
            doc.markdown().map_err(|e| e.to_string())?
        } else {
            arcade_link::shortcuts::Sheet::from_json(&text).and_then(|s| s.markdown()).map_err(|e| e.to_string())?
        };
        if command == "markdown" {
            print!("{markdown}");
        } else {
            println!("{file}: valid");
        }
        return Ok(());
    }
    let reg = Registry::load(loc);
    let mut rows = Vec::new();
    for app in reg.apps() {
        for shortcut in &app.shortcuts {
            let normalized = arcade_link::accelerator::normalize(&shortcut.accelerator);
            rows.push(json!({"app": app.id, "id": shortcut.id, "accelerator": shortcut.accelerator,
                "canonical": normalized.as_ref().ok(), "error": normalized.err().map(|e| e.to_string()), "conflicts": []}));
        }
    }
    for a in 0..rows.len() {
        for b in (a + 1)..rows.len() {
            if arcade_link::accelerator::conflicts(rows[a]["accelerator"].as_str().unwrap(), rows[b]["accelerator"].as_str().unwrap()).unwrap_or(false) {
                let other_a = json!({"app": rows[a]["app"], "id": rows[a]["id"]});
                let other_b = json!({"app": rows[b]["app"], "id": rows[b]["id"]});
                rows[a]["conflicts"].as_array_mut().unwrap().push(other_b);
                rows[b]["conflicts"].as_array_mut().unwrap().push(other_a);
            }
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?);
    Ok(())
}

fn manifest_for(loc: &Locations, id: &str) -> Result<Manifest, String> {
    Registry::load(loc).get(id).cloned().ok_or_else(|| {
        let file = loc.registry.join(format!("{id}.json"));
        match file.exists() {
            true => format!("{id} is not installed (the executable in {} is gone)", file.display()),
            false => format!("{id} is not installed (no manifest in {})", loc.registry.display()),
        }
    })
}

fn describe(loc: &Locations, args: &[String]) -> Result<(), String> {
    let id = app_id(args.get(1).ok_or("describe needs an app")?);
    let (source, actions) = match Client::connect(loc, &id, &me()) {
        Ok(mut c) => ("live", c.describe().map_err(|e| e.to_string())?),
        Err(_) => ("manifest", manifest_for(loc, &id)?.actions),
    };
    if flag(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&actions).unwrap_or_default());
        return Ok(());
    }
    println!("{id} ({source}, {} actions)", actions.len());
    for a in actions {
        let mark = if a.available { " " } else { "✗" };
        let flags = format!(
            "{}{}",
            if a.interactive { " interactive" } else { "" },
            if a.effects.is_empty() { String::new() } else { format!(" [{}]", a.effects.join(", ")) }
        );
        println!("{mark} {:<42} {:<32} accepts {}{flags}", a.id, a.title, if a.accepts.is_empty() { "-".into() } else { a.accepts.join(" ") });
        if let Some(r) = &a.reason {
            println!("    unavailable: {r}");
        }
    }
    Ok(())
}

fn inputs(args: &[String]) -> Result<Vec<Content>, String> {
    let mut out = Vec::new();
    let files: Vec<&str> = values(args, "--file");
    match files.len() {
        0 => {}
        1 => out.push(Content::file(std::path::Path::new(files[0]))),
        _ => {
            let paths: Vec<&std::path::Path> = files.iter().map(std::path::Path::new).collect();
            out.push(Content::files(&paths));
        }
    }
    if let Some(t) = value(args, "--text") {
        let mut c = Content::text(value(args, "--type").unwrap_or("text/plain"), t);
        c.hints = values(args, "--hint").into_iter().map(String::from).collect();
        out.push(c);
    }
    if let Some(u) = value(args, "--url") {
        out.push(Content::url(u));
    }
    for j in values(args, "--input-json") {
        out.push(serde_json::from_str(j).map_err(|e| format!("--input-json: {e}"))?);
    }
    Ok(out)
}

fn invoke(loc: &Locations, args: &[String]) -> Result<(), String> {
    let id = app_id(args.get(1).ok_or("invoke needs an app")?);
    let action = args.get(2).ok_or("invoke needs an action")?;
    let mut options = serde_json::Map::new();
    for kv in values(args, "--option") {
        let (k, v) = kv.split_once('=').ok_or_else(|| format!("--option {kv}: expected key=value"))?;
        options.insert(k.into(), serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.into())));
    }
    let mut req = InvokeRequest::new(action, "arcade.link-cli").preset(value(args, "--preset")).options(Value::Object(options));
    req.inputs = inputs(args)?;
    let manifest = manifest_for(loc, &id)?;
    cancel_on_interrupt();
    let mut progress = |p: &arcade_link::JobProgress| {
        let pct = p.fraction.map(|f| format!("{:>3.0}% ", f * 100.0)).unwrap_or_default();
        eprintln!("… {pct}{}", p.message);
    };
    let mut launching = || eprintln!("… starting {id}");
    let started = std::time::Instant::now();
    let result = client::invoke_action(
        loc,
        &me(),
        &manifest,
        &req,
        CallOptions { on_progress: Some(&mut progress), cancel: Some(&CANCEL), on_launching: Some(&mut launching) },
    );
    let elapsed = started.elapsed();
    match result {
        Ok(r) => {
            if flag(args, "--json") {
                println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
            } else {
                if let Some(m) = &r.message {
                    println!("{m}");
                }
                for o in &r.outputs {
                    println!("{}", serde_json::to_string(o).unwrap_or_default());
                }
                if let Some(d) = &r.data {
                    println!("{}", serde_json::to_string_pretty(d).unwrap_or_default());
                }
            }
            eprintln!("({:.1} ms)", elapsed.as_secs_f64() * 1000.0);
            Ok(())
        }
        Err(e) => Err(format!("{} [{}]", e.user_message(&manifest.name), e)),
    }
}

fn simple(loc: &Locations, args: &[String], method: &str, params: Value) -> Result<(), String> {
    let id = app_id(args.get(1).ok_or("needs an app")?);
    let name = arcade_link::manifest::app_name(&id).to_string();
    let shown = |e: arcade_link::LinkError| format!("{} [{}]", e.user_message(&name), e);
    let mut c = Client::connect(loc, &id, &me()).map_err(shown)?;
    let r = c.call(method, params).map_err(shown)?;
    println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
    Ok(())
}

fn watch(loc: &Locations) -> Result<(), String> {
    let shared = arcade_link::SharedRegistry::load(loc);
    let print = |r: &Registry| {
        let ids: Vec<&str> = r.apps().iter().map(|m| m.id.as_str()).collect();
        println!("registry: {}", ids.join(", "));
    };
    shared.with(print);
    let watching = shared.watch(move |r| {
        print(r);
    });
    if !watching {
        return Err("cannot watch the registry on this system".into());
    }
    // Also subscribe to app.changed on every running app.
    for m in shared.snapshot().apps() {
        let id = m.id.clone();
        let loc = loc.clone();
        std::thread::spawn(move || {
            if let Ok(mut c) = Client::connect(&loc, &id, &me()) {
                if c.subscribe(&["app.changed"]).is_ok() {
                    while let Ok(n) = c.next_notification(None) {
                        println!("{id}: {} {}", n.method.unwrap_or_default(), n.params.unwrap_or_default());
                    }
                }
                println!("{id}: connection closed");
            }
        });
    }
    loop {
        std::thread::park();
    }
}

fn check_manifest(args: &[String]) -> Result<(), String> {
    let path = args.get(1).ok_or("check-manifest needs a file")?;
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let m = Manifest::from_json(&text)?;
    println!("{} {} ok: {} actions", m.id, m.version, m.actions.len());
    Ok(())
}
