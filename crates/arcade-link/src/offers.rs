//! "What can I do with this?" (SPEC §5.5): one answer shared by every caller (apps, the file manager menus, the
//! browser extension), so no caller keeps its own list of other apps' actions. A pure function over the installed
//! cards, the catalog's `rules` and the user's choices (`offers.json`); Qt implements the same rules, pinned by
//! `spec/vectors/offers.json`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io;

use crate::content::{self, Content};
use crate::error::format_limit;
use crate::manifest::{Action, Manifest};
use crate::paths::{self, Locations};

/// One thing the user has selected: its type (`file/csv`, `text/url`, …) and what callers know about it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl Item {
    pub fn new(kind: &str) -> Item {
        Item { kind: kind.into(), ..Item::default() }
    }
}

/// Corrections kept in Arcade-catalog's `catalog.json` (`rules`), so a wiring mistake is fixed by editing data.
/// Keys are action references, `<app-id>/<action-id>`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// Never offered.
    pub hide: Vec<String>,
    /// Shown with this title.
    pub rename: BTreeMap<String, String>,
    /// Added to the rank for offered types a pattern takes: `{ref: {pattern: points}}`.
    pub boost: BTreeMap<String, BTreeMap<String, i32>>,
    /// Treated as doing this job (overrides the card's `job`).
    pub jobs: BTreeMap<String, String>,
}

/// The user's choices (`offers.json`, [`Locations::offer_prefs`]).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// The chosen action per job: `{"image.read-text": "arcade.lens/lens.recognize"}`.
    pub defaults: BTreeMap<String, String>,
    /// Actions the user removed.
    pub hidden: Vec<String>,
    /// Actions the user keeps on top.
    pub pinned: Vec<String>,
    /// Where results go: a folder, or empty for each action's own default.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub output_folder: String,
    /// Ask where to save every time.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub ask_where: bool,
}

impl Prefs {
    /// The saved choices, or none. A malformed file reads as none (it is the user's, never Link's to repair).
    pub fn load(locations: &Locations) -> Prefs {
        std::fs::read_to_string(locations.offer_prefs())
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map(|v| serde_json::from_value(v).unwrap_or_default())
            .unwrap_or_default()
    }

    pub fn save(&self, locations: &Locations) -> io::Result<()> {
        let path = locations.offer_prefs();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        paths::write_atomic(&path, text.as_bytes(), false)
    }
}

/// One entry to show.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub app: String,
    pub action: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub rank: i32,
    /// False: shown greyed out with `reason`, or as an install entry when `install` is set.
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What to install first (`engine:ffmpeg`, `app:arcade.box`) for an action that is unavailable without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
    /// The user's default for its job.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub preferred: bool,
    /// Another app's way of doing a job that has a default: shown under "Other apps".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub other: bool,
    /// Two or more apps do this job and the user hasn't picked one: callers show all and offer "Always use …".
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub choose: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub one_click: bool,
}

impl Offer {
    /// `<app-id>/<action-id>`, the key `Rules` and `Prefs` use.
    pub fn reference(&self) -> String {
        reference(&self.app, &self.action)
    }
}

pub fn reference(app: &str, action: &str) -> String {
    format!("{app}/{action}")
}

/// The type a selection is offered as: the item's own type for one item; for several, the array of the narrowest
/// type every item is (`file/csv` + `file/spreadsheet` → `file/spreadsheet[]`, mixed files → `file/any[]`). `None`
/// for nothing selected, or a mix that has no array type (text with files, several folders).
pub fn input_type(items: &[Item]) -> Option<String> {
    let first = items.first()?;
    if items.len() == 1 {
        return Some(first.kind.clone());
    }
    if !items.iter().all(|i| i.kind.starts_with("file/") && !i.kind.ends_with("[]")) {
        return None;
    }
    let chain = std::iter::once(first.kind.clone()).chain(std::iter::successors(content::parent_type(&first.kind), |k| content::parent_type(k)));
    let common = chain.into_iter().find(|t| items.iter().all(|i| content::is_kind(&i.kind, t))).unwrap_or_else(|| "file/any".into());
    Some(format!("{common}[]"))
}

fn takes(action: &Action, items: &[Item], offered: Option<&str>) -> bool {
    match (items, offered) {
        ([], _) => action.accepts.is_empty(),
        ([one], _) => {
            let c = Content { kind: one.kind.clone(), hints: one.hints.clone(), ..Content::default() };
            action.accepts.iter().any(|a| content::content_matches(a, &c))
        }
        (_, Some(t)) => action.accepts.iter().any(|a| content::type_matches(a, t)),
        _ => false,
    }
}

/// What to offer for `items` on `surface` (empty: any surface), from every card with Link on. Ordered: pinned,
/// then usable before greyed-out and install entries, the user's defaults before "Other apps", higher rank, title.
pub fn offers(items: &[Item], surface: &str, cards: &[Manifest], rules: &Rules, prefs: &Prefs) -> Vec<Offer> {
    let offered = input_type(items);
    if items.len() > 1 && offered.is_none() {
        return Vec::new();
    }
    // Ranks are per kind: three images rank like one.
    let kind = offered.as_deref().unwrap_or_default().trim_end_matches("[]").to_string();
    let mut out: Vec<Offer> = Vec::new();
    for card in cards.iter().filter(|c| c.settings.link_enabled) {
        for action in card.actions.iter().filter(|a| a.on_this_platform()) {
            let r = reference(&card.id, &action.id);
            if rules.hide.contains(&r) || prefs.hidden.contains(&r) || out.iter().any(|o| o.reference() == r) {
                continue;
            }
            if !(surface.is_empty() || action.on_surface(surface)) || !takes(action, items, offered.as_deref()) {
                continue;
            }
            let mut offer = Offer {
                app: card.id.clone(),
                action: action.id.clone(),
                title: rules.rename.get(&r).cloned().unwrap_or_else(|| action.title.clone()),
                group: action.group.clone(),
                rank: i32::from(action.rank_for(&kind))
                    + rules.boost.get(&r).map_or(0, |b| b.iter().filter(|(p, _)| content::type_matches(p, &kind)).map(|(_, n)| *n).max().unwrap_or(0)),
                enabled: true,
                job: rules.jobs.get(&r).cloned().or_else(|| action.job.clone()).filter(|j| !j.is_empty()),
                pinned: prefs.pinned.contains(&r),
                one_click: action.is_one_click(),
                ..Offer::default()
            };
            if !action.available {
                // Never a broken entry (SPEC §1): an unavailable action shows only as "install what it needs".
                let Some(need) = action.requires.first() else { continue };
                offer.enabled = false;
                offer.install = Some(need.clone());
                offer.reason = action.reason.clone();
            } else if let Some(max) = action.max_items.filter(|m| items.len() > *m as usize) {
                offer.enabled = false;
                offer.reason = Some(format!("Takes at most {max} {}", if max == 1 { "item" } else { "items" }));
            } else if let Some(max) = action.max_bytes.filter(|m| items.iter().any(|i| i.size.is_some_and(|s| s > *m))) {
                offer.enabled = false;
                offer.reason = Some(format!("Takes files up to {}", format_limit(max)));
            }
            out.push(offer);
        }
    }
    // Two apps doing one job: both show until the user picks a default; then the default leads, the rest are "other".
    let jobs: Vec<String> = out.iter().filter_map(|o| o.job.clone()).collect();
    for job in jobs {
        let apps: Vec<&str> = out.iter().filter(|o| o.job.as_ref() == Some(&job)).map(|o| o.app.as_str()).collect();
        let distinct = apps.iter().enumerate().any(|(i, a)| apps[..i].iter().any(|b| b != a));
        if !distinct {
            continue;
        }
        let chosen = prefs.defaults.get(&job).filter(|d| out.iter().any(|o| o.job.as_ref() == Some(&job) && &o.reference() == *d)).cloned();
        for o in out.iter_mut().filter(|o| o.job.as_ref() == Some(&job)) {
            match &chosen {
                Some(d) => {
                    o.preferred = o.reference() == *d;
                    o.other = !o.preferred;
                }
                None => o.choose = true,
            }
        }
    }
    out.sort_by(|a, b| {
        (!a.pinned, !a.enabled, a.other, -a.rank, a.title.to_lowercase(), &a.app, &a.action).cmp(&(
            !b.pinned,
            !b.enabled,
            b.other,
            -b.rank,
            b.title.to_lowercase(),
            &b.app,
            &b.action,
        ))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_and_tolerate_a_broken_file() {
        let dir = std::env::temp_dir().join(format!("arcade-link-offers-{}", std::process::id()));
        let loc = Locations::under(&dir);
        assert_eq!(Prefs::load(&loc), Prefs::default());
        let mut p = Prefs::default();
        p.defaults.insert("image.read-text".into(), "arcade.lens/lens.recognize".into());
        p.pinned.push("arcade.clipboard/clipboard.add".into());
        p.save(&loc).unwrap();
        assert_eq!(Prefs::load(&loc), p);
        std::fs::write(loc.offer_prefs(), "{\"defaults\": 3}").unwrap();
        assert_eq!(Prefs::load(&loc), Prefs::default());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn catalog_rules_are_read() {
        let c = crate::catalog::Catalog::parse(r#"{"apps": [], "rules": {"hide": ["arcade.box/box.open"]}}"#).unwrap();
        assert_eq!(c.rules.hide, ["arcade.box/box.open"]);
        let c = crate::catalog::Catalog::parse(r#"{"apps": [], "rules": {"hide": 3}}"#).unwrap();
        assert_eq!(c.rules, Rules::default());
    }
}
