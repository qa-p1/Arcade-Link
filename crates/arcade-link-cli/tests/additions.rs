use arcade_link::{Client, Locations, PeerInfo};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_arcade-link");
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("alc-v3-{}", arcade_link::endpoint::new_token().unwrap()));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args).env("ARCADE_HOME", &self.0);
        c
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct OwnedChild(Child);
impl OwnedChild {
    fn send(&mut self, line: &str) {
        self.0.stdin.as_mut().unwrap().write_all(line.as_bytes()).unwrap();
        self.0.stdin.as_mut().unwrap().flush().unwrap();
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(format!("{name}.json"))
}
fn ready(root: &Root, id: &str) -> Client {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(c) = Client::connect(&Locations::under(&root.0), id, &PeerInfo::default()) {
            return c;
        }
        assert!(Instant::now() < deadline, "mock did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn lines(child: &mut OwnedChild) -> mpsc::Receiver<String> {
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}
fn expect(rx: &mpsc::Receiver<String>, state: &str) {
    assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), state);
}

#[test]
fn mock_answers_every_new_method_and_persists_toggles_and_shortcuts() {
    let root = Root::new();
    let mut fixture_value: Value = serde_json::from_slice(&fs::read(fixture("clipboard")).unwrap()).unwrap();
    fixture_value["shortcuts"] = json!([{"id":"toggle","accelerator":"Ctrl+Shift+Space"}]);
    let fixture_file = root.0.join("clipboard.json");
    fs::write(&fixture_file, fixture_value.to_string()).unwrap();
    let mut child = OwnedChild(
        root.command(&["mock", "--as", "clipboard", "--actions", fixture_file.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut client = ready(&root, "arcade.clipboard");
    assert_eq!(client.describe_full().unwrap().methods.len(), 7);
    client.settings().unwrap();
    client.restart(None).unwrap();
    let menu = client.menu().unwrap();
    assert_eq!(menu[0].checked, Some(false));
    client.menu_invoke(&menu[0].id).unwrap();
    assert_eq!(client.menu().unwrap()[0].checked, Some(true));
    let doc: Value = serde_json::from_slice(&fs::read(root.0.join("apps/arcade.clipboard.json")).unwrap()).unwrap();
    let id = doc["shortcuts"][0]["id"].as_str().unwrap();
    assert!(client.shortcuts_set(id, "Control+alt+g").unwrap().applied);
    let changed: Value = serde_json::from_slice(&fs::read(root.0.join("apps/arcade.clipboard.json")).unwrap()).unwrap();
    assert_eq!(changed["shortcuts"][0]["accelerator"], "Ctrl+Alt+G");
    let files = client.settings_export().unwrap();
    assert!(files.iter().all(|f| Path::new(f.path.as_ref().unwrap()).starts_with(&root.0)));
    assert!(client.settings_import(&files).unwrap().imported);
    client.settings_import(&files).unwrap();
    assert!(root.0.join("handoff/mock-imported-settings.backup.json").exists());
    let ls = root.command(&["ls", "--json"]).output().unwrap();
    let rows: Value = serde_json::from_slice(&ls.stdout).unwrap();
    assert_eq!(rows[0]["tray"], "own");
    assert!(rows[0].get("install").is_some());
    let shortcuts = root.command(&["shortcuts"]).output().unwrap();
    assert!(String::from_utf8_lossy(&shortcuts.stdout).contains("Ctrl+Alt+G"));
    child.send("unused\n");
}
#[test]
fn scriptable_shortcut_results_and_unsupported() {
    let root = Root::new();
    let mut doc: Value = serde_json::from_slice(&fs::read(fixture("find")).unwrap()).unwrap();
    doc["mockMethods"] = json!({"app.shortcuts.set":{"result":{"applied":false,"via":"manual","hint":"Bind arcade-find --toggle in Settings → Keyboard"}}});
    let file = root.0.join("fixture.json");
    fs::write(&file, doc.to_string()).unwrap();
    let _child = OwnedChild(
        root.command(&["mock", "--as", "find", "--actions", file.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut c = ready(&root, "arcade.find");
    let id = doc["shortcuts"][0]["id"].as_str().unwrap();
    let r = c.shortcuts_set(id, "Ctrl+Alt+G").unwrap();
    assert!(!r.applied);
    assert!(r.hint.is_some());
    doc["methods"] = json!([]);
    let oldfile = root.0.join("old.json");
    fs::write(&oldfile, doc.to_string()).unwrap();
    let _old = OwnedChild(
        root.command(&["mock", "--as", "old", "--actions", oldfile.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(ready(&root, "arcade.old").settings().unwrap_err().is_unsupported());
}
#[test]
fn cli_and_mock_tray_hosts_are_scriptable_and_crash_hands_back() {
    for mock in [false, true] {
        let root = Root::new();
        let mut command =
            if mock { root.command(&["mock", "--as", "tools", "--actions", fixture("tools").to_str().unwrap()]) } else { root.command(&["tray", "host"]) };
        let mut host = OwnedChild(command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
        drop(ready(&root, "arcade.tools"));
        let mut app = OwnedChild(root.command(&["tray", "find"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap());
        let states = lines(&mut app);
        expect(&states, "hosted");
        host.send("hosted false\n");
        expect(&states, "own");
        host.send("hosted true\n");
        expect(&states, "hosted");
        host.send("exclude find\n");
        expect(&states, "own");
        host.send("include find\n");
        expect(&states, "hosted");
        app.send("link off\n");
        expect(&states, "own");
        app.send("link on\n");
        expect(&states, "hosted");
        let started = Instant::now();
        host.send("crash\n");
        expect(&states, "own");
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
#[test]
fn shortcuts_validate_and_markdown_match_shared_vectors() {
    let root = Root::new();
    let v: Value = serde_json::from_slice(&fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/vectors/shortcuts.json")).unwrap()).unwrap();
    let case = &v["valid"][0];
    let file = root.0.join("shortcuts.json");
    fs::write(&file, case["document"].to_string()).unwrap();
    assert!(root.command(&["shortcuts", "validate", file.to_str().unwrap()]).output().unwrap().status.success());
    let output = root.command(&["shortcuts", "markdown", file.to_str().unwrap()]).output().unwrap();
    assert_eq!(String::from_utf8(output.stdout).unwrap(), case["markdown"].as_str().unwrap());
    fs::write(&file, v["invalid"][0]["document"].to_string()).unwrap();
    assert!(!root.command(&["shortcuts", "validate", file.to_str().unwrap()]).output().unwrap().status.success());
}
