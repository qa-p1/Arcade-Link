//! `arcade-link conformance <repo-dir | AppImage> [--binary PATH] [--json]`:
//! checks an app against `ARCADE_APP.md` and prints one line per rule ID.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    Skip,
}

struct Report(Vec<(String, Status, String)>);

impl Report {
    fn add(&mut self, id: &str, status: Status, detail: impl Into<String>) {
        self.0.push((id.into(), status, detail.into()));
    }
    fn check(&mut self, id: &str, ok: bool, pass: impl Into<String>, fail: impl Into<String>) {
        if ok {
            self.add(id, Status::Pass, pass)
        } else {
            self.add(id, Status::Fail, fail)
        }
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mut target = None;
    let mut binary = None;
    let mut json_out = false;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--binary" => binary = Some(PathBuf::from(it.next().ok_or("--binary needs a path")?)),
            "--json" => json_out = true,
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s => target = Some(PathBuf::from(s)),
        }
    }
    let target = target.ok_or("conformance needs an app repository or AppImage")?;
    let mut report = Report(Vec::new());
    let scratch = tempdir("conformance")?;

    let (tree, binary) = if target.is_file() {
        let extracted = extract_appimage(&target, &scratch).map_err(|e| format!("{}: {e}", target.display()))?;
        (extracted, Some(binary.unwrap_or(target.clone())))
    } else {
        (target.clone(), binary)
    };
    let package = target.is_file();
    static_checks(&tree, package, &mut report);
    match &binary {
        Some(b) => runtime_checks(b, &scratch, &mut report),
        None => {
            for id in ["CLI-1", "CLI-2", "CLI-3", "CLI-4", "FS-1"] {
                report.add(id, Status::Skip, "no --binary given");
            }
        }
    }
    for id in ["RT-1", "RT-2", "RT-3", "RT-4", "RT-5", "RT-6", "UI-1", "UI-2", "UI-3", "PERF-1", "PERF-2"] {
        report.add(id, Status::Skip, "desktop harness check (desktop-e2e.yml)");
    }
    let _ = std::fs::remove_dir_all(&scratch);

    let failed = report.0.iter().filter(|(_, s, _)| *s == Status::Fail).count();
    if json_out {
        let checks: Vec<Value> = report.0.iter().map(|(id, s, d)| json!({"id": id, "status": format!("{s:?}").to_lowercase(), "detail": d})).collect();
        println!("{}", json!({"target": target.display().to_string(), "failed": failed, "checks": checks}));
    } else {
        for (id, s, d) in &report.0 {
            println!("{:<5} {id:<7} {d}", format!("{s:?}").to_uppercase());
        }
        println!("{} checks, {failed} failed", report.0.len());
    }
    if failed > 0 {
        return Err(format!("{failed} conformance check(s) failed"));
    }
    Ok(())
}

fn tempdir(name: &str) -> Result<PathBuf, String> {
    let d = std::env::temp_dir().join(format!("arcade-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

fn extract_appimage(appimage: &Path, scratch: &Path) -> std::io::Result<PathBuf> {
    let out = Command::new(appimage).arg("--appimage-extract").current_dir(scratch).stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    let root = scratch.join("squashfs-root");
    if !out.success() || !root.is_dir() {
        return Err(std::io::Error::other("--appimage-extract failed"));
    }
    Ok(root)
}

// ---------------------------------------------------------------------------
// Static checks

const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", "build", "vendor", ".dart_tool", "dist", "fuzz"];

fn walk(root: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        match e.file_type() {
            Ok(t) if t.is_dir() && depth < 8 && !SKIP_DIRS.contains(&name.as_str()) => walk(&p, out, depth + 1),
            Ok(t) if t.is_file() || t.is_symlink() => out.push(p),
            _ => {}
        }
    }
}

fn name_of(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn static_checks(root: &Path, package: bool, r: &mut Report) {
    let mut files = Vec::new();
    walk(root, &mut files, 0);
    let names: Vec<String> = files.iter().map(|p| name_of(p)).collect();
    let text_of = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();

    // ID-1 / ID-2: one desktop identity, spelled one way.
    let desktop: BTreeSet<String> =
        names.iter().filter(|n| n.starts_with("dev.arcade.") && n.ends_with(".desktop")).map(|n| n.trim_end_matches(".desktop").to_string()).collect();
    let well_formed = |id: &str| {
        let name = id.trim_start_matches("dev.arcade.");
        name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) && name.chars().all(|c| c.is_ascii_alphanumeric())
    };
    let other_desktop: Vec<&String> = names.iter().filter(|n| n.ends_with(".desktop") && !n.starts_with("dev.arcade.")).collect();
    match desktop.len() {
        0 => r.add("ID-1", Status::Fail, format!("no dev.arcade.<Name>.desktop entry found (found {other_desktop:?})")),
        1 => {
            let id = desktop.iter().next().expect("one");
            r.check("ID-1", well_formed(id), format!("desktop id {id}"), format!("desktop id {id} is not dev.arcade.<Name>"));
        }
        _ => r.add("ID-1", Status::Fail, format!("several desktop ids: {desktop:?}")),
    }
    let autostart = files.iter().filter(|p| p.to_string_lossy().contains("autostart") && name_of(p).ends_with(".desktop")).count();
    r.check(
        "ID-2",
        desktop.len() == 1 && autostart <= 1,
        "one launcher entry, at most one autostart entry",
        format!("{} launcher ids, {autostart} autostart entries", desktop.len()),
    );

    // ID-3: an app icon and a symbolic tray icon.
    let icon = names.iter().any(|n| n.starts_with("dev.arcade.") && (n.ends_with(".svg") || n.ends_with(".png")));
    // Packages may compile the tray icon into the binary; repositories must carry it.
    let symbolic = package || names.iter().any(|n| n.contains("symbolic") && n.ends_with(".svg"));
    r.check("ID-3", icon && symbolic, "app icon and symbolic tray icon", format!("app icon dev.arcade.<Name>.svg/png: {icon}, symbolic tray icon: {symbolic}"));

    // REPO-1: licence, notices, security policy, changelog.
    let has = |pred: &dyn Fn(&str) -> bool| names.iter().any(|n| pred(n));
    let licence = has(&|n| n.starts_with("LICENSE") || n.starts_with("COPYING"));
    let notices = has(&|n| n.starts_with("THIRD_PARTY_NOTICES"));
    let mut missing: Vec<&str> = Vec::new();
    if !licence {
        missing.push("LICENSE");
    }
    if !notices {
        missing.push("THIRD_PARTY_NOTICES");
    }
    if !package {
        if !root.join("SECURITY.md").is_file() {
            missing.push("SECURITY.md");
        }
        let changelog = text_of(&root.join("CHANGELOG.md"));
        if !changelog.to_lowercase().contains("keep a changelog") {
            missing.push("CHANGELOG.md (Keep a Changelog)");
        }
    }
    r.check("REPO-1", missing.is_empty(), "licence, notices, security policy and changelog present", format!("missing: {}", missing.join(", ")));

    if package {
        for id in ["REPO-2", "REL-1", "REL-2", "LINK-2"] {
            r.add(id, Status::Skip, "repository check; run against the repo");
        }
        return;
    }

    // REPO-2: dependency audit, licence check and update bot.
    let workflows: Vec<PathBuf> = std::fs::read_dir(root.join(".github/workflows"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yml" || e == "yaml"))
        .collect();
    let all_workflows: String = workflows.iter().map(|p| text_of(p)).collect::<Vec<_>>().join("\n");
    let audit = ["cargo deny", "cargo-deny", "cargo audit", "audit-check", "dependency-review", "pub outdated", "osv-scanner"]
        .iter()
        .any(|k| all_workflows.contains(k));
    let bot = root.join(".github/dependabot.yml").is_file() || root.join("renovate.json").is_file() || root.join(".github/renovate.json").is_file();
    r.check("REPO-2", audit && bot, "dependency audit in CI and an update bot", format!("audit in CI: {audit}, Dependabot/Renovate: {bot}"));

    // REL-1: the four workflows, built on Link's reusable ones; network steps bounded.
    let wanted = ["ci.yml", "release.yml", "promote.yml", "nightly.yml"];
    let missing_wf: Vec<&str> = wanted.iter().copied().filter(|w| !root.join(".github/workflows").join(w).is_file()).collect();
    let reuse = all_workflows.contains("Arcade-Link/.github/workflows/") || all_workflows.contains("Arcade-link/.github/workflows/");
    let unbounded = unbounded_network_steps(&workflows);
    r.check(
        "REL-1",
        missing_wf.is_empty() && reuse && unbounded.is_empty(),
        "ci/release/promote/nightly on Link's reusable workflows, network steps bounded",
        format!("missing: {missing_wf:?}; uses Link workflows: {reuse}; network steps without timeout: {unbounded:?}"),
    );

    // REL-2: signed manifests next to the 0.4-compatible arcade-release.json.
    let manifest = all_workflows.contains("release-manifest") || all_workflows.contains("Arcade-Link/.github/workflows/app-publish.yml");
    let legacy = all_workflows.contains("arcade-release") || all_workflows.contains("Arcade-Link/.github/workflows/app-publish.yml");
    r.check(
        "REL-2",
        manifest && legacy,
        "release-manifest and arcade-release.json published",
        format!("release-manifest: {manifest}, arcade-release.json: {legacy}"),
    );

    // LINK-2: Link pinned by a v1 tag.
    let mut pins = Vec::new();
    for p in files.iter().filter(|p| matches!(name_of(p).as_str(), "Cargo.toml" | "CMakeLists.txt" | "pubspec.yaml" | "VENDORED" | "package.json")) {
        for line in text_of(p).lines().filter(|l| l.contains("Arcade-Link") || l.contains("Arcade-link")) {
            pins.push(line.trim().to_string());
        }
    }
    let bad: Vec<&String> =
        pins.iter().filter(|l| !(l.contains("tag") && l.contains("\"v1.")) && (l.contains("rev") || l.contains("branch") || l.contains("tag"))).collect();
    if pins.is_empty() {
        r.add("LINK-2", Status::Skip, "no Arcade Link dependency line found (vendored copy?)");
    } else {
        r.check("LINK-2", bad.is_empty(), "Arcade Link pinned by a v1 tag", format!("not pinned to a v1 tag: {bad:?}"));
    }
}

/// Steps that run `apt-get`, `curl`, `wget`, `pip install` or `npm ci` without a
/// `timeout-minutes` on the step or its job.
fn unbounded_network_steps(workflows: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for wf in workflows {
        let text = std::fs::read_to_string(wf).unwrap_or_default();
        let mut job_timeout = false;
        let mut step = String::new();
        let mut step_timeout = false;
        let flush = |step: &str, step_timeout: bool, job_timeout: bool, out: &mut Vec<String>| {
            let network = ["apt-get", "curl ", "wget ", "pip install", "npm ci", "npm install"].iter().any(|k| step.contains(k));
            if network && !step_timeout && !job_timeout {
                let name = step.lines().find_map(|l| l.trim().strip_prefix("- name:").or_else(|| l.trim().strip_prefix("name:"))).unwrap_or("step");
                out.push(format!("{}: {}", name_of(wf), name.trim()));
            }
        };
        for line in text.lines() {
            let indent = line.len() - line.trim_start().len();
            let t = line.trim();
            if indent == 2 && t.ends_with(':') && !t.starts_with('-') {
                flush(&step, step_timeout, job_timeout, &mut out);
                step.clear();
                job_timeout = false;
                step_timeout = false;
            } else if indent == 4 && t.starts_with("timeout-minutes") {
                job_timeout = true;
            } else if t.starts_with("- ") && indent >= 6 {
                flush(&step, step_timeout, job_timeout, &mut out);
                step.clear();
                step_timeout = false;
            }
            if t.starts_with("timeout-minutes") && indent > 4 {
                step_timeout = true;
            }
            step.push_str(line);
            step.push('\n');
        }
        flush(&step, step_timeout, job_timeout, &mut out);
    }
    out
}

// ---------------------------------------------------------------------------
// Runtime checks: the binary in an isolated HOME, without a display.

struct Isolated {
    home: PathBuf,
}

impl Isolated {
    fn new(scratch: &Path) -> Self {
        let home = scratch.join("home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("scratch home");
        Isolated { home }
    }

    fn run(&self, binary: &Path, args: &[&str]) -> Result<(i32, String, String), String> {
        let h = &self.home;
        let mut c = Command::new(binary);
        c.args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", h)
            .env("XDG_CONFIG_HOME", h.join(".config"))
            .env("XDG_DATA_HOME", h.join(".local/share"))
            .env("XDG_CACHE_HOME", h.join(".cache"))
            .env("XDG_STATE_HOME", h.join(".local/state"))
            .env("XDG_RUNTIME_DIR", h.join("run"))
            .env("ARCADE_HOME", h.join("arcade-link"))
            .env("APPIMAGE_EXTRACT_AND_RUN", "1")
            .env("QT_QPA_PLATFORM", "offscreen")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            c.process_group(0);
        }
        let _ = std::fs::create_dir_all(h.join("run"));
        let mut child = c.spawn().map_err(|e| format!("{}: {e}", binary.display()))?;
        let group = child.id();
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            if Instant::now() > end {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{} {} did not exit within 20 s", binary.display(), args.join(" ")));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        #[cfg(unix)]
        // SAFETY: the group is the one this command was started in (its
        // leader has exited); only processes it left behind are signalled.
        unsafe {
            libc::killpg(group as libc::pid_t, libc::SIGKILL);
        }
        #[cfg(not(unix))]
        let _ = group;
        Ok((out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned()))
    }
}

fn runtime_checks(binary: &Path, scratch: &Path, r: &mut Report) {
    let env = Isolated::new(scratch);

    match env.run(binary, &["--status"]) {
        Ok((code, out, err)) => {
            let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
            let parsed = lines.first().and_then(|l| serde_json::from_str::<Value>(l).ok());
            // 3 is the LSB status code for "not running".
            let ok = (code == 0 || code == 3) && lines.len() == 1 && parsed.as_ref().is_some_and(Value::is_object);
            r.check("CLI-1", ok, "--status prints one line of JSON", format!("exit {code}, {} stdout lines, stderr: {}", lines.len(), first_line(&err)));
        }
        Err(e) => r.add("CLI-1", Status::Fail, e),
    }

    let unknown = env.run(binary, &["--arcade-conformance-unknown-flag"]);
    let help = env.run(binary, &["--help"]);
    match (unknown, help) {
        (Ok((uc, _, uerr)), Ok((hc, hout, _))) => r.check(
            "CLI-2",
            uc == 2 && !uerr.trim().is_empty() && hc == 0 && !hout.trim().is_empty(),
            "unknown flag exits 2 with usage; --help exits 0",
            format!("unknown flag: exit {uc}, stderr {:?}; --help: exit {hc}", first_line(&uerr)),
        ),
        (Err(e), _) | (_, Err(e)) => r.add("CLI-2", Status::Fail, e),
    }

    match env.run(binary, &["--version"]) {
        Ok((code, out, _)) => {
            let line = out.trim();
            let semver = line.rsplit(' ').next().is_some_and(|v| {
                let core = v.trim_start_matches('v').split(['-', '+']).next().unwrap_or("");
                core.split('.').count() == 3 && core.split('.').all(|p| p.parse::<u64>().is_ok())
            });
            r.check("CLI-3", code == 0 && line.contains(' ') && semver, format!("--version: {line}"), format!("exit {code}, output {line:?}"));
        }
        Err(e) => r.add("CLI-3", Status::Fail, e),
    }

    match env.run(binary, &["--arcade-manifest"]) {
        Ok((code, out, err)) => match arcade_link::Manifest::from_json(out.trim()) {
            Ok(m) => r.check("CLI-4", code == 0, format!("manifest for {} {}", m.id, m.version), format!("exit {code}")),
            Err(e) => r.add("CLI-4", Status::Fail, format!("exit {code}: {e}; stderr: {}", first_line(&err))),
        },
        Err(e) => r.add("CLI-4", Status::Fail, e),
    }

    // FS-1: everything the commands above wrote is under arcade/.
    let mut written = Vec::new();
    walk(&env.home, &mut written, 0);
    let stray: Vec<String> =
        written.iter().filter_map(|p| p.strip_prefix(&env.home).ok()).map(|p| p.to_string_lossy().into_owned()).filter(|p| !is_allowed_path(p)).collect();
    r.check("FS-1", stray.is_empty(), "files only under arcade/ folders", format!("written outside arcade/: {stray:?}"));
}

fn is_allowed_path(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let under = |base: &[&str]| parts.len() > base.len() && parts[..base.len()] == *base && parts[base.len()] == "arcade";
    under(&[".config"])
        || under(&[".local", "share"])
        || under(&[".cache"])
        || under(&[".local", "state"])
        || parts.first() == Some(&"arcade-link")
        || parts.first() == Some(&"run")
        // Toolkit caches the app does not control.
        || matches!(parts.as_slice(), [".cache", "fontconfig", ..] | [".cache", "mesa_shader_cache", ..] | [".cache", "mesa_shader_cache_db", ..])
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(160).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_arcade_folders_and_toolkit_caches_are_allowed() {
        assert!(is_allowed_path(".config/arcade/look/settings.json"));
        assert!(is_allowed_path(".local/share/arcade/runtime/x"));
        assert!(is_allowed_path(".cache/fontconfig/abc"));
        assert!(!is_allowed_path(".config/arcade-look/settings.json"));
        assert!(!is_allowed_path(".local/share/dev.arcade.Look/x"));
        assert!(!is_allowed_path(".config/autostart/dev.arcade.Look.desktop"));
    }

    #[test]
    fn network_steps_need_a_timeout() {
        let d = tempdir("wf-test").unwrap();
        let wf = d.join("ci.yml");
        std::fs::write(
            &wf,
            "jobs:\n  a:\n    runs-on: x\n    steps:\n      - name: deps\n        run: sudo apt-get install -y x\n      - name: ok\n        timeout-minutes: 5\n        run: curl -fsSL x\n  b:\n    timeout-minutes: 10\n    steps:\n      - name: fine\n        run: apt-get update\n",
        )
        .unwrap();
        assert_eq!(unbounded_network_steps(&[wf]), vec!["ci.yml: deps".to_string()]);
    }
}
