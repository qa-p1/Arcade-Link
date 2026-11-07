//! The shared runtime contract (RT-1…RT-5), exercised in real processes: the
//! test binary re-runs itself as the child where a test needs one.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use arcade_link::runtime;

const ROLE: &str = "ARCADE_RUNTIME_TEST_ROLE";
const DIR: &str = "ARCADE_RUNTIME_TEST_DIR";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("al-runtime-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run_child(role: &str, dir: &Path) -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args(["--exact", "child", "--nocapture", "--test-threads=1"]).env(ROLE, role).env(DIR, dir);
    c
}

/// The body of every child process; does nothing in a normal test run.
#[test]
fn child() {
    let (Ok(role), Ok(dir)) = (std::env::var(ROLE), std::env::var(DIR)) else { return };
    let dir = PathBuf::from(dir);
    match role.as_str() {
        "hold-lock" => {
            let _lock = runtime::acquire_instance(&dir, "app").unwrap().expect("first instance");
            std::fs::write(dir.join("locked"), "").unwrap();
            std::thread::sleep(Duration::from_secs(2));
        }
        "panic" => {
            runtime::install_crash_reports("Arcade Test", "1.2.3", dir);
            panic!("the test asked for a panic");
        }
        "segv" => {
            runtime::install_crash_reports("Arcade Test", "1.2.3", dir);
            unsafe { libc_raise_segv() };
        }
        "successor" => {
            std::fs::write(dir.join("successor"), std::env::var(runtime::RESTART_AFTER_ENV).unwrap_or_default()).unwrap();
        }
        "restart" => {
            std::env::set_var(ROLE, "successor");
            runtime::spawn_successor(&["--exact", "child", "--nocapture", "--test-threads=1"]).unwrap();
            std::fs::write(dir.join("predecessor"), std::process::id().to_string()).unwrap();
        }
        "unpack" => {
            if let Some(code) = runtime::relaunch_unpacked("arcade.test", "1.2.3", true) {
                std::process::exit(code);
            }
            let exe = std::env::current_exe().unwrap();
            std::fs::write(dir.join("ran-from"), format!("{}\n{}", exe.display(), std::env::var("APPDIR").unwrap_or_default())).unwrap();
            std::process::exit(7);
        }
        other => panic!("unknown role {other}"),
    }
}

#[cfg(unix)]
unsafe fn libc_raise_segv() {
    extern "C" {
        fn raise(sig: i32) -> i32;
    }
    raise(11);
}

#[cfg(not(unix))]
unsafe fn libc_raise_segv() {}

#[test]
fn only_one_instance_holds_the_slot_and_a_crash_frees_it() {
    let dir = tmp("instance");
    let first = runtime::acquire_instance(&dir, "app").unwrap().unwrap();
    assert!(runtime::acquire_instance(&dir, "app").unwrap().is_none());
    drop(first);
    let again = runtime::acquire_instance(&dir, "app").unwrap();
    assert!(again.is_some());
    drop(again);

    let mut holder = run_child("hold-lock", &dir).spawn().unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while !dir.join("locked").exists() {
        assert!(Instant::now() < end, "child never locked");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(runtime::acquire_instance(&dir, "app").unwrap().is_none());
    holder.kill().unwrap();
    holder.wait().unwrap();
    assert!(runtime::acquire_instance_waiting(&dir, "app", Duration::from_secs(5)).unwrap().is_some());
}

#[test]
fn a_panic_leaves_a_local_report() {
    let dir = tmp("panic");
    let out = run_child("panic", &dir).output().unwrap();
    assert!(!out.status.success());
    let reports = runtime::crash_reports(&dir);
    assert_eq!(reports.len(), 1, "{reports:?}");
    let text = std::fs::read_to_string(&reports[0]).unwrap();
    for needle in ["app: Arcade Test", "version: 1.2.3", "kind: panic", "the test asked for a panic", "backtrace:"] {
        assert!(text.contains(needle), "missing {needle:?} in\n{text}");
    }
}

#[cfg(unix)]
#[test]
fn a_fatal_signal_leaves_a_report_and_still_kills_the_process() {
    use std::os::unix::process::ExitStatusExt;
    let dir = tmp("segv");
    let out = run_child("segv", &dir).output().unwrap();
    assert_eq!(out.status.signal(), Some(11), "{:?}", out.status);
    let reports = runtime::crash_reports(&dir);
    assert_eq!(reports.len(), 1, "{reports:?}");
    let text = std::fs::read_to_string(&reports[0]).unwrap();
    assert!(text.contains("kind: fatal signal") && text.contains("signal: 11"), "{text}");
}

#[test]
fn old_reports_are_pruned() {
    let dir = tmp("prune");
    for i in 0..30 {
        std::fs::write(dir.join(format!("{i:011}-1-panic.crash")), "x").unwrap();
    }
    std::fs::write(dir.join("notes.txt"), "kept").unwrap();
    runtime::install_crash_reports("Arcade Test", "1", dir.clone());
    let left = runtime::crash_reports(&dir);
    assert_eq!(left.len(), 19);
    assert!(left[0].ends_with("00000000029-1-panic.crash"));
    assert!(dir.join("notes.txt").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn reaped_children_never_linger_as_zombies() {
    let pid = runtime::spawn_reaped(Command::new("true").arg("x")).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Err(_) => break,
            Ok(stat) => {
                assert!(Instant::now() < end, "still present: {stat}");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

#[test]
fn a_restart_starts_the_installed_program_with_the_predecessor_pid() {
    let dir = tmp("restart");
    let out = run_child("restart", &dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let predecessor = std::fs::read_to_string(dir.join("predecessor")).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while !dir.join("successor").exists() {
        assert!(Instant::now() < end, "the successor never ran");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(std::fs::read_to_string(dir.join("successor")).unwrap(), predecessor);
}

#[test]
fn waiting_for_a_process_returns_once_it_exits() {
    let mut sleeper = Command::new(if cfg!(windows) { "ping" } else { "sleep" })
        .args(if cfg!(windows) { vec!["-n", "2", "127.0.0.1"] } else { vec!["1"] })
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    assert!(runtime::process_alive(sleeper.id()));
    let pid = sleeper.id();
    let start = Instant::now();
    let reaper = std::thread::spawn(move || sleeper.wait());
    runtime::wait_for_exit(pid, Duration::from_secs(10));
    reaper.join().unwrap().unwrap();
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[cfg(target_os = "linux")]
#[test]
fn trimming_returns_freed_heap_to_the_system() {
    fn rss_kb() -> u64 {
        let s = std::fs::read_to_string("/proc/self/status").unwrap();
        s.lines().find(|l| l.starts_with("VmRSS:")).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap()
    }
    let mut blocks: Vec<Vec<u8>> = (0..120_000).map(|i| vec![i as u8; 600]).collect();
    let pin = vec![1u8; 600];
    blocks.truncate(0);
    blocks.shrink_to_fit();
    let before = rss_kb();
    runtime::trim();
    let after = rss_kb();
    assert!(before > after + 30_000, "RSS {before} kB → {after} kB");
    assert_eq!(pin[0], 1);
}

#[cfg(target_os = "linux")]
#[test]
fn an_appimage_app_relaunches_from_its_unpacked_copy_once() {
    use std::os::unix::fs::symlink;
    let dir = tmp("unpack");
    let appdir = dir.join("mount");
    std::fs::create_dir_all(appdir.join("usr/bin")).unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), appdir.join("usr/bin/app")).unwrap();
    symlink("usr/bin/app", appdir.join("AppRun")).unwrap();
    std::fs::write(appdir.join("usr/data.txt"), "payload").unwrap();
    let appimage = dir.join("App.AppImage");
    std::fs::write(&appimage, "image").unwrap();
    let data = dir.join("data");

    let run = || {
        Command::new(appdir.join("usr/bin/app"))
            .args(["--exact", "child", "--nocapture", "--test-threads=1"])
            .env(ROLE, "unpack")
            .env(DIR, &dir)
            .env("APPIMAGE", &appimage)
            .env("APPDIR", &appdir)
            .env("XDG_DATA_HOME", &data)
            .env_remove(runtime::UNPACKED_ENV)
            .output()
            .unwrap()
    };
    let out = run();
    assert_eq!(out.status.code(), Some(7), "the unpacked copy's exit code comes back: {}", String::from_utf8_lossy(&out.stderr));
    let ran = std::fs::read_to_string(dir.join("ran-from")).unwrap();
    let unpacked = data.join("arcade/runtime");
    let (exe, appdir_seen) = ran.split_once('\n').unwrap();
    assert!(Path::new(exe).starts_with(&unpacked), "ran from {exe}");
    assert!(Path::new(appdir_seen).starts_with(&unpacked), "APPDIR {appdir_seen}");
    let copy = Path::new(appdir_seen);
    assert_eq!(std::fs::read_to_string(copy.join("usr/data.txt")).unwrap(), "payload");
    assert!(std::fs::symlink_metadata(copy.join("AppRun")).unwrap().file_type().is_symlink());
    assert!(copy.join(".complete").exists());

    // A second start reuses the copy; a changed AppImage gets a new one.
    std::fs::write(copy.join("usr/marker"), "kept").unwrap();
    assert_eq!(run().status.code(), Some(7));
    assert!(copy.join("usr/marker").exists());
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(&appimage, "image v2").unwrap();
    assert_eq!(run().status.code(), Some(7));
    assert_eq!(std::fs::read_dir(&unpacked).unwrap().count(), 2);
}
