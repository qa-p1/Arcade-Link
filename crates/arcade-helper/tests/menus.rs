//! The menu entries the helper writes, launched the way each file manager launches them (Link SPEC §5.6; PLAN §5
//! items 3 and 4). The entries point at a copy of this test program in a folder with awkward characters; started
//! with `ARCADE_ARGV_RECORD` set, it only records its arguments, so each check sees exactly what the helper would get.
//!
//! - Linux: "Open With" entries through GLib's own launcher (`gio launch`, what Nautilus, Nemo and Caja use) after
//!   `desktop-file-validate`; Nautilus and Caja scripts run directly; Dolphin service menus (also checked for the
//!   executable bit Plasma 6 requires) and Nemo actions by the Exec rules; Thunar's `uca.xml` checked by `xmllint`
//!   and its command run by `sh` as Thunar does. Removing takes away every entry and keeps the user's own Thunar
//!   action.
//! - Windows (in CI only: it writes the real `HKCU`): each verb's command line run with the file in place of `%1`;
//!   removing leaves no key.
//!
//! The validators are required where `CI` is set; elsewhere a missing one is reported and its check left out.

// The shared helpers are used by the Linux and Windows checks only.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use arcade_helper::integrations::{self, Entry, Home};

const RECORD: &str = "ARCADE_ARGV_RECORD";

fn main() {
    if let Some(out) = std::env::var_os(RECORD) {
        let args: Vec<String> = std::env::args().skip(1).collect();
        std::fs::write(out, args.join("\n")).expect("record the arguments");
        return;
    }
    let root = std::env::temp_dir().join(format!("arcade-helper-menus-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    #[cfg(target_os = "linux")]
    linux::run(&root);
    #[cfg(windows)]
    windows::run(&root);
    std::fs::remove_dir_all(&root).ok();
    println!("menus: ok");
}

fn entries() -> Vec<Entry> {
    vec![
        Entry::picker(),
        Entry {
            reference: "arcade.box/box:arcade.image.convert#webp".into(),
            title: "Convert to \"WebP\" $1".into(),
            accepts: vec!["file/image".into(), "file/image[]".into()],
        },
    ]
}

/// What the helper must receive for `entry` and `files`.
fn expected(entry: &Entry, files: &[&Path]) -> Vec<String> {
    entry.args().into_iter().chain(files.iter().map(|f| f.to_string_lossy().into_owned())).collect()
}

/// A copy of this program in `dir` (named `name`), so entries can launch it.
fn recorder(dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let exe = dir.join(name);
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    exe
}

/// Waits for the recorded arguments (launchers like `gio launch` return before the program runs).
fn recorded(out: &Path) -> Vec<String> {
    let start = Instant::now();
    loop {
        if let Ok(s) = std::fs::read_to_string(out) {
            std::fs::remove_file(out).ok();
            return s.lines().map(str::to_string).collect();
        }
        assert!(start.elapsed() < Duration::from_secs(10), "nothing recorded at {}", out.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A required tool, or `None` (with a note) outside CI.
fn tool(name: &str) -> Option<String> {
    let found = std::process::Command::new("sh").args(["-c", &format!("command -v {name}")]).output().is_ok_and(|o| o.status.success());
    if found {
        return Some(name.to_string());
    }
    assert!(std::env::var_os("CI").is_none(), "{name} is required in CI");
    eprintln!("menus: {name} not installed here, its check is left out");
    None
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Command;

    /// One value from a desktop-entry style file: `key` in `[group]`, with the file's string escapes undone.
    fn value(contents: &str, group: &str, key: &str) -> String {
        let mut inside = false;
        for line in contents.lines() {
            if line.starts_with('[') {
                inside = line == format!("[{group}]");
            } else if inside {
                if let Some(v) = line.strip_prefix(&format!("{key}=")) {
                    let mut out = String::new();
                    let mut chars = v.chars();
                    while let Some(c) = chars.next() {
                        if c != '\\' {
                            out.push(c);
                            continue;
                        }
                        match chars.next() {
                            Some('s') => out.push(' '),
                            Some('n') => out.push('\n'),
                            Some('t') => out.push('\t'),
                            Some('r') => out.push('\r'),
                            Some('\\') => out.push('\\'),
                            other => panic!("invalid escape \\{other:?} in {key}={v}"),
                        }
                    }
                    return out;
                }
            }
        }
        panic!("no {key} in [{group}]:\n{contents}")
    }

    /// The Desktop Entry spec's Exec rules: quoted arguments (with `\"`, `` \` ``, `\$` and `\\` inside), `%%`, and
    /// `%F` as the selected files.
    fn exec_argv(exec: &str, files: &[&Path]) -> Vec<String> {
        let mut argv = Vec::new();
        let mut chars = exec.chars().peekable();
        loop {
            while chars.peek() == Some(&' ') {
                chars.next();
            }
            let Some(&first) = chars.peek() else { break };
            if first == '"' {
                chars.next();
                let mut arg = String::new();
                loop {
                    match chars.next().expect("unterminated quote") {
                        '"' => break,
                        '\\' => {
                            let c = chars.next().expect("dangling backslash");
                            assert!("\"`$\\".contains(c), "invalid quoted escape \\{c} in {exec}");
                            arg.push(c);
                        }
                        c => {
                            assert!(!"`$\\".contains(c), "{c} must be escaped inside quotes: {exec}");
                            arg.push(c);
                        }
                    }
                }
                argv.push(arg.replace("%%", "%"));
            } else {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ' ' {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                if word == "%F" {
                    argv.extend(files.iter().map(|f| f.to_string_lossy().into_owned()));
                } else {
                    assert!(!word.contains('%'), "unknown field code in {exec}");
                    argv.push(word);
                }
            }
        }
        argv
    }

    fn run_argv(argv: &[String], out: &Path) {
        let status = Command::new(&argv[0]).args(&argv[1..]).env(RECORD, out).status().unwrap();
        assert!(status.success());
    }

    fn xml_text(s: &str) -> String {
        s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
    }

    fn sh_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    pub fn run(root: &Path) {
        let exe = recorder(&root.join("Arcade o'k $x `y` \\z"), "arcade helper");
        let out = root.join("argv");
        let files_dir = root.join("files");
        std::fs::create_dir_all(&files_dir).unwrap();
        let a = files_dir.join("a b.png");
        let b = files_dir.join("it's $HOME \"q\".png");
        for f in [&a, &b] {
            std::fs::write(f, b"png").unwrap();
        }
        let files = [a.as_path(), b.as_path()];
        let home = Home::under(&root.join("home"));
        // The user's own Thunar action must survive both writing and removing.
        let uca = home.config.join("Thunar/uca.xml");
        std::fs::create_dir_all(uca.parent().unwrap()).unwrap();
        let mine = "<action>\n\t<icon>x</icon>\n\t<name>Mine</name>\n\t<unique-id>1234-1</unique-id>\n\t<command>echo %f</command>\n\t<description></description>\n\t<patterns>*</patterns>\n\t<other-files/>\n</action>\n";
        std::fs::write(&uca, format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n{mine}</actions>\n")).unwrap();

        let entries = entries();
        let plan = integrations::plan_for_this_os(&exe, &entries, &home);
        let state = root.join("state.json");
        integrations::apply(&plan, &state).unwrap();
        let (validate, gio, xmllint) = (tool("desktop-file-validate"), tool("gio"), tool("xmllint"));
        let mut checked = 0;
        for (i, entry) in entries.iter().enumerate() {
            let want = expected(entry, &files);
            let id = if entry.is_picker() { "dev.arcade.helper".to_string() } else { format!("dev.arcade.helper.pin{i}") };

            // "Open With": validated, then launched by GLib itself.
            let open_with = home.data.join(format!("applications/{id}.desktop"));
            if let Some(v) = &validate {
                let r = Command::new(v).arg(&open_with).output().unwrap();
                assert!(r.status.success() && r.stdout.is_empty(), "{}: {}", open_with.display(), String::from_utf8_lossy(&r.stdout));
            }
            if let Some(g) = &gio {
                let r = Command::new(g).arg("launch").arg(&open_with).args(files).env(RECORD, &out).output().unwrap();
                assert!(r.status.success(), "gio launch {}: {}", open_with.display(), String::from_utf8_lossy(&r.stderr));
                assert_eq!(recorded(&out), want, "Open With via GLib");
                checked += 1;
            }
            let contents = std::fs::read_to_string(&open_with).unwrap();
            run_argv(&exec_argv(&value(&contents, "Desktop Entry", "Exec"), &files), &out);
            assert_eq!(recorded(&out), want, "Open With by the Exec rules");

            // Nautilus and Caja run the script with the selected paths.
            let title = entry.title.replace('/', "-");
            for script in [home.data.join("nautilus/scripts/Arcade").join(&title), home.config.join("caja/scripts/Arcade").join(&title)] {
                assert_eq!(std::fs::metadata(&script).unwrap().permissions().mode() & 0o111, 0o111, "{}", script.display());
                let status = Command::new(&script).args(files).env(RECORD, &out).status().unwrap();
                assert!(status.success());
                assert_eq!(recorded(&out), want, "{}", script.display());
                checked += 1;
            }

            // Dolphin: executable (Plasma 6), and its action's Exec.
            let dolphin = home.data.join(format!("kio/servicemenus/{id}.desktop"));
            assert_eq!(std::fs::metadata(&dolphin).unwrap().permissions().mode() & 0o111, 0o111, "Plasma 6 needs it executable");
            let contents = std::fs::read_to_string(&dolphin).unwrap();
            assert_eq!(value(&contents, "Desktop Action arcade", "Name"), entry.title);
            run_argv(&exec_argv(&value(&contents, "Desktop Action arcade", "Exec"), &files), &out);
            assert_eq!(recorded(&out), want, "Dolphin");

            // Nemo.
            let nemo = home.data.join(format!("nemo/actions/{id}.nemo_action"));
            let contents = std::fs::read_to_string(&nemo).unwrap();
            assert_eq!(value(&contents, "Nemo Action", "Name"), format!("Arcade: {}", entry.title));
            run_argv(&exec_argv(&value(&contents, "Nemo Action", "Exec"), &files), &out);
            assert_eq!(recorded(&out), want, "Nemo");
            checked += 2;
        }

        // Thunar: valid XML, the user's action kept, and each command run by sh with %F as Thunar fills it in.
        let xml = std::fs::read_to_string(&uca).unwrap();
        if let Some(x) = &xmllint {
            let r = Command::new(x).arg("--noout").arg(&uca).output().unwrap();
            assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        }
        assert!(xml.contains(mine.trim_end()), "the user's own action is kept:\n{xml}");
        for (i, entry) in entries.iter().enumerate() {
            let block = xml.split("<action>").find(|b| b.contains(&format!("<unique-id>arcade-helper-{i}</unique-id>"))).unwrap_or_else(|| panic!("{xml}"));
            assert!(block.contains(&format!("<name>{}</name>", entry.title.replace('"', "&quot;"))), "{block}");
            let command = xml_text(&block[block.find("<command>").unwrap() + 9..block.find("</command>").unwrap()]);
            let list = files.iter().map(|f| sh_quote(&f.to_string_lossy())).collect::<Vec<_>>().join(" ");
            let status = Command::new("sh").arg("-c").arg(command.replace("%F", &list)).env(RECORD, &out).status().unwrap();
            assert!(status.success());
            assert_eq!(recorded(&out), expected(entry, &files), "Thunar");
            checked += 1;
        }

        // Removing takes every entry away and keeps the user's Thunar action.
        integrations::remove_all(&state).unwrap();
        for f in &plan.files {
            assert!(!f.path.exists(), "left behind: {}", f.path.display());
        }
        let xml = std::fs::read_to_string(&uca).unwrap();
        assert!(!xml.contains("arcade-helper-") && xml.contains(mine.trim_end()), "{xml}");
        eprintln!("menus: {checked} launches checked on Linux");
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::os::windows::process::CommandExt as _;
    use std::process::Command;

    fn reg_default(key: &str) -> Option<String> {
        let r = Command::new("reg").args(["query", &format!(r"HKCU\{key}"), "/ve"]).output().unwrap();
        if !r.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&r.stdout).into_owned();
        text.lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_string()))
    }

    fn reg_value(key: &str, name: &str) -> Option<String> {
        let r = Command::new("reg").args(["query", &format!(r"HKCU\{key}"), "/v", name]).output().unwrap();
        let text = String::from_utf8_lossy(&r.stdout).into_owned();
        r.status.success().then(|| text.lines().find_map(|l| l.split_once("REG_SZ").map(|(_, v)| v.trim().to_string()))).flatten()
    }

    pub fn run(root: &Path) {
        if std::env::var_os("CI").is_none() {
            eprintln!("menus: the Windows check writes the real HKCU, so it runs only in CI");
            return;
        }
        let exe = recorder(&root.join("Arcade o'k (x) & y"), "arcade helper.exe");
        let out = root.join("argv");
        let file = root.join("files").join("a b & c's.png");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"png").unwrap();
        let home = Home::under(&root.join("home"));
        let entries = entries();
        let plan = integrations::plan_for_this_os(&exe, &entries, &home);
        let state = root.join("state.json");
        integrations::apply(&plan, &state).unwrap();
        let mut checked = 0;
        for k in plan.registry.iter().filter(|k| k.path.ends_with(r"\command")) {
            let line = reg_default(&k.path).unwrap_or_else(|| panic!("{} wasn't written", k.path));
            // Explorer puts the file's path where %1 is and starts the program once per file.
            let line = line.replace("%1", &file.to_string_lossy());
            let quoted_exe = format!("\"{}\"", exe.to_string_lossy());
            let rest = line.strip_prefix(&quoted_exe).unwrap_or_else(|| panic!("{line}"));
            let status = Command::new(&exe).raw_arg(rest.trim_start()).env(RECORD, &out).status().unwrap();
            assert!(status.success());
            let verb = k.path.trim_end_matches(r"\command");
            let title = reg_value(verb, "MUIVerb").unwrap();
            let entry = entries.iter().find(|e| e.title == title).unwrap_or_else(|| panic!("{title}"));
            assert_eq!(recorded(&out), expected(entry, &[&file]), "{}", k.path);
            if !entry.is_picker() && verb.starts_with(integrations::windows::FILES_ROOT) {
                let filter = reg_value(verb, "AppliesTo").unwrap();
                assert!(filter.contains("System.FileExtension:=.png"), "{filter}");
            }
            checked += 1;
        }
        assert!(checked >= 3, "picker and pin on files, picker on folders: {checked}");
        integrations::remove_all(&state).unwrap();
        for r in [integrations::windows::FILES_ROOT, integrations::windows::FOLDERS_ROOT] {
            let status = Command::new("reg").args(["query", &format!(r"HKCU\{r}")]).output().unwrap().status;
            assert!(!status.success(), "{r} left behind");
        }
        eprintln!("menus: {checked} verbs checked on Windows");
    }
}
