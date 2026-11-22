//! Which actions get their own menu entry (PLAN §4.5, "pinned actions"): the user's pins from Tools, or, until they
//! pin anything, the best one-click action for each common kind of file. Always led by the "Arcade…" picker.

use arcade_link::offers::{self, Item, Prefs, Rules};
use arcade_link::Manifest;

use crate::integrations::Entry;

/// The most menu entries besides the picker.
pub const MAX_PINS: usize = 8;

/// Kinds that get a default pin, most common first.
const DEFAULT_KINDS: [&str; 8] = ["file/image", "file/video", "file/audio", "file/pdf", "file/document", "file/spreadsheet", "file/archive", "file/text"];

fn entry_for(cards: &[Manifest], rules: &Rules, reference: &str) -> Option<Entry> {
    let (app, action) = reference.split_once('/')?;
    let card = cards.iter().find(|c| c.id == app && c.settings.link_enabled)?;
    let a = card.action(action).filter(|a| a.available && a.on_this_platform() && !a.accepts.is_empty())?;
    let title = rules.rename.get(reference).cloned().unwrap_or_else(|| a.title.clone());
    Some(Entry { reference: reference.to_string(), title, accepts: a.accepts.clone() })
}

/// The picker, then the pins.
pub fn entries(cards: &[Manifest], rules: &Rules, prefs: &Prefs) -> Vec<Entry> {
    let mut out = vec![Entry::picker()];
    if !prefs.pinned.is_empty() {
        out.extend(prefs.pinned.iter().filter_map(|r| entry_for(cards, rules, r)).take(MAX_PINS));
        return out;
    }
    for kind in DEFAULT_KINDS {
        if out.len() > MAX_PINS {
            break;
        }
        let best = offers::offers(&[Item::new(kind)], arcade_link::manifest::surface::FILE_MENU, cards, rules, prefs)
            .into_iter()
            .filter(|o| o.enabled && o.one_click && !o.other && o.rank > 0)
            .find(|o| !out.iter().any(|e| e.reference == o.reference()));
        if let Some(o) = best {
            if let Some(e) = entry_for(cards, rules, &o.reference()) {
                out.push(e);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cards() -> Vec<Manifest> {
        let card = json!({"schema": 1, "id": "arcade.box", "name": "Arcade Box", "executable": "/x", "actions": [
            {"id": "box:image.webp", "title": "Convert to WebP", "accepts": ["file/image"], "oneClick": true, "rank": {"file/image": 80}},
            {"id": "box:image.edit", "title": "Edit image", "accepts": ["file/image"], "rank": {"file/image": 90}, "interactive": true},
            {"id": "box:pdf.compress", "title": "Compress PDF", "accepts": ["file/pdf"], "oneClick": true, "rank": {"file/pdf": 70}},
            {"id": "box:audio.x", "title": "Unranked", "accepts": ["file/audio"], "oneClick": true},
            {"id": "box:video.x", "title": "Needs FFmpeg", "accepts": ["file/video"], "oneClick": true, "rank": {"file/video": 70}, "available": false, "requires": ["engine:ffmpeg"]}
        ]});
        vec![Manifest::from_json(&card.to_string()).unwrap()]
    }

    #[test]
    fn defaults_are_the_best_one_click_actions() {
        let e = entries(&cards(), &Rules::default(), &Prefs::default());
        let refs: Vec<&str> = e.iter().map(|e| e.reference.as_str()).collect();
        assert_eq!(refs, ["", "arcade.box/box:image.webp", "arcade.box/box:pdf.compress"]);
    }

    #[test]
    fn the_users_pins_win_and_missing_ones_drop_out() {
        let prefs = Prefs { pinned: vec!["arcade.box/box:pdf.compress".into(), "arcade.gone/x".into(), "arcade.box/box:video.x".into()], ..Prefs::default() };
        let mut rules = Rules::default();
        rules.rename.insert("arcade.box/box:pdf.compress".into(), "Make PDF smaller".into());
        let e = entries(&cards(), &rules, &prefs);
        assert_eq!(e.len(), 2);
        assert_eq!(e[1].title, "Make PDF smaller");
    }
}
