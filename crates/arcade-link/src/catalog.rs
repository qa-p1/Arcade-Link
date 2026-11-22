//! The shared app list (SPEC §3.1): every Arcade app's name, tagline, release page and "what these two do together"
//! sentences, from Arcade-catalog's `catalog.json`. Arcade Tools keeps a copy next to the registry folder
//! ([`Locations::catalog`]); every app reads that copy, so a new Arcade app needs an entry there, not a Link release.
//! Without a readable copy the compiled values in [`crate::manifest`] apply, so each app still works alone.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::manifest::{self, ids};
use crate::paths::{self, Locations};

/// One app in the catalog. Only the fields Link reads; everything else in the file is left to Tools.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogApp {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tagline: String,
    /// `owner/name` on GitHub.
    #[serde(default)]
    pub repo: String,
    /// One sentence per peer id: what this app and that peer do together.
    #[serde(default)]
    pub pairs: BTreeMap<String, String>,
}

/// The app list. Empty when no catalog could be read: every lookup then falls back to the compiled value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Catalog {
    pub apps: Vec<CatalogApp>,
    /// Corrections to offers (`rules`); empty when absent or malformed.
    pub rules: crate::offers::Rules,
}

impl Catalog {
    /// Reads `catalog.json` text. An entry with the wrong shape is skipped rather than failing the whole list; a
    /// document without an `apps` array is an error.
    pub fn parse(text: &str) -> Result<Catalog, String> {
        let doc: Value = serde_json::from_str(text).map_err(|e| format!("catalog is not JSON: {e}"))?;
        let apps = doc.get("apps").and_then(Value::as_array).ok_or("catalog has no apps list")?;
        // Arcade Tools has its own entry beside the apps.
        let apps = apps
            .iter()
            .chain(doc.get("tools").filter(|t| t.is_object()))
            .filter_map(|v| serde_json::from_value::<CatalogApp>(v.clone()).ok())
            .filter(|a| a.id.starts_with("arcade.") && a.id.len() > 7)
            .collect();
        let rules = doc.get("rules").and_then(|r| serde_json::from_value(r.clone()).ok()).unwrap_or_default();
        Ok(Catalog { apps, rules })
    }

    /// The shared copy, or an empty catalog. Re-read only when the file changes.
    pub fn load(locations: &Locations) -> Catalog {
        static CACHE: Mutex<Option<(PathBuf, Option<SystemTime>, Catalog)>> = Mutex::new(None);
        let path = locations.catalog();
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((p, m, c)) = cache.as_ref() {
            if *p == path && *m == modified && modified.is_some() {
                return c.clone();
            }
        }
        let catalog = std::fs::read_to_string(&path).ok().and_then(|t| Catalog::parse(&t).ok()).unwrap_or_default();
        *cache = Some((path, modified, catalog.clone()));
        catalog
    }

    pub fn app(&self, id: &str) -> Option<&CatalogApp> {
        self.apps.iter().find(|a| a.id == id)
    }

    /// The display name ("Arcade Box").
    pub fn name(&self, id: &str) -> String {
        match self.app(id) {
            Some(a) if !a.name.is_empty() => a.name.clone(),
            _ => manifest::app_name(id).to_string(),
        }
    }

    /// The one-line tagline.
    pub fn tagline(&self, id: &str) -> String {
        match self.app(id) {
            Some(a) if !a.tagline.is_empty() => a.tagline.clone(),
            _ => manifest::app_pitch(id).to_string(),
        }
    }

    /// What `app` and `peer` do together, for the Connected apps page; the peer's tagline when no sentence exists.
    pub fn pair(&self, app: &str, peer: &str) -> String {
        if let Some(s) = self.app(app).and_then(|a| a.pairs.get(peer)).filter(|s| !s.is_empty()) {
            return s.clone();
        }
        let compiled = manifest::peer_description(app, peer);
        if compiled.is_empty() || compiled == manifest::app_pitch(peer) {
            self.tagline(peer)
        } else {
            compiled.to_string()
        }
    }

    /// Where to get the app without Arcade Tools.
    pub fn releases_url(&self, id: &str) -> String {
        match self.app(id) {
            Some(a) if a.repo.split('/').filter(|p| !p.is_empty()).count() == 2 => {
                format!("https://github.com/{}/releases", a.repo)
            }
            _ => manifest::releases_url(id).to_string(),
        }
    }

    /// What Connected apps pages list, without Arcade Tools (it isn't a peer): the known apps the catalog still
    /// lists, in their usual order, then the catalog's other apps in its order.
    pub fn app_ids(&self) -> Vec<String> {
        let listed: Vec<&str> = self.apps.iter().map(|a| a.id.as_str()).filter(|id| *id != ids::TOOLS).collect();
        if listed.is_empty() {
            return ids::APPS.iter().map(|s| s.to_string()).collect();
        }
        let known = ids::APPS.iter().copied().filter(|id| listed.contains(id));
        let new = listed.iter().copied().filter(|id| !ids::APPS.contains(id));
        known.chain(new).map(str::to_string).collect()
    }
}

/// Stores `text` as the shared copy (Arcade Tools calls this after each catalog download). Refuses text that isn't
/// a catalog with at least one app, so a bad download never replaces a good copy. Returns whether the file changed.
pub fn write_shared(locations: &Locations, text: &str) -> io::Result<bool> {
    let parsed = Catalog::parse(text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if parsed.apps.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "catalog lists no apps"));
    }
    let path = locations.catalog();
    if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    paths::write_atomic(&path, text.as_bytes(), false)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_copy_round_trip_and_fallbacks() {
        let dir = std::env::temp_dir().join(format!("arcade-link-catalog-{}", std::process::id()));
        let loc = Locations::under(&dir);
        assert_eq!(Catalog::load(&loc).name(ids::BOX), "Arcade Box", "compiled fallback without a copy");
        assert!(write_shared(&loc, r#"{"apps": []}"#).is_err());
        assert!(write_shared(&loc, "not json").is_err());
        let text = r#"{"apps": [{"id": "arcade.box", "name": "Arcade Box", "repo": "qa-p1/Arcade-box"},
                                {"id": "arcade.paint", "name": "Arcade Paint", "tagline": "Draw.", "repo": "qa-p1/Arcade-paint"}]}"#;
        assert!(write_shared(&loc, text).unwrap());
        assert!(!write_shared(&loc, text).unwrap());
        assert!(!loc.catalog().starts_with(&loc.registry), "the copy must not look like an app's card");
        let c = Catalog::load(&loc);
        assert_eq!(c.name("arcade.paint"), "Arcade Paint");
        assert_eq!(c.releases_url("arcade.paint"), "https://github.com/qa-p1/Arcade-paint/releases");
        assert_eq!(c.app_ids(), ["arcade.box", "arcade.paint"]);
        std::fs::remove_dir_all(dir).ok();
    }
}
