//! The real helper program quits over the Link the way Arcade Tools asks it to before replacing it: at once, with
//! its endpoint gone, so `quit_running` doesn't wait out its timeout on any OS.

use std::time::{Duration, Instant};

use arcade_link::Locations;

#[test]
fn the_resident_quits_at_once_and_leaves_no_endpoint() {
    let root = std::env::temp_dir().join(format!("arcade-helper-quit-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let locations = Locations::under(&root.join("arcade"));
    let helper = |arg: &str| {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_arcade-helper"));
        c.arg(arg)
            .env("ARCADE_HOME", root.join("arcade"))
            .env("HOME", root.join("home"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("APPDATA", root.join("roaming"))
            .env("LOCALAPPDATA", root.join("local"));
        c
    };
    let mut child = helper("--background").spawn().unwrap();
    let start = Instant::now();
    while arcade_link::endpoint::read(&locations, "arcade.helper").is_err() {
        assert!(start.elapsed() < Duration::from_secs(20), "the helper didn't start");
        std::thread::sleep(Duration::from_millis(50));
    }
    // `status`: the running version and where its menus went (Linux: the Open With entry at least). The helper
    // answers as soon as its endpoint is up and writes its menus just after, so the menus may take a moment.
    let start = Instant::now();
    loop {
        let out = helper("status").output().unwrap();
        let status: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(status["running"], status["version"], "{status}");
        let open_with = status["fileManagers"].as_array().unwrap().iter().any(|n| n == "Open With (every file manager)");
        if open_with || !cfg!(target_os = "linux") {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "no menus written: {status}");
        std::thread::sleep(Duration::from_millis(100));
    }
    // `quit` (what Arcade Tools' Windows installer runs), then the Link quit Tools uses: both stop it at once.
    let out = helper("quit").output().unwrap();
    assert_eq!((out.status.code(), String::from_utf8_lossy(&out.stdout).trim().to_string()), (Some(0), "stopped".to_string()));
    assert!(child.wait().unwrap().success());
    let out = helper("quit").output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "not running");
    let mut child = helper("--background").spawn().unwrap();
    let start = Instant::now();
    while arcade_link::endpoint::read(&locations, "arcade.helper").is_err() {
        assert!(start.elapsed() < Duration::from_secs(20), "the helper didn't start again");
        std::thread::sleep(Duration::from_millis(50));
    }
    let start = Instant::now();
    let quit = arcade_link::install::quit_running(&locations, "arcade.helper", Duration::from_secs(10)).unwrap();
    assert!(quit && start.elapsed() < Duration::from_secs(3), "quit took {:?}", start.elapsed());
    assert!(arcade_link::endpoint::read(&locations, "arcade.helper").is_err(), "endpoint left behind");
    let status = child.wait().unwrap();
    assert!(status.success(), "{status}");
    // It wrote its menus at start (on Windows into the real HKCU); take them away again.
    let mut remove = helper("integrate");
    let out = remove.arg("--remove").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("removed"), "{}", String::from_utf8_lossy(&out.stdout));
    std::fs::remove_dir_all(&root).ok();
}

/// Uninstalling while the helper runs (`integrate --remove`: the entries, then its card) stops it, and it doesn't
/// write the entries back on its way out.
#[test]
fn removing_its_card_stops_the_resident() {
    let root = std::env::temp_dir().join(format!("arcade-helper-uninstall-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let locations = Locations::under(&root.join("arcade"));
    let helper = |arg: &str| {
        let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_arcade-helper"));
        c.arg(arg)
            .env("ARCADE_HOME", root.join("arcade"))
            .env("HOME", root.join("home"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("APPDATA", root.join("roaming"))
            .env("LOCALAPPDATA", root.join("local"));
        c
    };
    let mut child = helper("--background").spawn().unwrap();
    let start = Instant::now();
    while !locations.manifest("arcade.helper").exists() || arcade_link::endpoint::read(&locations, "arcade.helper").is_err() {
        assert!(start.elapsed() < Duration::from_secs(20), "the helper didn't start");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(500));
    let out = helper("integrate").arg("--remove").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(10) {
            child.kill().ok();
            panic!("the helper still runs 10 s after its card was removed");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "{status}");
    assert!(arcade_link::endpoint::read(&locations, "arcade.helper").is_err(), "endpoint left behind");
    // Nothing written back (Linux: the Open With entries live under XDG_DATA_HOME/applications).
    std::thread::sleep(Duration::from_millis(500));
    #[cfg(target_os = "linux")]
    {
        let apps = root.join("data").join("applications");
        let left: Vec<_> = std::fs::read_dir(&apps).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        assert!(left.is_empty(), "entries written back: {left:?}");
    }
    let out = helper("integrate").arg("--remove").output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("0 written, 0 removed"), "{}", String::from_utf8_lossy(&out.stdout));
    std::fs::remove_dir_all(&root).ok();
}
