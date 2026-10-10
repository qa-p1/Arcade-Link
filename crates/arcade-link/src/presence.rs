//! An app's presence: its manifest plus its endpoint, kept in step with the
//! "Connect with other Arcade apps" switch (SPEC §3, §8.2).

use std::sync::{Arc, Mutex};

use crate::handoff;
use crate::manifest::{self, Manifest, ManifestDocument};
use crate::paths::Locations;
use crate::server::{Handler, Server, ServerConfig};
use crate::wire::PeerInfo;

/// Owns the manifest and the server. Create it on a background thread,
/// after the app's first frame: it writes a file, binds a socket and lists
/// the handoff directory.
pub struct Presence {
    locations: Locations,
    handler: Arc<dyn Handler>,
    state: Mutex<State>,
}

struct State {
    manifest: Manifest,
    server: Option<Server>,
    last_error: Option<String>,
    additions: manifest::ManifestAdditions,
}

impl Presence {
    /// Writes the manifest and, if the Link is enabled, starts listening.
    /// With the master switch off the manifest has no actions and nothing
    /// listens, but the app still shows up as installed.
    pub fn start(locations: Locations, manifest: Manifest, handler: Arc<dyn Handler>) -> Presence {
        Self::start_document(locations, manifest.into(), handler)
    }

    pub fn start_document(locations: Locations, document: ManifestDocument, handler: Arc<dyn Handler>) -> Presence {
        let p = Presence {
            locations,
            handler,
            state: Mutex::new(State { manifest: document.manifest.clone(), additions: document.additions.clone(), server: None, last_error: None }),
        };
        p.apply(document, true);
        handoff::cleanup_stale(&p.locations);
        p
    }

    /// Rewrites the manifest (only if it changed), starts or stops the
    /// server to match `settings.linkEnabled`, and tells subscribers.
    pub fn update(&self, manifest: Manifest) {
        let additions = self.state.lock().unwrap_or_else(|e| e.into_inner()).additions.clone();
        self.apply(ManifestDocument { manifest, additions }, false);
    }

    pub fn update_document(&self, document: ManifestDocument) {
        self.apply(document, false);
    }

    fn apply(&self, mut document: ManifestDocument, first: bool) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let enabled = document.manifest.settings.link_enabled;
        if !enabled {
            document.manifest.actions.clear();
        }
        let menu_changed = document.additions.menu != st.additions.menu;
        let changed = match manifest::write_document(&self.locations, &document) {
            Ok(c) => c,
            Err(e) => {
                st.last_error = Some(format!("could not write the manifest: {e}"));
                false
            }
        };
        if enabled && st.server.is_none() {
            let config = ServerConfig {
                app: PeerInfo { id: document.manifest.id.clone(), version: document.manifest.version.clone() },
                locations: self.locations.clone(),
            };
            match Server::start(config, self.handler.clone()) {
                Ok(s) => {
                    st.server = Some(s);
                    st.last_error = None;
                }
                Err(e) => st.last_error = Some(format!("could not listen: {e}")),
            }
        } else if !enabled {
            st.server = None;
        }
        if changed && !first {
            if let Some(s) = &st.server {
                s.notify_changed();
                if menu_changed {
                    s.notify_menu_changed();
                }
            }
        }
        st.manifest = document.manifest;
        st.additions = document.additions;
    }

    pub fn manifest(&self) -> Manifest {
        self.state.lock().map(|s| s.manifest.clone()).unwrap_or_else(|e| e.into_inner().manifest.clone())
    }

    pub fn locations(&self) -> &Locations {
        &self.locations
    }

    /// Listening right now.
    pub fn listening(&self) -> bool {
        self.state.lock().map(|s| s.server.is_some()).unwrap_or(false)
    }

    /// Jobs are running (`app.quit` answers `busy`).
    pub fn busy(&self) -> bool {
        self.state.lock().map(|s| s.server.as_ref().is_some_and(Server::busy)).unwrap_or(false)
    }

    /// The last problem writing the manifest or listening, for diagnostics.
    pub fn last_error(&self) -> Option<String> {
        self.state.lock().ok().and_then(|s| s.last_error.clone())
    }

    /// Stops listening and removes the endpoint file (the manifest stays).
    pub fn stop(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.server = None;
        }
    }
}

/// The diagnostics block every Connected apps page shows.
pub fn diagnostics(presence: Option<&Presence>, locations: &Locations) -> Vec<(String, String)> {
    let mut rows = vec![("Registry".to_string(), locations.registry.display().to_string()), ("Runtime".to_string(), locations.runtime.display().to_string())];
    match presence {
        Some(p) => {
            rows.push(("Endpoint".into(), if p.listening() { "listening".into() } else { "not listening".into() }));
            rows.push(("Last error".into(), p.last_error().unwrap_or_else(|| "none".into())));
        }
        None => rows.push(("Endpoint".into(), "not started".into())),
    }
    rows
}
