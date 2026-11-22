//! What the browser extension can ask (see [`crate::native`] for the framing). Answers come from the same shared
//! step every app uses (`offers`), and runs go to the resident, so the browser sees exactly what the file menu
//! sees.
//!
//! | method | params → result |
//! |---|---|
//! | `hello` | `{}` → `{helper, browser, apps:[{id, name, installed, releases}]}` |
//! | `offers` | `{surface, items:[{type, size?, hints?}]}` → `{offers:[Offer + appName]}` |
//! | `form` | `{ref}` → `{title, app, options?}` |
//! | `run` | `{ref, items:[BrowserItem], values?}` → `{job}` |
//! | `jobs` | `{since, waitMs}` → `{revision, jobs}` |
//! | `cancel`, `dismiss` | `{job}` → `{}` |
//! | `prefs.get` / `prefs.set` | `{}` / `{prefs}` → the preferences |
//! | `install` | `{app}` → `{via: "tools" \| "page", url?}` |
//!
//! A `BrowserItem` is `{type, text?}` (text, links, pages), `{type, path}` (a finished download) or
//! `{type, name, data}` (base64 bytes the extension fetched, such as an image on the page).

use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arcade_link::catalog::Catalog;
use arcade_link::content::Content;
use arcade_link::handoff::Handoff;
use arcade_link::offers::{self, Item, Prefs};
use arcade_link::registry::Registry;
use arcade_link::{LinkError, Locations};

use crate::client::HelperClient;
use crate::ids;
use crate::native::{self, Caller};
use crate::resident::{self, RunSpec};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BrowserItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub data: Option<String>,
    pub hints: Vec<String>,
}

/// The largest file the extension may hand over as bytes.
pub const MAX_DATA: usize = 48 * 1024 * 1024;

pub struct Bridge {
    pub locations: Locations,
    pub exe: PathBuf,
    pub caller: Caller,
}

fn err(e: LinkError) -> (String, String) {
    (e.code.as_str().to_string(), e.message)
}

fn bad(message: &str) -> (String, String) {
    ("denied".into(), message.into())
}

impl Bridge {
    fn client(&self) -> Result<HelperClient, (String, String)> {
        HelperClient::connect(&self.locations, &self.exe, "browser").map_err(err)
    }

    /// One request's result, or `(code, message)`.
    pub fn handle(&self, method: &str, params: &Value) -> Result<Value, (String, String)> {
        let catalog = Catalog::load(&self.locations);
        let registry = Registry::load(&self.locations);
        let s = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        match method {
            "hello" => {
                let apps: Vec<Value> = catalog
                    .app_ids()
                    .iter()
                    .map(|id| json!({"id": id, "name": catalog.name(id), "installed": registry.get(id).is_some_and(|m| m.settings.link_enabled), "releases": catalog.releases_url(id)}))
                    .collect();
                Ok(json!({"helper": env!("CARGO_PKG_VERSION"), "browser": self.caller.browser(), "apps": apps}))
            }
            "offers" => {
                let items: Vec<Item> = serde_json::from_value(params.get("items").cloned().unwrap_or_default()).map_err(|e| bad(&e.to_string()))?;
                let prefs = Prefs::load(&self.locations);
                let list: Vec<Value> = offers::offers(&items, &s("surface"), registry.apps(), &catalog.rules, &prefs)
                    .into_iter()
                    .filter(|o| o.app != ids::HELPER_ID)
                    .map(|o| {
                        let mut v = serde_json::to_value(&o).unwrap_or_default();
                        v["appName"] = json!(catalog.name(&o.app));
                        v["ref"] = json!(o.reference());
                        v
                    })
                    .collect();
                Ok(json!({"offers": list}))
            }
            "form" => {
                let reference = s("ref");
                let (app, _, _) = crate::jobs::parse_reference(&reference).ok_or_else(|| bad("not an action reference"))?;
                let card = registry.get(app).ok_or_else(|| bad("that app isn't installed"))?;
                let action = resident::action_of(card, &reference).ok_or_else(|| bad("no such action"))?;
                Ok(json!({"title": action.title, "app": catalog.name(app), "options": action.options, "oneClick": action.is_one_click()}))
            }
            "run" => {
                let items: Vec<BrowserItem> = serde_json::from_value(params.get("items").cloned().unwrap_or_default()).map_err(|e| bad(&e.to_string()))?;
                let inputs = self.inputs(&items)?;
                let values: Map<String, Value> = params.get("values").and_then(Value::as_object).cloned().unwrap_or_default();
                let spec = RunSpec { reference: s("ref"), inputs, values, origin: "browser".into(), ..RunSpec::default() };
                let job = self.client()?.run(&spec).map_err(err)?;
                Ok(json!({"job": job}))
            }
            "jobs" => {
                let since = params.get("since").and_then(Value::as_u64).unwrap_or(0);
                let wait = Duration::from_millis(params.get("waitMs").and_then(Value::as_u64).unwrap_or(0));
                let (revision, jobs) = self.client()?.jobs(since, wait, false).map_err(err)?;
                Ok(json!({"revision": revision, "jobs": jobs}))
            }
            "cancel" => {
                let job = params.get("job").and_then(Value::as_u64).ok_or_else(|| bad("needs a job"))?;
                self.client()?.cancel(job).map_err(err)?;
                Ok(json!({}))
            }
            "dismiss" => {
                self.client()?.dismiss(params.get("job").and_then(Value::as_u64)).map_err(err)?;
                Ok(json!({}))
            }
            "prefs.get" => Ok(serde_json::to_value(Prefs::load(&self.locations)).unwrap_or_default()),
            "prefs.set" => {
                let prefs: Prefs = serde_json::from_value(params.get("prefs").cloned().unwrap_or_default()).map_err(|e| bad(&e.to_string()))?;
                prefs.save(&self.locations).map_err(|e| bad(&e.to_string()))?;
                // Pins changed menus; the resident rewrites them.
                if let Ok(mut c) = self.client() {
                    c.integrate().ok();
                }
                Ok(serde_json::to_value(prefs).unwrap_or_default())
            }
            "install" => {
                let app = s("app");
                if !app.starts_with("arcade.") {
                    return Err(bad("not an Arcade app"));
                }
                let me = arcade_link::wire::PeerInfo { id: ids::HELPER_ID.into(), version: env!("CARGO_PKG_VERSION").into() };
                match arcade_link::client::request_install(&self.locations, &me, &app) {
                    Ok(()) => Ok(json!({"via": "tools"})),
                    Err(_) => Ok(json!({"via": "page", "url": catalog.releases_url(&app)})),
                }
            }
            _ => Err(("unsupported".into(), format!("unknown method {method}"))),
        }
    }

    /// The browser's items as Link inputs. Bytes go to a handoff file the helper owns (removed with stale handoff
    /// folders after a day, since the job outlives this process).
    fn inputs(&self, items: &[BrowserItem]) -> Result<Vec<Content>, (String, String)> {
        let mut out = Vec::new();
        let mut handoff: Option<Handoff> = None;
        for item in items {
            if let Some(path) = &item.path {
                let p = Path::new(path);
                if !p.is_absolute() || !p.exists() {
                    return Err(bad("the downloaded file isn't there"));
                }
                out.push(crate::jobs::file_content(p));
            } else if let Some(data) = &item.data {
                if data.len() > MAX_DATA / 3 * 4 + 4 {
                    return Err(bad("that file is too large to hand over"));
                }
                let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|_| bad("the file data is damaged"))?;
                let name = safe_name(item.name.as_deref().unwrap_or("download"));
                if handoff.is_none() {
                    handoff = Some(Handoff::create(&self.locations, ids::HELPER_ID).map_err(|e| bad(&e.to_string()))?);
                }
                let mut c = handoff.as_ref().map(|h| h.file(&name, &bytes)).transpose().map_err(|e| bad(&e.to_string()))?.unwrap_or_default();
                if item.kind.starts_with("file/") {
                    c.kind = item.kind.clone();
                }
                out.push(c);
            } else if let Some(text) = &item.text {
                let mut c = if item.kind == "text/url" { Content::url(text) } else { Content::text(&item.kind, text) };
                c.hints = item.hints.clone();
                out.push(c);
            } else {
                return Err(bad("an item has no text, path or data"));
            }
        }
        if let Some(h) = handoff {
            h.keep();
        }
        Ok(out)
    }
}

/// A file name the extension suggested, reduced to something safe on every OS.
pub fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base.chars().map(|c| if c.is_control() || r#"<>:"|?*"#.contains(c) { '_' } else { c }).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').chars().take(120).collect::<String>();
    if cleaned.is_empty() {
        "download".into()
    } else {
        cleaned
    }
}

/// Serves the extension until it disconnects. Each request is answered on its own thread, so a long `jobs` wait
/// never holds up the others.
pub fn serve(bridge: Bridge, input: &mut impl Read, output: impl Write + Send + 'static) -> std::io::Result<()> {
    if !bridge.caller.allowed() {
        let mut output = output;
        native::write_message(&mut output, &json!({"error": {"code": "denied", "message": "only the Arcade extension may use the helper"}}))?;
        return Ok(());
    }
    let bridge = Arc::new(bridge);
    let out = Arc::new(Mutex::new(output));
    let mut threads = Vec::new();
    while let Some(msg) = native::read_message(input)? {
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let (bridge, out) = (bridge.clone(), out.clone());
        threads.push(std::thread::spawn(move || {
            let answer = match bridge.handle(&method, &params) {
                Ok(r) => native::result(&id, r),
                Err((code, message)) => native::error(&id, &code, &message),
            };
            let mut w = out.lock().unwrap_or_else(|e| e.into_inner());
            if native::write_message(&mut *w, &answer).is_err() {
                native::write_message(&mut *w, &native::error(&id, "internal", "the answer was too large")).ok();
            }
        }));
        threads.retain(|t| !t.is_finished());
    }
    for t in threads {
        t.join().ok();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use arcade_link::manifest::{write_manifest, Manifest};

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn locations(name: &str) -> (PathBuf, Locations) {
        let root = std::env::temp_dir().join(format!("arcade-bridge-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let l = Locations::under(&root);
        for d in [&l.registry, &l.runtime, &l.handoff] {
            std::fs::create_dir_all(d).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        (root, l)
    }

    /// Sends framed requests the way a browser does; the answers by id.
    fn talk(bridge: Bridge, requests: &[Value]) -> Vec<Value> {
        let mut input = Vec::new();
        for r in requests {
            native::write_message(&mut input, r).unwrap();
        }
        let sink = Sink::default();
        serve(bridge, &mut std::io::Cursor::new(input), sink.clone()).unwrap();
        let bytes = sink.0.lock().unwrap().clone();
        let mut cursor = std::io::Cursor::new(bytes);
        let mut out = Vec::new();
        while let Some(m) = native::read_message(&mut cursor).unwrap() {
            out.push(m);
        }
        out.sort_by_key(|m| m["id"].as_u64());
        out
    }

    fn chrome() -> Caller {
        Caller::Chromium { extension_id: ids::CHROME_EXTENSION_ID.into() }
    }

    #[test]
    fn other_extensions_are_refused() {
        let (root, l) = locations("refused");
        let bridge = Bridge { locations: l, exe: "/nonexistent".into(), caller: Caller::Chromium { extension_id: "a".repeat(32) } };
        let out = talk(bridge, &[json!({"id": 1, "method": "hello"})]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["error"]["code"], "denied");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn hello_offers_and_form_answer_from_the_cards() {
        let (root, l) = locations("offers");
        let exe = std::env::current_exe().unwrap();
        let mut m = Manifest::new("arcade.paint", "1.0.0", &exe.to_string_lossy());
        m.actions = vec![serde_json::from_value(json!({"id": "paint.invert", "title": "Invert colors", "accepts": ["file/image"], "oneClick": true})).unwrap()];
        write_manifest(&l, &m).unwrap();
        let bridge = Bridge { locations: l, exe: "/nonexistent".into(), caller: chrome() };
        let out = talk(
            bridge,
            &[
                json!({"id": 1, "method": "hello"}),
                json!({"id": 2, "method": "offers", "params": {"surface": "", "items": [{"type": "file/image"}]}}),
                json!({"id": 3, "method": "form", "params": {"ref": "arcade.paint/paint.invert"}}),
                json!({"id": 4, "method": "nope"}),
                json!({"id": 5, "method": "install", "params": {"app": "evil.app"}}),
                json!({"id": 6, "method": "install", "params": {"app": "arcade.box"}}),
            ],
        );
        assert_eq!(out.len(), 6, "{out:?}");
        assert_eq!(out[0]["result"]["browser"], "chromium");
        let offers = out[1]["result"]["offers"].as_array().unwrap();
        assert_eq!(offers.len(), 1, "{offers:?}");
        assert_eq!((offers[0]["ref"].as_str(), offers[0]["title"].as_str()), (Some("arcade.paint/paint.invert"), Some("Invert colors")));
        assert_eq!((out[2]["result"]["title"].as_str(), out[2]["result"]["oneClick"].as_bool()), (Some("Invert colors"), Some(true)));
        assert_eq!(out[3]["error"]["code"], "unsupported");
        assert_eq!(out[4]["error"]["code"], "denied");
        // Without Arcade Tools the extension opens the app's release page instead.
        assert_eq!(out[5]["result"]["via"], "page");
        assert!(out[5]["result"]["url"].as_str().unwrap().starts_with("https://"), "{:?}", out[5]);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn browser_bytes_become_a_handoff_file() {
        let (root, l) = locations("bytes");
        let bridge = Bridge { locations: l.clone(), exe: "/nonexistent".into(), caller: chrome() };
        let data = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG fake");
        let items = [
            BrowserItem { kind: "file/image".into(), name: Some("../cat?.png".into()), data: Some(data), ..Default::default() },
            BrowserItem { kind: "text/url".into(), text: Some("https://example.com/".into()), ..Default::default() },
        ];
        let inputs = bridge.inputs(&items).unwrap();
        let path = PathBuf::from(inputs[0].path.as_deref().unwrap());
        assert!(path.starts_with(&l.handoff), "{path:?}");
        assert_eq!(path.file_name().unwrap(), "cat_.png");
        assert_eq!(std::fs::read(&path).unwrap(), b"\x89PNG fake");
        assert_eq!(inputs[1].kind, "text/url");
        let bad = [BrowserItem { kind: "file/image".into(), data: Some("***".into()), ..Default::default() }];
        assert!(bridge.inputs(&bad).is_err());
        let missing = [BrowserItem { kind: "file/image".into(), path: Some("relative.png".into()), ..Default::default() }];
        assert!(bridge.inputs(&missing).is_err());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name(r"C:\x\photo?.png"), "photo_.png");
        assert_eq!(safe_name(".hidden"), "hidden");
        assert_eq!(safe_name(""), "download");
    }
}
