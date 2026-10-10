use arcade_link::{
    accelerator::{self, Platform},
    manifest::{Docs, ManifestDocument, TrayHostSettings},
    receipt::*,
    shortcuts::{Document, Sheet},
    Locations, Manifest,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

fn vectors(name: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors").join(name)).unwrap()).unwrap()
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("al-v3-{}", arcade_link::endpoint::new_token().unwrap()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn accelerator_vectors() {
    let v = vectors("accelerators.json");
    for pair in v["same"].as_array().unwrap() {
        assert_eq!(accelerator::normalize(pair[0].as_str().unwrap()).unwrap(), accelerator::normalize(pair[1].as_str().unwrap()).unwrap());
    }
    for pair in v["different"].as_array().unwrap() {
        assert_ne!(accelerator::normalize(pair[0].as_str().unwrap()).unwrap(), accelerator::normalize(pair[1].as_str().unwrap()).unwrap());
    }
    for c in v["normalize"].as_array().unwrap() {
        let normalized = accelerator::normalize(c["input"].as_str().unwrap()).unwrap();
        assert_eq!(normalized, c["expected"].as_str().unwrap(), "{c}");
        assert!(accelerator::is_canonical(&normalized));
    }
    for c in v["display"].as_array().unwrap() {
        let os = serde_json::from_value(c["os"].clone()).unwrap();
        assert_eq!(accelerator::display(c["input"].as_str().unwrap(), os).unwrap(), c["expected"].as_str().unwrap(), "{c}");
    }
    for c in v["conflicts"].as_array().unwrap() {
        assert_eq!(accelerator::conflicts(c["a"].as_str().unwrap(), c["b"].as_str().unwrap()).unwrap(), c["expected"].as_bool().unwrap(), "{c}");
    }
    for c in v["invalid"].as_array().unwrap() {
        assert!(accelerator::normalize(c.as_str().unwrap()).is_err(), "{c}");
    }
    assert!(accelerator::conflicts("Ctrl++", "Ctrl+Equal").is_err());
}
#[test]
fn shortcut_vectors_and_markdown() {
    let v = vectors("shortcuts.json");
    for case in v["valid"].as_array().unwrap() {
        let markdown = if case.get("sheet").is_some() {
            let s = Sheet::from_json(&case["sheet"].to_string()).unwrap();
            assert!(s.matches(Platform::Linux, "FIREFOX"));
            s.markdown().unwrap()
        } else {
            let d = Document::from_json(&case["document"].to_string()).unwrap();
            if let Some(m) = case.get("manifest") {
                let mut manifest = Manifest::new(m["id"].as_str().unwrap(), "1", "/test");
                manifest.shortcuts = serde_json::from_value(m["shortcuts"].clone()).unwrap();
                d.validate(Some(&manifest)).unwrap();
            }
            d.markdown().unwrap()
        };
        assert_eq!(markdown, case["markdown"].as_str().unwrap(), "{}", case["name"]);
    }
    for case in v["invalid"].as_array().unwrap() {
        let invalid = if case.get("sheet").is_some() {
            Sheet::from_json(&case["sheet"].to_string()).is_err()
        } else if let Some(m) = case.get("manifest") {
            let mut manifest = Manifest::new(m["id"].as_str().unwrap(), "1", "/test");
            manifest.shortcuts = serde_json::from_value(m["shortcuts"].clone()).unwrap();
            Document::from_json(&case["document"].to_string()).and_then(|d| d.validate(Some(&manifest))).is_err()
        } else {
            Document::from_json(&case["document"].to_string()).is_err()
        };
        assert!(invalid, "{}", case["name"]);
    }
}
#[test]
fn receipt_vectors() {
    let v = vectors("receipts.json");
    for case in v["valid"].as_array().unwrap() {
        let r = Receipt::from_json(&case["receipt"].to_string()).unwrap();
        assert_eq!(Receipt::from_json(&serde_json::to_string(&r).unwrap()).unwrap(), r);
    }
    for case in v["invalid"].as_array().unwrap() {
        assert!(Receipt::from_json(&case["receipt"].to_string()).is_err(), "{}", case["name"]);
    }
}
#[test]
fn atomic_receipts_are_private_and_leave_no_staging_files() {
    let root = Root::new();
    let store = Arc::new(Store::new(&Locations::under(&root.0)));
    let mut r = Receipt::from_json(&vectors("receipts.json")["valid"][0]["receipt"].to_string()).unwrap();
    r.path = root.0.join("Arcade-Find.AppImage");
    store.write(&r).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(store.path(&r.id).unwrap(), fs::Permissions::from_mode(0o644)).unwrap();
    }
    let mut handles = Vec::new();
    for i in 0..8 {
        let store = store.clone();
        let mut r = r.clone();
        r.version = format!("0.3.{i}");
        handles.push(std::thread::spawn(move || {
            for _ in 0..10 {
                store.write(&r).unwrap();
                assert!(store.read(&r.id).unwrap().is_some());
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(store.path(&r.id).unwrap()).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(fs::read_dir(store.dir()).unwrap().count(), 1);
    r.method = InstallMethod::Dev;
    assert!(store.write(&r).is_err());
    assert!(store.path("../../escape").is_err());
    store.remove("arcade.find").unwrap();
    assert!(store.read("arcade.find").unwrap().is_none());
}
#[test]
fn arcade_home_child() {
    if let Some(root) = std::env::var_os("ARCADE_RECEIPT_TEST_HOME") {
        assert_eq!(Store::discover().dir(), PathBuf::from(root).join("installs"));
    }
}
#[test]
fn arcade_home_receipt_override_is_process_scoped() {
    let root = Root::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "arcade_home_child", "--nocapture"])
        .env("ARCADE_HOME", &root.0)
        .env("ARCADE_RECEIPT_TEST_HOME", &root.0)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
}
#[test]
fn manifest_documents_keep_existing_struct_literals_and_new_fields() {
    let root = Root::new();
    let loc = Locations::under(&root.0);
    let mut m = Manifest::new("arcade.tools", "0.3.0", std::env::current_exe().unwrap().to_str().unwrap());
    m.settings = arcade_link::manifest::ManifestSettings { link_enabled: true };
    let mut doc: ManifestDocument = m.clone().into();
    doc.additions.docs = Some(Docs { version: "0.3.0".into() });
    doc.additions.tray_host = Some(TrayHostSettings { enabled: true, excluded: vec!["arcade.find".into()] });
    doc.additions.install = Some(InstallMethod::Manual);
    assert!(arcade_link::manifest::write_document(&loc, &doc).unwrap());
    assert!(!arcade_link::manifest::write_document(&loc, &doc).unwrap());
    assert_eq!(Manifest::from_json(&doc.to_json()).unwrap(), m);
    let reg = arcade_link::Registry::load(&loc);
    assert_eq!(reg.additions("arcade.tools"), Some(&doc.additions));
    assert_eq!(serde_json::from_str::<ManifestDocument>(&doc.to_json()).unwrap(), doc);
    assert_eq!(serde_json::from_value::<Value>(json!(doc)).unwrap()["settings"]["trayHost"]["enabled"], true);
}
