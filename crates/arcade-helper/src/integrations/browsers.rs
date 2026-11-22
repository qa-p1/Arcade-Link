//! Native messaging host files, so the Arcade extension can reach the helper (PLAN §4.4). Written only for browsers
//! whose settings folder exists, so a computer without Brave never gets a Brave folder.

use serde_json::json;
use std::path::{Path, PathBuf};

use super::{File, Home, Plan, RegKey};
use crate::ids;

fn manifest(exe: &Path, chromium: bool) -> String {
    let mut m = json!({
        "name": ids::HOST_NAME,
        "description": "Arcade: run your Arcade apps' actions from the browser",
        "path": exe.to_string_lossy(),
        "type": "stdio",
    });
    if chromium {
        m["allowed_origins"] = json!([ids::chrome_origin()]);
    } else {
        m["allowed_extensions"] = json!([ids::FIREFOX_ADDON_ID]);
    }
    serde_json::to_string_pretty(&m).unwrap_or_default() + "\n"
}

/// (browser settings folder that must exist, host folder inside it, Chromium family?)
fn unix_hosts(os: &str, home: &Home) -> Vec<(PathBuf, PathBuf, bool)> {
    let c = |root: PathBuf| (root.clone(), root.join("NativeMessagingHosts"), true);
    if os == "macos" {
        let a = &home.app_data;
        vec![
            c(a.join("Google/Chrome")),
            c(a.join("Google/Chrome Beta")),
            c(a.join("Chromium")),
            c(a.join("BraveSoftware/Brave-Browser")),
            c(a.join("Microsoft Edge")),
            c(a.join("Vivaldi")),
            (a.join("Firefox"), a.join("Mozilla/NativeMessagingHosts"), false),
            (a.join("zen"), a.join("Mozilla/NativeMessagingHosts"), false),
            (a.join("LibreWolf"), a.join("LibreWolf/NativeMessagingHosts"), false),
        ]
    } else {
        let cfg = &home.config;
        vec![
            c(cfg.join("google-chrome")),
            c(cfg.join("google-chrome-beta")),
            c(cfg.join("google-chrome-unstable")),
            c(cfg.join("chromium")),
            c(cfg.join("BraveSoftware/Brave-Browser")),
            c(cfg.join("microsoft-edge")),
            c(cfg.join("vivaldi")),
            (home.home.join(".mozilla"), home.home.join(".mozilla/native-messaging-hosts"), false),
            // Zen's own folder is confirmed on the owner's computer (LOCAL-PROMPTS B); both candidates until then.
            (home.home.join(".zen"), home.home.join(".zen/native-messaging-hosts"), false),
            (home.home.join(".librewolf"), home.home.join(".librewolf/native-messaging-hosts"), false),
        ]
    }
}

/// The plan for `os` ("linux", "macos", "windows").
pub fn plan_for(os: &str, exe: &Path, home: &Home) -> Plan {
    let mut plan = Plan::default();
    let file = format!("{}.json", ids::HOST_NAME);
    if os == "windows" {
        // Windows finds the host through the registry; the manifests live next to the helper's data.
        let dir = home.app_data.join("Arcade").join("helper");
        for (name, chromium) in [("chromium", true), ("firefox", false)] {
            plan.files.push(File { path: dir.join(format!("{name}.json")), contents: manifest(exe, chromium), executable: false });
        }
        for (vendor, chromium) in [
            (r"Software\Google\Chrome", true),
            (r"Software\Chromium", true),
            (r"Software\BraveSoftware\Brave-Browser", true),
            (r"Software\Microsoft\Edge", true),
            (r"Software\Vivaldi", true),
            (r"Software\Mozilla", false),
        ] {
            let key = format!(r"{vendor}\NativeMessagingHosts\{}", ids::HOST_NAME);
            let path = dir.join(if chromium { "chromium.json" } else { "firefox.json" });
            plan.registry_roots.push(key.clone());
            plan.registry.push(RegKey { path: key, values: vec![(String::new(), path.to_string_lossy().into_owned())] });
        }
        return plan;
    }
    let mut seen: Vec<PathBuf> = Vec::new();
    for (settings, hosts, chromium) in unix_hosts(os, home) {
        if settings.is_dir() && !seen.contains(&hosts) {
            plan.files.push(File { path: hosts.join(&file), contents: manifest(exe, chromium), executable: false });
            seen.push(hosts);
        }
    }
    plan
}

pub fn plan(exe: &Path, home: &Home) -> Plan {
    plan_for(arcade_link::paths::current_platform(), exe, home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_files_only_for_installed_browsers() {
        let root = std::env::temp_dir().join(format!("arcade-helper-browsers-{}", std::process::id()));
        let home = Home::under(&root);
        std::fs::create_dir_all(home.config.join("google-chrome")).unwrap();
        std::fs::create_dir_all(root.join(".mozilla")).unwrap();
        let plan = plan_for("linux", Path::new("/opt/arcade/arcade-helper"), &home);
        let paths: Vec<_> = plan.files.iter().map(|f| f.path.strip_prefix(&root).unwrap().to_path_buf()).collect();
        assert_eq!(
            paths,
            [
                PathBuf::from(".config/google-chrome/NativeMessagingHosts/dev.arcade.helper.json"),
                PathBuf::from(".mozilla/native-messaging-hosts/dev.arcade.helper.json")
            ]
        );
        let chrome: serde_json::Value = serde_json::from_str(&plan.files[0].contents).unwrap();
        assert_eq!(chrome["allowed_origins"][0], ids::chrome_origin());
        assert_eq!(chrome["path"], "/opt/arcade/arcade-helper");
        let firefox: serde_json::Value = serde_json::from_str(&plan.files[1].contents).unwrap();
        assert_eq!(firefox["allowed_extensions"][0], ids::FIREFOX_ADDON_ID);
        assert!(firefox.get("allowed_origins").is_none());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn windows_uses_the_registry() {
        let home = Home::under(Path::new("C:/Users/u"));
        let plan = plan_for("windows", Path::new(r"C:\Arcade\arcade-helper.exe"), &home);
        assert_eq!(plan.files.len(), 2);
        assert!(plan.registry.iter().any(|k| k.path == r"Software\Mozilla\NativeMessagingHosts\dev.arcade.helper" && k.values[0].1.ends_with("firefox.json")));
        assert_eq!(plan.registry_roots.len(), 6);
    }
}
