//! The debug CLI and the mock peer, end to end, as separate processes.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_arcade-link");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

struct Home(PathBuf);

impl Home {
    fn new(name: &str) -> Home {
        let p = std::env::temp_dir().join(format!("alc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Home(p)
    }

    fn cmd(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .args(args)
            .env("ARCADE_HOME", &self.0)
            .output()
            .unwrap()
    }

    fn mock(&self, as_id: &str, fixture: &str) -> Child {
        let mut child = Command::new(BIN)
            .args([
                "mock",
                "--as",
                as_id,
                "--actions",
                fixtures().join(format!("{fixture}.json")).to_str().unwrap(),
            ])
            .env("ARCADE_HOME", &self.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if String::from_utf8_lossy(&self.cmd(&["ls"]).stdout).contains("running") {
                return child;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("mock {fixture} did not start");
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        // Stop any mock still running (launch-on-demand ones included).
        for id in ["box", "clipboard", "look", "lens", "wheel"] {
            let _ = Command::new(BIN)
                .args(["quit", id, "--force"])
                .env("ARCADE_HOME", &self.0)
                .output();
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn ls_describe_invoke_against_a_mock() {
    let home = Home::new("basic");
    let mut mock = home.mock("box", "box");
    let ls = text(&home.cmd(&["ls"]));
    assert!(ls.contains("arcade.box") && ls.contains("running"), "{ls}");
    let d = text(&home.cmd(&["describe", "box"]));
    assert!(
        d.contains("live")
            && d.contains("Convert to WebP")
            && d.contains("Tesseract isn't installed"),
        "{d}"
    );
    let img = home.0.join("a.png");
    std::fs::write(&img, b"png").unwrap();
    let r = home.cmd(&[
        "invoke",
        "box",
        "box:arcade.image.convert",
        "--preset",
        "webp",
        "--file",
        img.to_str().unwrap(),
    ]);
    let out = text(&r);
    assert!(
        r.status.success() && out.contains("step 1") && out.contains("Convert to WebP done"),
        "{out}"
    );
    let r = home.cmd(&[
        "invoke",
        "box",
        "box:arcade.pdf.ocr",
        "--file",
        img.to_str().unwrap(),
    ]);
    assert!(
        !r.status.success()
            && text(&r).contains("Arcade Box can't do this yet: Tesseract isn't installed."),
        "{}",
        text(&r)
    );
    // The peer crashes mid-job: the caller gets a clean error, not a hang.
    let r = home.cmd(&[
        "invoke",
        "box",
        "box:arcade.media.inspect",
        "--file",
        img.to_str().unwrap(),
    ]);
    assert!(
        !r.status.success() && text(&r).contains("isn't running"),
        "{}",
        text(&r)
    );
    let _ = mock.wait();
}

#[test]
fn headless_actions_run_one_shot_and_interactive_ones_launch_the_app() {
    let home = Home::new("lifecycle");
    let mut mock = home.mock("box", "box");
    assert!(home.cmd(&["quit", "box"]).status.success());
    let _ = mock.wait();
    assert!(text(&home.cmd(&["ls"])).contains("installed"));
    // Not running + headless + launch.invoke → one-shot process; no resident instance appears.
    let r = home.cmd(&[
        "invoke",
        "box",
        "box:arcade.text.structured",
        "--preset",
        "format-json",
        "--text",
        "{\"ok\":true}",
    ]);
    assert!(
        r.status.success() && text(&r).contains("Formatted"),
        "{}",
        text(&r)
    );
    assert!(!text(&home.cmd(&["ls"])).contains("running"));
    // Not running + interactive → launched in the background, then invoked.
    let r = home.cmd(&["invoke", "box", "box.open", "--text", "hello"]);
    assert!(
        r.status.success() && text(&r).contains("starting arcade.box"),
        "{}",
        text(&r)
    );
    assert!(text(&home.cmd(&["ls"])).contains("running"));
}

#[test]
fn private_mode_and_missing_apps_map_to_standard_messages() {
    let home = Home::new("errors");
    let mut mock = home.mock("clipboard", "private-clipboard");
    let r = home.cmd(&["invoke", "clipboard", "clipboard.add", "--text", "hello"]);
    assert!(
        text(&r).contains("Arcade Clipboard is in Private mode."),
        "{}",
        text(&r)
    );
    let r = home.cmd(&["invoke", "look", "look.preview", "--text", "x"]);
    assert!(
        !r.status.success() && text(&r).contains("arcade.look is not installed"),
        "{}",
        text(&r)
    );
    let _ = home.cmd(&["quit", "clipboard"]);
    let _ = mock.wait();
}

#[test]
fn manifests_validate() {
    let home = Home::new("check");
    let path = home.0.join("m.json");
    std::fs::write(
        &path,
        r#"{"schema":1,"id":"arcade.x","name":"X","executable":"/x","actions":[]}"#,
    )
    .unwrap();
    assert!(home
        .cmd(&["check-manifest", path.to_str().unwrap()])
        .status
        .success());
    std::fs::write(&path, r#"{"schema":1,"name":"X"}"#).unwrap();
    assert!(!home
        .cmd(&["check-manifest", path.to_str().unwrap()])
        .status
        .success());
}
