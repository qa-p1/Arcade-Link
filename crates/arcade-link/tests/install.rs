#![cfg(feature = "install")]
use arcade_link::install::*;
#[cfg(target_os = "linux")]
use arcade_link::receipt::{InstallMethod, ManagedBy};
use std::{
    fs,
    path::{Path, PathBuf},
};
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("al-install-{}", arcade_link::endpoint::new_token().unwrap()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn runtime(executable: PathBuf) -> Runtime {
    Runtime { executable, appimage: None, appdir: None, dev_build: false }
}

#[cfg(target_os = "linux")]
#[test]
fn foreign_receipt_paths_cannot_drive_native_mutations() {
    const CHILD: &str = "ARCADE_FOREIGN_RECEIPT_TEST";
    if let Some(root) = std::env::var_os(CHILD) {
        let e = Environment::under(Path::new(&root));
        let source = e.home.join("download.AppImage");
        fs::write(&source, b"fixture").unwrap();
        let app = find_info();
        let r = runtime(source);
        let mut receipt = install(&e, find_options(&app, &r, "1")).unwrap();
        let victim = PathBuf::from(r"C:\Victim.exe");
        fs::write(&victim, b"unrelated file in the working directory").unwrap();
        receipt.path = victim.clone();
        receipt.validate().unwrap(); // Still valid as portable Windows data.
        assert!(receipt.validate_native_paths().is_err());
        e.receipts().write(&receipt).unwrap();
        assert!(uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).is_err());
        assert!(repair(&e, &app, None).is_err());
        assert!(install(&e, find_options(&app, &r, "2")).is_err());
        assert_eq!(fs::read(&victim).unwrap(), b"unrelated file in the working directory");
        assert!(e.receipts().read(&app.id).unwrap().is_some());
        assert!(!integration_status(&e, &app.id, "bash").unwrap().entries[0].exists);
        return;
    }
    let root = Root::new();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "foreign_receipt_paths_cannot_drive_native_mutations", "--nocapture"])
        .current_dir(&root.0)
        .env(CHILD, &root.0)
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(target_os = "linux")]
fn find_info() -> AppInfo {
    AppInfo {
        id: "arcade.find".into(),
        name: "Arcade Find".into(),
        filename: "Arcade-Find.AppImage".into(),
        desktop_id: "arcade-find".into(),
        cli_name: "arcade-find".into(),
        autostart_id: "arcade-find".into(),
        background_args: vec!["--background".into()],
        legacy_desktop_ids: vec!["old-find".into()],
    }
}

#[cfg(target_os = "linux")]
fn find_options<'a>(app: &'a AppInfo, runtime: &'a Runtime, version: &'a str) -> InstallOptions<'a> {
    InstallOptions {
        app,
        runtime,
        version,
        channel: "stable",
        managed_by: ManagedBy::SelfManaged,
        icons: &[],
        start_at_login: false,
        remove_download: false,
        refresh_caches: false,
    }
}

#[cfg(target_os = "linux")]
#[test]
fn stale_receipt_cannot_rewrite_or_delete_foreign_integration() {
    for marker in ["X-Arcade-Id=arcade.look\n", ""] {
        let root = Root::new();
        let e = Environment::under(&root.0);
        let app = find_info();
        let source = root.0.join("download.AppImage");
        fs::write(&source, b"v1").unwrap();
        let r = runtime(source);
        let mut opts = find_options(&app, &r, "1");
        opts.start_at_login = true;
        let first = install(&e, opts).unwrap();
        let foreign = format!("[Desktop Entry]\nType=Application\n{marker}Exec=/foreign/program\n");
        for path in first.integration.desktop_entry.iter().chain(first.integration.autostart.iter()) {
            fs::write(path, &foreign).unwrap();
        }
        let cli = first.integration.cli.as_ref().unwrap();
        fs::remove_file(cli).unwrap();
        std::os::unix::fs::symlink("/foreign/program", cli).unwrap();
        let original = fs::read(e.receipts().path(&app.id).unwrap()).unwrap();
        let error = uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap_err();
        assert!(error.to_string().contains("ownership conflict"));
        assert_eq!(fs::read(e.receipts().path(&app.id).unwrap()).unwrap(), original);
        assert!(first.path.exists());
        assert_eq!(integration_status(&e, &app.id, "bash").unwrap().skipped.len(), 3);
        let repaired = repair(&e, &app, None).unwrap();
        assert!(repaired.integration.desktop_entry.is_none());
        assert!(repaired.integration.autostart.is_none());
        assert!(repaired.integration.cli.is_none());
        assert_eq!(repaired.integration.skipped.len(), 3);
        uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap();
        for path in first.integration.desktop_entry.iter().chain(first.integration.autostart.iter()) {
            assert_eq!(fs::read_to_string(path).unwrap(), foreign);
        }
        assert_eq!(fs::read_link(cli).unwrap(), Path::new("/foreign/program"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn repair_refuses_traversal_before_changing_entries_or_receipt() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let source = root.0.join("download.AppImage");
    fs::write(&source, b"v1").unwrap();
    let r = runtime(source);
    let first = install(&e, find_options(&app, &r, "1")).unwrap();
    let moved = e.applications.join("Moved-Find.AppImage");
    fs::write(&moved, b"moved copy").unwrap();
    let traversal = e.applications.join("../Arcade/Moved-Find.AppImage");
    assert!(traversal.is_file());
    let launcher = first.integration.desktop_entry.as_ref().unwrap();
    let before = fs::read(launcher).unwrap();
    let receipt_before = fs::read(e.receipts().path(&app.id).unwrap()).unwrap();
    assert!(repair(&e, &app, Some(&traversal)).unwrap_err().to_string().contains("parent traversal"));
    assert_eq!(fs::read(launcher).unwrap(), before);
    assert_eq!(fs::read(e.receipts().path(&app.id).unwrap()).unwrap(), receipt_before);
    assert_eq!(fs::read_link(first.integration.cli.as_ref().unwrap()).unwrap(), first.path);
    let repaired = repair(&e, &app, Some(&moved)).unwrap();
    repaired.validate_native_paths().unwrap();
    uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap();
    assert!(!moved.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn duplicate_cleanup_decodes_desktop_escaping_and_exec_quoting() {
    let root = Root::new();
    let mut e = Environment::under(&root.0);
    e.applications = root.0.join("Applications with spaces/Arcade");
    let app = find_info();
    let source = root.0.join("download.AppImage");
    fs::write(&source, b"fixture").unwrap();
    let r = runtime(source);
    let dest = e.applications.join(&app.filename);
    let dir = e.data.join("applications");
    fs::create_dir_all(&dir).unwrap();
    let launcher = dir.join("same-executable.desktop");
    let text = format!("[Desktop Entry]\nType=Application\nX-Arcade-Id={}\nExec=\"{}\" %U\n", app.id, dest.display().to_string().replace(' ', r"\s"));
    fs::write(&launcher, &text).unwrap();
    let invalid = dir.join("invalid.desktop");
    fs::write(&invalid, "[Desktop Entry]\nX-Arcade-Id=arcade.find\nExec=\"unterminated\n").unwrap();
    install(&e, find_options(&app, &r, "1")).unwrap();
    assert_eq!(fs::read_to_string(&launcher).unwrap(), text);
    assert!(invalid.exists(), "malformed launchers are not proof of a stale executable");
}

#[cfg(target_os = "linux")]
#[test]
fn update_preserves_an_unowned_previous_file() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let source = root.0.join("download.AppImage");
    let r = runtime(source.clone());
    fs::write(&source, b"v1").unwrap();
    let first = install(&e, find_options(&app, &r, "1")).unwrap();
    assert!(first.previous.is_none());
    let foreign = e.applications.join(".previous").join(&app.filename);
    fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    fs::write(&foreign, b"unrelated backup").unwrap();
    fs::write(&source, b"v2").unwrap();
    let second = install(&e, find_options(&app, &r, "2")).unwrap();
    let owned = &second.previous.as_ref().unwrap().path;
    assert_ne!(owned, &foreign);
    assert_eq!(fs::read(owned).unwrap(), b"v1");
    fs::write(&source, b"v3").unwrap();
    let third = install(&e, find_options(&app, &r, "3")).unwrap();
    assert_eq!(&third.previous.as_ref().unwrap().path, owned);
    assert_eq!(fs::read(owned).unwrap(), b"v2");
    uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap();
    assert_eq!(fs::read(&foreign).unwrap(), b"unrelated backup");
    assert!(!owned.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn rollback_atomically_restores_a_launcher_symlink_over_the_generated_file() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let source = root.0.join("download.AppImage");
    let r = runtime(source.clone());
    fs::write(&source, b"v1").unwrap();
    let mut first = install(&e, find_options(&app, &r, "1")).unwrap();
    let launcher = first.integration.desktop_entry.as_ref().unwrap();
    let target = root.0.join("launcher-original.desktop");
    fs::rename(launcher, &target).unwrap();
    std::os::unix::fs::symlink(&target, launcher).unwrap();
    let original = fs::read(&target).unwrap();
    let autostart = e.config.join("autostart/arcade-find.desktop");
    fs::create_dir_all(autostart.parent().unwrap()).unwrap();
    fs::write(&autostart, b"[Desktop Entry]\nX-Arcade-Id=arcade.find\nHidden=true\n").unwrap();
    first.integration.autostart = Some(autostart);
    e.receipts().write(&first).unwrap();
    fs::write(&source, b"v2").unwrap();
    assert!(install(&e, find_options(&app, &r, "2")).is_err());
    assert!(fs::symlink_metadata(launcher).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_link(launcher).unwrap(), target);
    assert_eq!(fs::read(launcher).unwrap(), original);
    assert_eq!(fs::read(&first.path).unwrap(), b"v1");
    assert_eq!(e.receipts().read(&app.id).unwrap().unwrap(), first);
}

#[test]
fn relative_install_environment_is_refused_before_writing() {
    let e = Environment::under(Path::new("relative-install-root"));
    assert!(e.validate().is_err());
    assert!(uninstall(&e, "arcade.find", UninstallOptions { remove_data: false, data_folders: &[] }).is_err());
    assert!(!e.home.exists());
}

#[test]
fn windows_isolated_path_identity_matches_installer_vectors() {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors/windows-path-identity.json")).unwrap();
    let vectors: serde_json::Value = serde_json::from_str(&text).unwrap();
    for case in vectors["cases"].as_array().unwrap() {
        assert_eq!(isolated_environment_key(Path::new(case["root"].as_str().unwrap())), case["key"].as_str().unwrap());
    }
}

#[cfg(target_os = "linux")]
#[test]
fn foreign_integration_is_skipped_reported_and_survives_uninstall() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let source = root.0.join("download.AppImage");
    fs::write(&source, b"app").unwrap();
    let r = runtime(source);
    let desktop = e.data.join("applications/arcade-find.desktop");
    let icon = e.data.join("icons/hicolor/256x256/apps/arcade-find.png");
    let autostart = e.config.join("autostart/arcade-find.desktop");
    let cli = e.bin.join("arcade-find");
    let victim = root.0.join("foreign-program");
    fs::write(&victim, b"foreign executable").unwrap();
    for p in [&desktop, &icon, &autostart, &cli] {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
    }
    fs::write(&desktop, "[Desktop Entry]\nType=Application\nExec=/unrelated/program\n").unwrap();
    fs::write(&autostart, "[Desktop Entry]\nType=Application\nExec=/unrelated/program --background\n").unwrap();
    fs::write(&icon, b"foreign icon").unwrap();
    std::os::unix::fs::symlink(&victim, &cli).unwrap();
    let png = root.0.join("find.png");
    fs::write(&png, b"app icon").unwrap();
    let icons = [Icon { source: png, size: IconSize::Pixels(256) }];
    let mut opts = find_options(&app, &r, "1");
    opts.icons = &icons;
    opts.start_at_login = true;
    let receipt = install(&e, opts).unwrap();
    assert!(receipt.integration.desktop_entry.is_none());
    assert!(receipt.integration.cli.is_none());
    assert!(receipt.integration.autostart.is_none());
    assert!(receipt.integration.icons.is_empty());
    assert_eq!(receipt.integration.skipped.len(), 4);
    let status = integration_status(&e, &app.id, "fish").unwrap();
    assert_eq!(status.skipped, receipt.integration.skipped);
    repair(&e, &app, None).unwrap();
    uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap();
    assert_eq!(fs::read(&icon).unwrap(), b"foreign icon");
    assert_eq!(fs::read_link(&cli).unwrap(), victim);
    assert!(fs::read_to_string(&desktop).unwrap().contains("/unrelated/program"));
    assert!(fs::read_to_string(&autostart).unwrap().contains("/unrelated/program"));
}

#[cfg(target_os = "linux")]
#[test]
fn integration_symlinked_parent_cannot_escape_its_root() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let source = root.0.join("download.AppImage");
    fs::write(&source, b"app").unwrap();
    let r = runtime(source);
    let foreign = e.data.join("unrelated");
    fs::create_dir_all(&foreign).unwrap();
    std::os::unix::fs::symlink(&foreign, e.data.join("applications")).unwrap();
    // The explicitly configured applications root is resolved consistently,
    // but a child symlink escaping an icons root must be refused.
    let icons_root = e.data.join("icons/hicolor");
    fs::create_dir_all(&icons_root).unwrap();
    std::os::unix::fs::symlink(&foreign, icons_root.join("256x256")).unwrap();
    let png = root.0.join("find.png");
    fs::write(&png, b"icon").unwrap();
    let icons = [Icon { source: png, size: IconSize::Pixels(256) }];
    let mut opts = find_options(&app, &r, "1");
    opts.icons = &icons;
    assert!(install(&e, opts).is_err());
    assert!(!foreign.join("apps/arcade-find.png").exists());
    assert!(e.receipts().read(&app.id).unwrap().is_none());
}

#[test]
fn dev_detection_appimage_and_prompt_suppression() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let source = root.0.join("Download.AppImage");
    fs::write(&source, b"fake").unwrap();
    let mut r = runtime(root.0.join("mount/usr/bin/find"));
    r.appimage = Some(source.clone());
    r.appdir = Some(root.0.join("mount"));
    let detected = detect(&e, "arcade.find", &r).unwrap();
    assert!(detected.is_app_image);
    assert_eq!(detected.running_from, source);
    assert_eq!(detected.state, State::NotInstalled);
    assert!(should_prompt(&detected, &[], false, false));
    for flag in ["--background", "--arcade-invoke", "--arcade-manifest", "--version", "--install", "--integration-status"] {
        assert!(!should_prompt(&detected, &[flag.into()], false, false));
    }
    assert!(!should_prompt(&detected, &[], true, false));
    assert!(!should_prompt(&detected, &[], false, true));
    r.executable = PathBuf::from("/usr/bin/foreign-app");
    assert!(!r.is_appimage());
    for (tree, marker) in [("source/target/debug", "Cargo.toml"), ("cmake/build/bin", "CMakeLists.txt")] {
        let exe = root.0.join(tree).join("app");
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        let source = exe.ancestors().nth(3).unwrap();
        fs::write(source.join(marker), b"source").unwrap();
        assert!(is_dev_build(&exe));
    }
    fs::create_dir_all(root.0.join("worktree/bin")).unwrap();
    fs::write(root.0.join("worktree/.git"), b"gitdir: /source/git").unwrap();
    assert!(is_dev_build(&root.0.join("worktree/bin/find")));
    r.dev_build = true;
    assert_eq!(detect(&e, "arcade.find", &r).unwrap().state, State::DevBuild);
    assert!(e.receipts().read("arcade.find").unwrap().is_none());
    assert!(platform::macos::is_translocated(Path::new("/private/var/AppTranslocation/token/App.app")));
    assert!(platform::macos::in_applications(&e, &e.home.join("Applications/Find.app")));
    assert!(strings::format(&strings::get().older.title, "Find", "0.3.0", "0.2.0", "0.3.0").contains("Update Find 0.2.0 → 0.3.0?"));
}

#[test]
fn macos_bundle_prompt_uses_the_same_suppression_rules() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let bundle = root.0.join("download/Find.app");
    let exe = bundle.join("Contents/MacOS/find");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(&exe, b"bundle fixture").unwrap();
    let detected = detect(&e, "arcade.find", &runtime(exe)).unwrap();
    assert!(!detected.is_app_image);
    assert_eq!(should_prompt(&detected, &[], false, false), cfg!(target_os = "macos"));
    for flag in ["--background", "--arcade-invoke", "--arcade-manifest", "--version"] {
        assert!(!should_prompt(&detected, &[flag.into()], false, false));
    }
    assert!(!should_prompt(&detected, &[], true, false));
    assert!(!should_prompt(&detected, &[], false, true));
}

#[cfg(target_os = "linux")]
#[test]
fn install_update_repair_uninstall_are_isolated_and_receipt_driven() {
    let root = Root::new();
    let mut e = Environment::under(&root.0);
    let app = AppInfo {
        id: "arcade.find".into(),
        name: "Arcade Find".into(),
        filename: "Arcade-Find.AppImage".into(),
        desktop_id: "arcade-find".into(),
        cli_name: "arcade-find".into(),
        autostart_id: "arcade-find".into(),
        background_args: vec!["--background".into()],
        legacy_desktop_ids: vec!["old-find".into()],
    };
    // "Remove the downloaded file" only applies inside Downloads.
    fs::create_dir_all(root.0.join("Downloads")).unwrap();
    let source = root.0.join("Downloads/download % dollar$.AppImage");
    fs::write(&source, b"version 1").unwrap();
    let r = runtime(source.clone());
    let png = root.0.join("find.png");
    let svg = root.0.join("find.svg");
    fs::write(&png, b"fake png").unwrap();
    fs::write(&svg, b"<svg/>").unwrap();
    let icons = [Icon { source: png, size: IconSize::Pixels(256) }, Icon { source: svg, size: IconSize::Scalable }];
    let options = |version, remove_download| InstallOptions {
        app: &app,
        runtime: &r,
        version,
        channel: "stable",
        managed_by: ManagedBy::SelfManaged,
        icons: &icons,
        start_at_login: false,
        remove_download,
        refresh_caches: false,
    };
    let launchers = e.data.join("applications");
    fs::create_dir_all(&launchers).unwrap();
    for name in ["stale", "old-find", "unrelated"] {
        fs::write(
            launchers.join(format!("{name}.desktop")),
            format!("[Desktop Entry]\nType=Application\nExec=/old/arcade-find\n{}", if name == "stale" { "X-Arcade-Id=arcade.find\n" } else { "" }),
        )
        .unwrap();
    }
    let autostart = e.config.join("autostart/arcade-find.desktop");
    fs::create_dir_all(autostart.parent().unwrap()).unwrap();
    fs::write(&autostart, "[Desktop Entry]\nType=Application\nX-Arcade-Id=arcade.find\nExec=/old/find --background\nHidden=true\n").unwrap();
    let receipt = install(&e, options("0.3.0", false)).unwrap();
    assert_eq!(receipt.method, InstallMethod::Appimage);
    assert!(source.exists());
    assert_eq!(fs::read(&receipt.path).unwrap(), b"version 1");
    assert_eq!(receipt.integration.autostart.as_ref(), Some(&autostart));
    assert!(fs::read_to_string(&autostart).unwrap().contains("Hidden=true"));
    assert!(!launchers.join("stale.desktop").exists());
    assert!(!launchers.join("old-find.desktop").exists());
    assert!(launchers.join("unrelated.desktop").exists());
    let desktop = fs::read_to_string(receipt.integration.desktop_entry.as_ref().unwrap()).unwrap();
    assert!(desktop.contains("X-Arcade-Id=arcade.find"));
    for action in ["Settings", "Quit", "Uninstall"] {
        assert!(desktop.contains(&format!("[Desktop Action {action}]")));
    }
    assert_eq!(receipt.integration.icons.len(), 2);
    assert_eq!(fs::read_link(receipt.integration.cli.as_ref().unwrap()).unwrap(), receipt.path);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(fs::metadata(&receipt.path).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(matches!(detect(&e, &app.id, &r).unwrap().state, State::InstalledElsewhere { .. }));
    assert_eq!(detect(&e, &app.id, &runtime(receipt.path.clone())).unwrap().state, State::InstalledHere);
    let before = fs::read(e.receipts().path(&app.id).unwrap()).unwrap();
    let status = integration_status(&e, &app.id, "fish").unwrap();
    assert!(!status.bin_on_path);
    assert!(status.path_hint.unwrap().starts_with("fish_add_path "));
    assert_eq!(fs::read(e.receipts().path(&app.id).unwrap()).unwrap(), before);
    e.path = Some(std::env::join_paths([e.bin.clone()]).unwrap());
    assert!(integration_status(&e, &app.id, "zsh").unwrap().bin_on_path);
    assert!(path_hint(&e.bin, "bash").starts_with("export PATH="));
    fs::write(&source, b"version 2").unwrap();
    let cli = receipt.integration.cli.as_ref().unwrap();
    fs::remove_file(cli).unwrap();
    fs::write(cli, b"a command owned by someone else").unwrap();
    let skipped = install(&e, options("0.3.0", false)).unwrap();
    assert!(skipped.integration.cli.is_none());
    assert!(skipped.integration.skipped.iter().any(|s| &s.path == cli));
    assert_eq!(fs::read(cli).unwrap(), b"a command owned by someone else");
    fs::remove_file(cli).unwrap();
    let updated = install(&e, options("0.3.1", true)).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&updated.path).unwrap(), b"version 2");
    let previous = updated.previous.as_ref().unwrap();
    assert_eq!(previous.version, "0.3.0");
    assert_eq!(fs::read(&previous.path).unwrap(), b"version 2");
    let moved = e.applications.join("Moved Find % $.AppImage");
    fs::rename(&updated.path, &moved).unwrap();
    fs::remove_file(updated.integration.desktop_entry.as_ref().unwrap()).unwrap();
    let repaired = repair(&e, &app, Some(&moved)).unwrap();
    assert_eq!(repaired.path, moved);
    assert_eq!(fs::read_link(repaired.integration.cli.as_ref().unwrap()).unwrap(), moved);
    let text = fs::read_to_string(repaired.integration.desktop_entry.as_ref().unwrap()).unwrap();
    assert!(text.contains("%%"));
    assert!(text.contains("\\\\$"));
    let data = e.data.join("arcade-find");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("settings"), b"settings").unwrap();
    let folders = [data.clone()];
    assert!(uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &folders }).unwrap());
    assert!(data.exists());
    assert!(!repaired.path.exists());
    assert!(!previous.path.exists());
    for p in repaired.integration.icons {
        assert!(!p.exists());
    }
    assert!(!autostart.exists());
    assert!(!uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap());
    fs::write(&source, b"version 3").unwrap();
    install(&e, options("0.3.2", false)).unwrap();
    assert!(!autostart.exists(), "does not create autostart without an explicit request");
    uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap();
    let mut reinstall = options("0.3.2", false);
    reinstall.start_at_login = true;
    install(&e, reinstall).unwrap();
    assert!(autostart.exists(), "creates autostart when explicitly requested");
    assert!(uninstall(&e, &app.id, UninstallOptions { remove_data: true, data_folders: &folders }).unwrap());
    assert!(!data.exists());
    assert!(launchers.join("unrelated.desktop").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn integration_failure_restores_current_previous_receipt_and_launchers() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = AppInfo {
        id: "arcade.find".into(),
        name: "Arcade Find".into(),
        filename: "Arcade-Find.AppImage".into(),
        desktop_id: "arcade-find".into(),
        cli_name: "arcade-find".into(),
        background_args: vec![],
        legacy_desktop_ids: vec![],
        autostart_id: "arcade-find".into(),
    };
    let source = root.0.join("download.AppImage");
    let r = runtime(source.clone());
    let opts = |version| InstallOptions {
        app: &app,
        runtime: &r,
        version,
        channel: "stable",
        managed_by: ManagedBy::SelfManaged,
        icons: &[],
        start_at_login: false,
        remove_download: true,
        refresh_caches: false,
    };
    fs::write(&source, b"v1").unwrap();
    install(&e, opts("1")).unwrap();
    fs::write(&source, b"v2").unwrap();
    let mut current = install(&e, opts("2")).unwrap();
    let desktop = current.integration.desktop_entry.as_ref().unwrap();
    let original_desktop = fs::read(desktop).unwrap();
    // This valid file lacks a main Exec. Failure occurs after the new binary,
    // icons and launcher have been written, exercising the rollback journal.
    let autostart = e.config.join("autostart/arcade-find.desktop");
    fs::create_dir_all(autostart.parent().unwrap()).unwrap();
    fs::write(&autostart, b"[Desktop Entry]\nX-Arcade-Id=arcade.find\nHidden=true\n").unwrap();
    current.integration.autostart = Some(autostart.clone());
    e.receipts().write(&current).unwrap();
    fs::write(&source, b"v3").unwrap();
    assert!(install(&e, opts("3")).is_err());
    assert_eq!(fs::read(&current.path).unwrap(), b"v2");
    assert_eq!(fs::read(&current.previous.as_ref().unwrap().path).unwrap(), b"v1");
    assert_eq!(fs::read(desktop).unwrap(), original_desktop);
    assert_eq!(fs::read_link(current.integration.cli.as_ref().unwrap()).unwrap(), current.path);
    assert_eq!(e.receipts().read(&app.id).unwrap().unwrap(), current);
    assert_eq!(fs::read(&source).unwrap(), b"v3");
    assert_eq!(fs::read(&autostart).unwrap(), b"[Desktop Entry]\nX-Arcade-Id=arcade.find\nHidden=true\n");
    for dir in [&e.applications, &e.applications.join(".previous")] {
        assert!(fs::read_dir(dir).unwrap().flatten().all(|p| {
            let name = p.file_name();
            let name = name.to_string_lossy();
            !name.ends_with("stage") && !name.ends_with("backup")
        }));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_dangling_symlink_at_the_destination_is_not_replaced() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let source = root.0.join("download.AppImage");
    fs::write(&source, b"fixture").unwrap();
    fs::create_dir_all(&e.applications).unwrap();
    let dest = e.applications.join(&app.filename);
    std::os::unix::fs::symlink(root.0.join("missing-target"), &dest).unwrap();
    let r = runtime(source);
    let error = install(&e, find_options(&app, &r, "1")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(fs::symlink_metadata(&dest).unwrap().file_type().is_symlink());
    assert!(e.receipts().read(&app.id).unwrap().is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn launchers_escape_newlines_and_route_percent_paths_through_env() {
    for home in ["home\nwith newline", "home % percent"] {
        let root = Root::new();
        let e = Environment::under(&root.0.join(home));
        let app = find_info();
        fs::create_dir_all(&e.home).unwrap();
        let source = e.home.join("download.AppImage");
        fs::write(&source, b"fixture").unwrap();
        let r = runtime(source);
        let receipt = install(&e, find_options(&app, &r, "1")).unwrap();
        let entry = receipt.integration.desktop_entry.clone().unwrap();
        let text = fs::read_to_string(&entry).unwrap();
        for line in text.lines().filter(|l| !l.is_empty()) {
            assert!(line.starts_with('#') || line.starts_with('[') || line.contains('='), "stray line {line:?} in\n{text}");
        }
        let exec = text.lines().find(|l| l.starts_with("Exec=")).unwrap();
        assert_eq!(exec.starts_with("Exec=env \""), home.contains('%'), "{exec}");
        if which("desktop-file-validate") {
            let out = std::process::Command::new("desktop-file-validate").arg(&entry).output().unwrap();
            let errors = String::from_utf8_lossy(&out.stdout).lines().filter(|l| l.contains("error:")).map(String::from).collect::<Vec<_>>();
            assert!(errors.is_empty(), "{errors:?}\n{text}");
        }
        // The installer still recognizes the entry as its own on reinstall.
        let again = install(&e, find_options(&app, &r, "2")).unwrap();
        assert_eq!(again.integration.desktop_entry, Some(entry));
        assert!(again.integration.skipped.is_empty(), "{:?}", again.integration.skipped);
    }
}

#[cfg(target_os = "linux")]
fn which(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

#[cfg(target_os = "linux")]
#[test]
fn a_pre_installer_launcher_and_login_entry_are_taken_over() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let downloads = root.0.join("Downloads");
    fs::create_dir_all(&downloads).unwrap();
    let legacy = downloads.join("Arcade-Find-0.2.1-x86_64.AppImage");
    fs::write(&legacy, b"old release").unwrap();
    let launcher = e.data.join("applications/arcade-find.desktop");
    let login = e.config.join("autostart/arcade-find.desktop");
    for path in [&launcher, &login] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("[Desktop Entry]\nType=Application\nName=Arcade Find\nExec=\"{}\" --background\n", legacy.display())).unwrap();
    }
    assert_eq!(legacy_copies(&e, &app), vec![legacy.clone()]);
    let source = downloads.join("Arcade-Find-x86_64.AppImage");
    fs::write(&source, b"new release").unwrap();
    let r = runtime(source.clone());
    let mut options = find_options(&app, &r, "0.3.1");
    options.remove_download = true;
    let receipt = install(&e, options).unwrap();
    assert!(receipt.integration.skipped.is_empty(), "{:?}", receipt.integration.skipped);
    assert_eq!(receipt.integration.autostart.as_ref(), Some(&login));
    for path in [&launcher, &login] {
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains(&receipt.path.display().to_string()) && !text.contains("0.2.1"), "{text}");
    }
    assert!(!source.exists() && !legacy.exists(), "downloaded copies are removed when asked");
    assert!(legacy_copies(&e, &app).is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn only_files_in_downloads_are_removed_and_stale_stages_are_cleaned() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    fs::create_dir_all(&e.applications).unwrap();
    let stale = e.applications.join(format!("Arcade-Find.{}.stage", arcade_link::endpoint::new_token().unwrap()));
    fs::write(&stale, b"half a copy").unwrap();
    let unrelated = e.applications.join("Arcade-Findings.stage");
    fs::write(&unrelated, b"not ours").unwrap();
    let kept = root.0.join("Apps/Arcade-Find-x86_64.AppImage");
    fs::create_dir_all(kept.parent().unwrap()).unwrap();
    fs::write(&kept, b"fixture").unwrap();
    let r = runtime(kept.clone());
    let mut options = find_options(&app, &r, "1");
    options.remove_download = true;
    install(&e, options).unwrap();
    assert!(kept.exists(), "a file outside Downloads is the user's");
    assert!(!stale.exists() && unrelated.exists());
}

#[cfg(target_os = "linux")]
#[test]
fn installs_and_uninstalls_of_one_app_take_turns() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    let app = find_info();
    let source = root.0.join("download.AppImage");
    fs::write(&source, b"fixture").unwrap();
    let held = lock(&e, &app.id).unwrap();
    let (env2, app2, source2) = (e.clone(), app.clone(), source.clone());
    let worker = std::thread::spawn(move || {
        let r = runtime(source2);
        install(&env2, find_options(&app2, &r, "1")).map(|_| ())
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(e.receipts().read(&app.id).unwrap().is_none(), "install ran while another holder had the lock");
    drop(held);
    worker.join().unwrap().unwrap();
    assert!(e.receipts().read(&app.id).unwrap().is_some());
    assert!(uninstall(&e, &app.id, UninstallOptions { remove_data: false, data_folders: &[] }).unwrap());
}

#[cfg(target_os = "linux")]
#[test]
fn install_errors_name_the_folder() {
    let root = Root::new();
    let e = Environment::under(&root.0);
    fs::create_dir_all(e.applications.parent().unwrap()).unwrap();
    fs::write(&e.applications, b"a file where the apps folder goes").unwrap();
    let source = root.0.join("download.AppImage");
    fs::write(&source, b"fixture").unwrap();
    let app = find_info();
    let r = runtime(source);
    let error = install(&e, find_options(&app, &r, "1")).unwrap_err().to_string();
    assert!(error.contains(&e.applications.display().to_string()), "{error}");
}
