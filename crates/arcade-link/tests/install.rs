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

#[test]
fn relative_install_environment_is_refused_before_writing() {
    let e = Environment::under(Path::new("relative-install-root"));
    assert!(e.validate().is_err());
    assert!(uninstall(&e, "arcade.find", UninstallOptions { remove_data: false, data_folders: &[] }).is_err());
    assert!(!e.home.exists());
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
    let source = root.0.join("download % dollar$.AppImage");
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
            format!("[Desktop Entry]\nExec=/old/executable\n{}", if name == "stale" { "X-Arcade-Id=arcade.find\n" } else { "" }),
        )
        .unwrap();
    }
    let autostart = e.config.join("autostart/arcade-find.desktop");
    fs::create_dir_all(autostart.parent().unwrap()).unwrap();
    fs::write(&autostart, "[Desktop Entry]\nExec=/old/find --background\nHidden=true\n").unwrap();
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
    assert!(install(&e, options("0.3.1", true)).is_err());
    assert_eq!(fs::read(&receipt.path).unwrap(), b"version 1", "preflight failures preserve the working installation");
    assert_eq!(e.receipts().read(&app.id).unwrap().unwrap().version, "0.3.0");
    assert!(source.exists());
    fs::remove_file(cli).unwrap();
    repair(&e, &app, None).unwrap();
    let updated = install(&e, options("0.3.1", true)).unwrap();
    assert!(!source.exists());
    assert_eq!(fs::read(&updated.path).unwrap(), b"version 2");
    let previous = updated.previous.as_ref().unwrap();
    assert_eq!(previous.version, "0.3.0");
    assert_eq!(fs::read(&previous.path).unwrap(), b"version 1");
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
    let current = install(&e, opts("2")).unwrap();
    let desktop = current.integration.desktop_entry.as_ref().unwrap();
    let original_desktop = fs::read(desktop).unwrap();
    // This valid file lacks a main Exec. Failure occurs after the new binary,
    // icons and launcher have been written, exercising the rollback journal.
    let autostart = e.config.join("autostart/arcade-find.desktop");
    fs::create_dir_all(autostart.parent().unwrap()).unwrap();
    fs::write(&autostart, b"[Desktop Entry]\nHidden=true\n").unwrap();
    fs::write(&source, b"v3").unwrap();
    assert!(install(&e, opts("3")).is_err());
    assert_eq!(fs::read(&current.path).unwrap(), b"v2");
    assert_eq!(fs::read(&current.previous.as_ref().unwrap().path).unwrap(), b"v1");
    assert_eq!(fs::read(desktop).unwrap(), original_desktop);
    assert_eq!(fs::read_link(current.integration.cli.as_ref().unwrap()).unwrap(), current.path);
    assert_eq!(e.receipts().read(&app.id).unwrap().unwrap(), current);
    assert_eq!(fs::read(&source).unwrap(), b"v3");
    assert_eq!(fs::read(&autostart).unwrap(), b"[Desktop Entry]\nHidden=true\n");
    for dir in [&e.applications, &e.applications.join(".previous")] {
        assert!(fs::read_dir(dir).unwrap().flatten().all(|p| {
            let name = p.file_name();
            let name = name.to_string_lossy();
            !name.ends_with("stage") && !name.ends_with("backup")
        }));
    }
}
