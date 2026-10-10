//! Reading the registry: which Arcade apps are installed and what they offer.
//!
//! Manifests are cached by modification time, so a refresh costs one
//! directory listing plus a `stat` per app. Nothing polls: refresh on demand
//! (off the UI thread), or let [`SharedRegistry::watch`] refresh on change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crate::content::Content;
use crate::manifest::{Action, Manifest, ManifestAdditions, ManifestDocument};
use crate::paths::Locations;

#[derive(Debug, Clone)]
struct Entry {
    modified: Option<SystemTime>,
    len: u64,
    manifest: Option<ManifestDocument>,
}

/// A snapshot of the installed apps.
#[derive(Debug, Clone)]
pub struct Registry {
    dir: PathBuf,
    entries: HashMap<PathBuf, Entry>,
    apps: Vec<Manifest>,
    additions: HashMap<String, ManifestAdditions>,
}

impl Registry {
    /// An empty registry for `locations`; call [`Registry::refresh`] to read it.
    pub fn new(locations: &Locations) -> Registry {
        Registry { dir: locations.registry.clone(), entries: HashMap::new(), apps: Vec::new(), additions: HashMap::new() }
    }

    /// Reads the registry now.
    pub fn load(locations: &Locations) -> Registry {
        let mut r = Registry::new(locations);
        r.refresh();
        r
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Re-reads changed manifests. Returns whether the set of apps changed.
    pub fn refresh(&mut self) -> bool {
        let mut seen = HashMap::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let Ok(meta) = e.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let modified = meta.modified().ok();
                let entry = match self.entries.get(&path) {
                    Some(old) if old.modified == modified && old.len == meta.len() => old.clone(),
                    _ => Entry { modified, len: meta.len(), manifest: std::fs::read_to_string(&path).ok().and_then(|t| ManifestDocument::from_json(&t).ok()) },
                };
                seen.insert(path, entry);
            }
        }
        let docs: Vec<_> = seen.values().filter_map(|e| e.manifest.as_ref()).filter(|d| d.manifest.executable_exists()).collect();
        let additions: HashMap<_, _> = docs.iter().map(|d| (d.manifest.id.clone(), d.additions.clone())).collect();
        let mut apps: Vec<Manifest> = docs.iter().map(|d| d.manifest.clone()).collect();
        apps.sort_by(|a, b| a.id.cmp(&b.id));
        self.entries = seen;
        let changed = apps != self.apps || additions != self.additions;
        self.apps = apps;
        self.additions = additions;
        changed
    }

    /// Installed apps whose executable exists, sorted by ID.
    pub fn apps(&self) -> &[Manifest] {
        &self.apps
    }

    pub fn get(&self, app_id: &str) -> Option<&Manifest> {
        self.apps.iter().find(|m| m.id == app_id)
    }

    pub fn additions(&self, app_id: &str) -> Option<&ManifestAdditions> {
        self.additions.get(app_id)
    }

    pub fn document(&self, app_id: &str) -> Option<ManifestDocument> {
        Some(ManifestDocument { manifest: self.get(app_id)?.clone(), additions: self.additions(app_id).cloned().unwrap_or_default() })
    }

    /// Installed peers other than `me`.
    pub fn peers<'a>(&'a self, me: &'a str) -> impl Iterator<Item = &'a Manifest> + 'a {
        self.apps.iter().filter(move |m| m.id != me)
    }

    /// Every action from a peer (not `me`) that is usable for `content`,
    /// skipping peers in `disabled`.
    pub fn offers_for<'a>(&'a self, me: &'a str, disabled: &'a [String], content: &'a Content) -> Vec<(&'a Manifest, &'a Action)> {
        self.peers(me).filter(|m| !disabled.contains(&m.id)).flat_map(|m| m.usable_actions().filter(|a| a.offer_for(content)).map(move |a| (m, a))).collect()
    }

    /// Effective shortcuts other apps use, for clash warnings: (app name, shortcut id, accelerator).
    pub fn shortcuts<'a>(&'a self, me: &'a str) -> impl Iterator<Item = (&'a str, &'a str, &'a str)> + 'a {
        self.peers(me).flat_map(|m| m.shortcuts.iter().map(move |s| (m.name.as_str(), s.id.as_str(), s.accelerator.as_str())))
    }

    /// The app that already uses `accelerator`, comparing case-insensitively
    /// and ignoring modifier order ("Ctrl+Alt+Space" = "alt+ctrl+space").
    pub fn shortcut_owner(&self, me: &str, accelerator: &str) -> Option<String> {
        let want = normalize_accelerator(accelerator);
        if want.is_empty() {
            return None;
        }
        self.shortcuts(me).find(|(_, _, a)| normalize_accelerator(a) == want).map(|(name, _, _)| name.to_string())
    }
}

/// Canonical form of an accelerator for comparison.
pub fn normalize_accelerator(accelerator: &str) -> String {
    let mut parts: Vec<String> = accelerator
        .split('+')
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .map(|p| match p.as_str() {
            "control" | "ctl" => "ctrl".to_string(),
            "option" | "opt" => "alt".to_string(),
            "command" | "cmd" | "super" | "meta" | "win" | "logo" => "super".to_string(),
            _ => p,
        })
        .collect();
    let key = parts.pop().unwrap_or_default();
    parts.sort();
    parts.push(key);
    parts.join("+")
}

type Listener = Box<dyn Fn(&Registry) + Send + Sync>;

/// A registry shared between threads, refreshed by a lazily started watcher.
#[derive(Clone)]
pub struct SharedRegistry {
    inner: Arc<RwLock<Registry>>,
    locations: Locations,
    #[allow(dead_code)]
    watcher: Arc<std::sync::Mutex<Option<Box<dyn std::any::Any + Send>>>>,
}

impl SharedRegistry {
    /// Reads the registry once (call it off the UI thread).
    pub fn load(locations: &Locations) -> SharedRegistry {
        SharedRegistry { inner: Arc::new(RwLock::new(Registry::load(locations))), locations: locations.clone(), watcher: Arc::new(std::sync::Mutex::new(None)) }
    }

    /// The cached snapshot. Never touches the disk.
    pub fn snapshot(&self) -> Registry {
        self.inner.read().map(|r| r.clone()).unwrap_or_else(|_| Registry::new(&self.locations))
    }

    /// Runs `f` with the cached registry. Never touches the disk.
    pub fn with<T>(&self, f: impl FnOnce(&Registry) -> T) -> T {
        match self.inner.read() {
            Ok(r) => f(&r),
            Err(_) => f(&Registry::new(&self.locations)),
        }
    }

    /// Re-reads changed manifests now. Returns whether anything changed.
    pub fn refresh(&self) -> bool {
        self.inner.write().map(|mut r| r.refresh()).unwrap_or(false)
    }

    /// Starts watching the registry and runtime directories; on any change
    /// the registry is refreshed and `on_change` runs (on the watcher thread)
    /// with the new snapshot. The watcher blocks on OS notifications: no
    /// timers and no polling. Returns false if watching isn't possible.
    #[cfg(feature = "watch")]
    pub fn watch(&self, on_change: impl Fn(&Registry) + Send + Sync + 'static) -> bool {
        use notify::{RecursiveMode, Watcher};
        let listener: Listener = Box::new(on_change);
        let inner = self.inner.clone();
        let handler = move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            // Readers opening and reading endpoint files and manifests (every
            // probe does) must not count as changes, or watchers that probe
            // on change would wake each other forever.
            if is_read_only(&event.kind) {
                return;
            }
            let changed_runtime = event.paths.iter().any(|p| p.extension().is_some_and(|x| x == "endpoint"));
            let snapshot = match inner.write() {
                Ok(mut r) => {
                    let changed = r.refresh();
                    if !changed && !changed_runtime {
                        return;
                    }
                    r.clone()
                }
                Err(_) => return,
            };
            listener(&snapshot);
        };
        let mut watcher = match notify::recommended_watcher(handler) {
            Ok(w) => w,
            Err(_) => return false,
        };
        let _ = std::fs::create_dir_all(&self.locations.registry);
        let _ = crate::paths::ensure_private_dir(&self.locations.runtime);
        let ok = watcher.watch(&self.locations.registry, RecursiveMode::NonRecursive).is_ok();
        let _ = watcher.watch(&self.locations.runtime, RecursiveMode::NonRecursive);
        if let Ok(mut slot) = self.watcher.lock() {
            *slot = Some(Box::new(watcher));
        }
        ok
    }

    /// Without the `watch` feature, callers refresh on demand.
    #[cfg(not(feature = "watch"))]
    pub fn watch(&self, _on_change: impl Fn(&Registry) + Send + Sync + 'static) -> bool {
        let _: Option<Listener> = None;
        false
    }
}

/// An event that only reports a read (open, read, close without writing).
/// Closing a file that was written still counts: inotify reports some writes
/// only that way.
#[cfg(feature = "watch")]
fn is_read_only(kind: &notify::EventKind) -> bool {
    use notify::event::{AccessKind, AccessMode};
    matches!(kind, notify::EventKind::Access(a) if *a != AccessKind::Close(AccessMode::Write))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{write_manifest, Shortcut};

    #[test]
    fn ignores_missing_executables_and_caches() {
        let dir = std::env::temp_dir().join(format!("arcade-link-registry-{}", std::process::id()));
        let loc = Locations::under(&dir);
        let exe = std::env::current_exe().unwrap();
        let mut good = Manifest::new("arcade.good", "1", exe.to_str().unwrap());
        good.name = "Arcade Good".into();
        good.shortcuts.push(Shortcut { id: "main".into(), accelerator: "Ctrl+Alt+Space".into() });
        write_manifest(&loc, &good).unwrap();
        write_manifest(&loc, &Manifest::new("arcade.gone", "1", "/nonexistent/app")).unwrap();
        std::fs::write(loc.registry.join("junk.json"), "{not json").unwrap();
        let mut r = Registry::load(&loc);
        assert_eq!(r.apps().len(), 1);
        assert!(!r.refresh());
        assert_eq!(r.shortcut_owner("arcade.me", "alt+ctrl+space").as_deref(), Some("Arcade Good"));
        assert_eq!(r.shortcut_owner("arcade.good", "Ctrl+Alt+Space"), None);
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(feature = "watch")]
    #[test]
    fn watcher_ignores_reads_of_endpoint_files() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let dir = std::env::temp_dir().join(format!("arcade-link-watch-{}", std::process::id()));
        let loc = Locations::under(&dir);
        let shared = SharedRegistry::load(&loc);
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        assert!(shared.watch(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        let settle = || std::thread::sleep(std::time::Duration::from_millis(300));
        let endpoint = loc.runtime.join("arcade.test.endpoint");
        let tmp = loc.runtime.join("arcade.test.endpoint.tmp");
        std::fs::write(&tmp, "{}").unwrap();
        std::fs::rename(&tmp, &endpoint).unwrap();
        settle();
        let after_write = calls.load(Ordering::SeqCst);
        assert!(after_write >= 1, "a new endpoint is a change");
        for _ in 0..5 {
            std::fs::read_to_string(&endpoint).unwrap();
        }
        settle();
        assert_eq!(calls.load(Ordering::SeqCst), after_write, "reading an endpoint is not a change");
        std::fs::remove_dir_all(dir).ok();
    }
}
