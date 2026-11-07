//! `release-manifest` → `promote-manifest` → `verify-manifest`, as the release
//! workflows run them.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_arcade-link");

fn dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("alc-release-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn run(args: &[&str], key: Option<&str>) -> Output {
    let mut c = Command::new(BIN);
    c.args(args).env_remove("ARCADE_UPDATE_SIGNING_KEY");
    if let Some(k) = key {
        c.env("ARCADE_UPDATE_SIGNING_KEY", k);
    }
    c.output().unwrap()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

struct Keys {
    public: String,
    secret: String,
}

fn keygen() -> Keys {
    let out = run(&["keygen", "--print-secret"], None);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("untrusted comment: minisign public key"));
    Keys { public: lines[1].into(), secret: format!("{}\n{}\n", lines[2], lines[3]) }
}

fn rc(d: &Path) -> PathBuf {
    std::fs::write(d.join("App-x86_64.AppImage"), (0..400_000u32).map(|i| (i * 7 % 251) as u8).collect::<Vec<_>>()).unwrap();
    std::fs::write(d.join("arcade-release.json"), "{}").unwrap();
    let out = run(
        &[
            "release-manifest",
            "--app",
            "arcade.t",
            "--version",
            "1.0.0",
            "--tag",
            "v1.0.0-rc.1",
            "--channel",
            "test",
            "--out",
            s(&d.join("rc")),
            s(&d.join("App-x86_64.AppImage")),
            s(&d.join("arcade-release.json")),
        ],
        None,
    );
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!d.join("rc/manifest.json.sig").exists(), "test-channel manifests are unsigned");
    d.join("rc/manifest.json")
}

#[test]
fn rc_promotes_to_a_signed_stable_manifest_over_the_same_bytes() {
    let d = dir("promote");
    let keys = keygen();
    let rc = rc(&d);
    let out = run(&["promote-manifest", s(&rc), "--tag", "v1.0.0", "--files", s(&d), "--out", s(&d.join("stable"))], Some(&keys.secret));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let ok = run(&["verify-manifest", s(&d.join("stable/manifest.json")), "--key", &keys.public, "--files", s(&d)], None);
    assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));

    let rc_m: serde_json::Value = serde_json::from_slice(&std::fs::read(&rc).unwrap()).unwrap();
    let st_m: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join("stable/manifest.json")).unwrap()).unwrap();
    assert_eq!(rc_m["files"], st_m["files"]);
    assert_eq!(st_m["channel"], "stable");
    assert_eq!(st_m["tag"], "v1.0.0");
}

#[test]
fn promotion_without_the_signing_key_publishes_nothing() {
    let d = dir("nokey");
    let rc = rc(&d);
    let out = run(&["promote-manifest", s(&rc), "--tag", "v1.0.0", "--files", s(&d), "--out", s(&d.join("stable"))], None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("refusing to publish an unsigned manifest"));
    assert!(!d.join("stable/manifest.json").exists());
}

#[test]
fn promotion_refuses_files_that_differ_from_the_rc() {
    let d = dir("differ");
    let keys = keygen();
    let rc = rc(&d);
    std::fs::write(d.join("arcade-release.json"), "{ }").unwrap();
    let out = run(&["promote-manifest", s(&rc), "--tag", "v1.0.0", "--files", s(&d), "--out", s(&d.join("stable"))], Some(&keys.secret));
    assert!(!out.status.success());
    assert!(!d.join("stable/manifest.json").exists());
}

#[test]
fn verification_fails_for_another_key_or_a_changed_file() {
    let d = dir("verify");
    let (keys, other) = (keygen(), keygen());
    let rc = rc(&d);
    assert!(run(&["promote-manifest", s(&rc), "--tag", "v1.0.0", "--files", s(&d), "--out", s(&d.join("st"))], Some(&keys.secret)).status.success());
    let m = d.join("st/manifest.json");
    assert!(!run(&["verify-manifest", s(&m), "--key", &other.public], None).status.success());
    assert!(run(&["verify-manifest", s(&m), "--key", &other.public, "--key", &keys.public], None).status.success());
    std::fs::write(d.join("App-x86_64.AppImage"), b"truncated").unwrap();
    assert!(!run(&["verify-manifest", s(&m), "--key", &keys.public, "--files", s(&d)], None).status.success());
}

#[test]
fn keygen_never_runs_without_a_destination_for_the_secret() {
    let out = run(&["keygen"], None);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
}

#[test]
fn unknown_commands_print_usage_and_exit_2() {
    let out = run(&["definitely-not-a-command"], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("USAGE"));
    let help = run(&["--help"], None);
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("USAGE"));
}
