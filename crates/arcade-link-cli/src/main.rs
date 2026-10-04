//! `arcade-link`: inspect and drive Arcade apps over the Link.
//!
//! Essential for development and for bug reports. Every command honors
//! `ARCADE_HOME`.

mod mock;

use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

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

<app> is a canonical ID (arcade.box) or its short form (box).";

fn me() -> PeerInfo {
    PeerInfo {
        id: "arcade.link-cli".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
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
    args.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].as_str())
        .collect()
}

fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    values(args, name).into_iter().next()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let loc = Locations::discover();
    let r = match args.first().map(String::as_str) {
        Some("ls") => ls(&loc, &args),
        Some("describe") => describe(&loc, &args),
        Some("invoke") => invoke(&loc, &args),
        Some("status") => simple(&loc, &args, "app.status", json!({})),
        Some("activate") => simple(&loc, &args, "app.activate", json!({})),
        Some("quit") => simple(
            &loc,
            &args,
            "app.quit",
            json!({ "force": flag(&args, "--force") }),
        ),
        Some("watch") => watch(&loc),
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
            json!({
                "id": m.id, "name": m.name, "version": m.version, "state": state,
                "linkEnabled": m.settings.link_enabled, "actions": m.actions.len(),
                "available": m.usable_actions().count(), "executable": m.executable,
            })
        })
        .collect();
    if flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
        return Ok(());
    }
    println!("registry: {}", loc.registry.display());
    if rows.is_empty() {
        println!("(no Arcade apps registered)");
    }
    for r in rows {
        println!(
            "{:<18} {:<18} {:<10} {:<18} {:>3} actions ({} available){}",
            r["id"].as_str().unwrap_or(""),
            r["name"].as_str().unwrap_or(""),
            r["version"].as_str().unwrap_or(""),
            r["state"].as_str().unwrap_or(""),
            r["actions"],
            r["available"],
            if r["linkEnabled"] == json!(false) {
                "  [Link off]"
            } else {
                ""
            }
        );
    }
    Ok(())
}

fn manifest_for(loc: &Locations, id: &str) -> Result<Manifest, String> {
    Registry::load(loc).get(id).cloned().ok_or_else(|| {
        format!(
            "{id} is not installed (no manifest in {})",
            loc.registry.display()
        )
    })
}

fn describe(loc: &Locations, args: &[String]) -> Result<(), String> {
    let id = app_id(args.get(1).ok_or("describe needs an app")?);
    let (source, actions) = match Client::connect(loc, &id, &me()) {
        Ok(mut c) => ("live", c.describe().map_err(|e| e.to_string())?),
        Err(_) => ("manifest", manifest_for(loc, &id)?.actions),
    };
    if flag(args, "--json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&actions).unwrap_or_default()
        );
        return Ok(());
    }
    println!("{id} ({source}, {} actions)", actions.len());
    for a in actions {
        let mark = if a.available { " " } else { "✗" };
        let flags = format!(
            "{}{}",
            if a.interactive { " interactive" } else { "" },
            if a.effects.is_empty() {
                String::new()
            } else {
                format!(" [{}]", a.effects.join(", "))
            }
        );
        println!(
            "{mark} {:<42} {:<32} accepts {}{flags}",
            a.id,
            a.title,
            if a.accepts.is_empty() {
                "-".into()
            } else {
                a.accepts.join(" ")
            }
        );
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
        c.hints = values(args, "--hint")
            .into_iter()
            .map(String::from)
            .collect();
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
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| format!("--option {kv}: expected key=value"))?;
        options.insert(
            k.into(),
            serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.into())),
        );
    }
    let mut req = InvokeRequest::new(action, "arcade.link-cli")
        .preset(value(args, "--preset"))
        .options(Value::Object(options));
    req.inputs = inputs(args)?;
    let manifest = manifest_for(loc, &id)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let mut progress = |p: &arcade_link::JobProgress| {
        let pct = p
            .fraction
            .map(|f| format!("{:>3.0}% ", f * 100.0))
            .unwrap_or_default();
        eprintln!("… {pct}{}", p.message);
    };
    let mut launching = || eprintln!("… starting {id}");
    let started = std::time::Instant::now();
    let result = client::invoke_action(
        loc,
        &me(),
        &manifest,
        &req,
        CallOptions {
            on_progress: Some(&mut progress),
            cancel: Some(&cancel),
            on_launching: Some(&mut launching),
        },
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
    let mut c = Client::connect(loc, &id, &me()).map_err(|e| e.to_string())?;
    let r = c.call(method, params).map_err(|e| e.to_string())?;
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
                        println!(
                            "{id}: {} {}",
                            n.method.unwrap_or_default(),
                            n.params.unwrap_or_default()
                        );
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
