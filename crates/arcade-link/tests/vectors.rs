//! Conformance vectors (`spec/vectors/*.json`), shared with the Qt module.

use arcade_link::content::{content_matches, file_kind_for_path};
use arcade_link::error::{format_limit, standard_message, ErrorCode};
use arcade_link::registry::normalize_accelerator;
use arcade_link::wire::{negotiate, Kind, Message};
use arcade_link::{Content, Manifest};
use serde_json::Value;
use std::path::Path;

fn load(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors").join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

#[test]
fn wire_messages() {
    let v = load("wire.json");
    for case in v["messages"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let parsed = Message::parse(case["line"].as_str().unwrap());
        assert_eq!(parsed.is_ok(), case["valid"].as_bool().unwrap(), "{name}: {parsed:?}");
        if let Ok(m) = parsed {
            let kind = match m.kind() {
                Kind::Request => "request",
                Kind::Notification => "notification",
                Kind::Response => "response",
            };
            assert_eq!(kind, case["kind"].as_str().unwrap(), "{name}");
        }
    }
    for case in v["negotiate"].as_array().unwrap() {
        let c: Vec<u32> = serde_json::from_value(case["client"].clone()).unwrap();
        let s: Vec<u32> = serde_json::from_value(case["server"].clone()).unwrap();
        assert_eq!(negotiate(&c, &s), case["result"].as_u64().map(|x| x as u32), "{case}");
    }
    assert_eq!(v["maxLineBytes"].as_u64().unwrap() as usize, arcade_link::wire::MAX_LINE);
}

#[test]
fn content_matching_and_kinds() {
    let v = load("content.json");
    for case in v["matches"].as_array().unwrap() {
        let mut c = Content { kind: case["offered"].as_str().unwrap().into(), ..Default::default() };
        if let Some(h) = case["hints"].as_array() {
            c.hints = h.iter().map(|x| x.as_str().unwrap().to_string()).collect();
        } else if case["accept"].as_str().unwrap().contains(";hint=") {
            c.hints = vec![];
        }
        assert_eq!(content_matches(case["accept"].as_str().unwrap(), &c), case["expected"].as_bool().unwrap(), "{case}");
    }
    for case in v["kinds"].as_array().unwrap() {
        assert_eq!(file_kind_for_path(Path::new(case["path"].as_str().unwrap())), case["kind"].as_str().unwrap(), "{case}");
    }
}

#[test]
fn error_messages() {
    let v = load("errors.json");
    for case in v["messages"].as_array().unwrap() {
        let code: ErrorCode = serde_json::from_value(case["code"].clone()).unwrap();
        let msg = standard_message(code, case["app"].as_str().unwrap(), case["reason"].as_str(), case["limit"].as_u64());
        assert_eq!(msg, case["message"].as_str().unwrap(), "{case}");
    }
    for case in v["limits"].as_array().unwrap() {
        assert_eq!(format_limit(case["bytes"].as_u64().unwrap()), case["text"].as_str().unwrap());
    }
}

#[test]
fn manifests() {
    let v = load("manifest.json");
    for case in v["valid"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let m = Manifest::from_json(&case["manifest"].to_string()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let e = &case["expect"];
        assert_eq!(m.id, e["id"].as_str().unwrap(), "{name}");
        assert_eq!(m.actions.len() as u64, e["actions"].as_u64().unwrap(), "{name}");
        assert_eq!(m.settings.link_enabled, e["linkEnabled"].as_bool().unwrap(), "{name}");
        assert_eq!(m.launch.invoke.is_some(), e["invoke"].as_bool().unwrap(), "{name}");
        if let Some(a) = e["firstActionAvailable"].as_bool() {
            assert_eq!(m.actions[0].available, a, "{name}");
            assert_eq!(u64::from(m.actions[0].version), e["firstActionVersion"].as_u64().unwrap(), "{name}");
        }
    }
    for case in v["invalid"].as_array().unwrap() {
        assert!(Manifest::from_json(&case["manifest"].to_string()).is_err(), "{}", case["name"]);
    }
}

#[test]
fn accelerators() {
    let v = load("accelerators.json");
    for pair in v["same"].as_array().unwrap() {
        assert_eq!(normalize_accelerator(pair[0].as_str().unwrap()), normalize_accelerator(pair[1].as_str().unwrap()), "{pair}");
    }
    for pair in v["different"].as_array().unwrap() {
        assert_ne!(normalize_accelerator(pair[0].as_str().unwrap()), normalize_accelerator(pair[1].as_str().unwrap()), "{pair}");
    }
}
