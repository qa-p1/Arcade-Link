"""Link v0.3 scaffolds. Every process and filesystem mutation is isolated.

App uptake and Tools' complete hub UI belong to subsequent fleet e2e groups.
Here watchers are the crate-backed CLI, not assertions about app tray widgets.
"""
import contextlib
import json
import os
from pathlib import Path
import subprocess
import time
from e2e_support import LinesProcess
from validate_shortcuts import validate


@check("trayhost")
def seven_watchers_hand_back_after_crash_and_restart_without_flash(s):
    env = dict(s.env, ARCADE_HOME=str(s.root / "tray-probe"))
    with contextlib.ExitStack() as owned:
        host = owned.enter_context(LinesProcess(CLI, ["tray", "host"], env))
        host.expect("ready")
        watchers = {app: owned.enter_context(LinesProcess(CLI, ["tray", app], env)) for app in APP_IDS}
        for watcher in watchers.values():
            watcher.expect("hosted")
        host.command("exclude find")
        watchers["arcade.find"].expect("own")
        host.command("include find")
        watchers["arcade.find"].expect("hosted")
        for watcher in watchers.values():
            watcher.send("link off")
            watcher.expect("own")
            watcher.send("link on")
            watcher.expect("hosted")
        started = time.perf_counter()
        host.kill()
        latencies = {app: round((watcher.expect("own") - started) * 1000, 2) for app, watcher in watchers.items()}
        assert max(latencies.values()) < 1000, latencies
        host = owned.enter_context(LinesProcess(CLI, ["tray", "host"], env))
        host.expect("ready")
        for watcher in watchers.values():
            watcher.expect("hosted")
        host.command("restarting")
        host.kill()
        for watcher in watchers.values():
            watcher.silent(.025)
        host = owned.enter_context(LinesProcess(CLI, ["tray", "host"], env))
        host.expect("ready")
        for watcher in watchers.values():
            watcher.silent(.025)
        # Starting with Link off must never suppress the own icon.
        off = owned.enter_context(LinesProcess(CLI, ["tray", "find", "--link-off"], env))
        off.expect("own")
        off.send("link on")
        off.expect("hosted")
        host.send("quit")
        for watcher in watchers.values():
            watcher.expect("own")
        off.expect("own")
    return f"all seven Rust watchers; SIGKILL hand-back ms: {latencies}; restart/exclusion/link-off/clean quit passed"


@check("install")
def install_update_repair_and_uninstall_use_only_a_temporary_home(s):
    root = s.root / "install-probe"
    root.mkdir()
    source = root / "download.AppImage"
    source.write_bytes(b"fake AppImage v1")
    (root / "mount/usr/bin").mkdir(parents=True)
    (root / "mount/usr/bin/find").write_bytes(b"mounted executable")
    (root / "icon.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 2h12v12H2z"/></svg>')
    harness = LINK / "target/debug/examples/install-harness"
    def call(*args):
        r = subprocess.run([str(harness), str(root), *args], env=s.env, capture_output=True, text=True, timeout=15)
        assert r.returncode == 0, r.stderr
        return json.loads(r.stdout)
    detection = call("detect", "--dev")
    assert detection["detection"]["state"]["kind"] == "devBuild" and not detection["prompt"], detection
    assert not (root / "installs/arcade.find.json").exists()
    receipt = call("install", "1")
    executable = Path(receipt["path"])
    assert executable.read_bytes() == b"fake AppImage v1"
    assert os.stat(root / "installs/arcade.find.json").st_mode & 0o777 == 0o600
    source.write_bytes(b"fake AppImage v2")
    receipt = call("install", "2")
    previous = Path(receipt["previous"]["path"])
    assert previous.read_bytes() == b"fake AppImage v1"
    moved = executable.with_name("Moved-Find.AppImage")
    executable.rename(moved)
    Path(receipt["integration"]["desktopEntry"]).unlink()
    receipt = call("repair", str(moved))
    assert Path(receipt["integration"]["cli"]).resolve() == moved
    status = call("status")
    assert not status["binOnPath"] and status["pathHint"].startswith("fish_add_path "), status
    assert call("uninstall")["removed"]
    assert not moved.exists() and not previous.exists()
    for key in ("desktopEntry", "cli", "autostart"):
        assert not Path(receipt["integration"][key]).exists()
    assert not (root / "installs/arcade.find.json").exists()
    return "temp home install/update(previous retained)/moved-path repair/uninstall/dev suppression passed"


@check("shortcuts")
def sibling_app_docs_and_catalog_sheets_validate(s):
    files = []
    missing = []
    for app, spec in APPS.items():
        path = spec["dir"] / "docs/user/shortcuts.json"
        if path.is_file():
            files.append(path)
        else:
            missing.append(app)
    catalog = Path(os.environ.get("ARCADE_CATALOG_REPO", str(CODING / "Rust/Arcade-catalog")))
    # index.json is catalog metadata, not a third-party shortcut sheet.
    sheets = [path for path in sorted((catalog / "shortcuts").glob("*.json")) if path.name != "index.json"]
    for path in files + sheets:
        document = json.loads(path.read_text(encoding="utf-8"))
        try:
            validate(document)
        except ValueError as error:
            raise AssertionError(f"{path}: {error}") from error
        result = s.cli("shortcuts", "validate", str(path))
        assert result.returncode == 0, f"{path}: {result.stdout}{result.stderr}"
    assert files or sheets, "no sibling shortcut documents or catalog sheets found"
    return f"{len(files)} app docs + {len(sheets)} catalog sheets passed Rust/Python; missing app docs: {missing}"


@check("tools-hub")
def hub_contract_against_mock_peers(s):
    """What Arcade Tools relies on from every peer, without real apps: the live
    registry, advertised optional methods, rebinding a shortcut through
    app.shortcuts.set, and a clear answer for a method a peer lacks."""
    for app in APPS:
        s.start(app)
    listing = s.cli("ls", check=True).stdout
    assert listing.count("running") == len(APPS), listing
    for app in APPS:
        described = json.loads(s.cli("call", app, "describe", check=True).stdout)
        methods = described.get("methods")
        assert isinstance(methods, list) and "app.shortcuts.set" in methods, (app, methods)
    shortcuts = json.loads(s.cli("shortcuts", check=True).stdout)
    find = next(entry for entry in shortcuts if entry["app"] == "arcade.find")
    applied = json.loads(s.cli("call", "arcade.find", "app.shortcuts.set", "--params",
                               json.dumps({"id": find["id"], "accelerator": "alt+ctrl+k"}), check=True).stdout)
    assert applied.get("applied") is True, applied
    rebound = json.loads(s.cli("shortcuts", check=True).stdout)
    entry = next(e for e in rebound if e["app"] == "arcade.find" and e["id"] == find["id"])
    assert entry["canonical"] == "Ctrl+Alt+K", entry
    missing = s.cli("call", "arcade.find", "app.not-a-method")
    assert missing.returncode != 0 and "doesn't support this method" in missing.stderr + missing.stdout, missing
    return f"{len(APPS)} mock peers listed, advertised methods, rebound {find['id']} to Ctrl+Alt+K, unsupported reported"
