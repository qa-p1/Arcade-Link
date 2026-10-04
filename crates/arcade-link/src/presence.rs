//! An app's presence: its manifest plus its endpoint, kept in step with the
//! "Connect with other Arcade apps" switch (SPEC §3, §8.2).

use std::sync::{Arc, Mutex};

use crate::handoff;
use crate::manifest::{self, Manifest};
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
}

impl Presence {
    /// Writes the manifest and, if the Link is enabled, starts listening.
    /// With the master switch off the manifest has no actions and nothing
    /// listens, but the app still shows up as installed.
    pub fn start(locations: Locations, manifest: Manifest, handler: Arc<dyn Handler>) -> Presence {
        let p = Presence {
            locations,
            handler,
            state: Mutex::new(State {
                manifest: manifest.clone(),
                server: None,
                last_error: None,
            }),
        };
        p.apply(manifest, true);
        handoff::cleanup_stale(&p.locations);
        p
    }

    /// Rewrites the manifest (only if it changed), starts or stops the
    /// server to match `settings.linkEnabled`, and tells subscribers.
    pub fn update(&self, manifest: Manifest) {
        self.apply(manifest, false);
    }

    fn apply(&self, mut manifest: Manifest, first: bool) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let enabled = manifest.settings.link_enabled;
        if !enabled {
            manifest.actions.clear();
        }
        let changed = match manifest::write_manifest(&self.locations, &manifest) {
            Ok(c) => c,
            Err(e) => {
                st.last_error = Some(format!("could not write the manifest: {e}"));
                false
            }
        };
        if enabled && st.server.is_none() {
            let config = ServerConfig {
                app: PeerInfo {
                    id: manifest.id.clone(),
                    version: manifest.version.clone(),
                },
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
            }
        }
        st.manifest = manifest;
    }

    pub fn manifest(&self) -> Manifest {
        self.state
            .lock()
            .map(|s| s.manifest.clone())
            .unwrap_or_else(|e| e.into_inner().manifest.clone())
    }

    pub fn locations(&self) -> &Locations {
        &self.locations
    }

    /// Listening right now.
    pub fn listening(&self) -> bool {
        self.state
            .lock()
            .map(|s| s.server.is_some())
            .unwrap_or(false)
    }

    /// Jobs are running (`app.quit` answers `busy`).
    pub fn busy(&self) -> bool {
        self.state
            .lock()
            .map(|s| s.server.as_ref().is_some_and(Server::busy))
            .unwrap_or(false)
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
    let mut rows = vec![
        (
            "Registry".to_string(),
            locations.registry.display().to_string(),
        ),
        (
            "Runtime".to_string(),
            locations.runtime.display().to_string(),
        ),
    ];
    match presence {
        Some(p) => {
            rows.push((
                "Endpoint".into(),
                if p.listening() {
                    "listening".into()
                } else {
                    "not listening".into()
                },
            ));
            rows.push((
                "Last error".into(),
                p.last_error().unwrap_or_else(|| "none".into()),
            ));
        }
        None => rows.push(("Endpoint".into(), "not started".into())),
    }
    rows
}
