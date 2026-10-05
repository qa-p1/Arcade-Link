"""Failure injection (plan §13.4): crashes, cancel, busy, stale endpoints and
a broken registry must end cleanly — an error with the standard message,
never a hang. Names injected by tools/e2e.py: check, Session, APPS, CLI."""

import json
import signal
import subprocess
import time
from pathlib import Path

FIXTURES = Path(__file__).resolve().parents[2] / "fixtures"


def mock(s, app="arcade.box", fixture="box.json"):
    p = subprocess.Popen([str(CLI), "mock", "--as", app, "--actions", str(FIXTURES / fixture)], env=s.env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    deadline = time.time() + 10
    while time.time() < deadline:
        if s.cli("status", app).returncode == 0:
            return p
        time.sleep(0.05)
    p.kill()
    raise AssertionError(f"mock {app} did not start")


def stop(p):
    if p.poll() is None:
        p.kill()
        p.wait()


def inputs(s):
    d = s.root / "failure-inputs"
    d.mkdir(exist_ok=True)
    (d / "a.mp4").write_bytes(b"\0" * 1000)
    (d / "a.png").write_bytes(b"x")
    return d


@check("failure")
def peer_crash_mid_job_ends_with_not_running(s):
    p = mock(s)
    try:
        t0 = time.time()
        r = s.cli("invoke", "box", "box:arcade.media.inspect", "--file", str(inputs(s) / "a.png"), timeout=15)
        assert r.returncode != 0 and "not_running" in r.stderr, r.stderr
        assert "isn't running" in r.stderr, r.stderr  # the standard message
        assert time.time() - t0 < 5, "a crash must not wait for a timeout"
        return r.stderr.strip().splitlines()[-1]
    finally:
        stop(p)


@check("failure")
def cancel_ends_the_job(s):
    p = mock(s)
    try:
        c = subprocess.Popen([str(CLI), "invoke", "box", "box:arcade.video.compress#share-25mb", "--file",
                              str(inputs(s) / "a.mp4")], env=s.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(0.35)
        c.send_signal(signal.SIGINT)
        out, err = c.communicate(timeout=10)
        assert c.returncode != 0 and "cancelled" in err and "Cancelled." in err, err
        # The peer is still healthy afterwards.
        assert s.cli("status", "box").returncode == 0
        return err.strip().splitlines()[-1]
    finally:
        stop(p)


@check("failure")
def quit_is_refused_while_jobs_run_unless_forced(s):
    p = mock(s)
    try:
        job = subprocess.Popen([str(CLI), "invoke", "box", "box:arcade.video.compress#share-25mb", "--file",
                                str(inputs(s) / "a.mp4")], env=s.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(0.2)
        r = s.cli("quit", "box")
        assert r.returncode != 0 and "busy" in r.stderr, r.stderr + r.stdout
        r = s.cli("quit", "box", "--force", check=True)
        p.wait(timeout=5)
        job.wait(timeout=10)
        assert job.returncode != 0, "the job must not report success after a forced quit"
        return "busy, then forced quit"
    finally:
        stop(p)


@check("failure")
def broken_registry_entries_are_skipped(s):
    reg = s.root / "arcade/apps"
    reg.mkdir(parents=True, exist_ok=True)
    (reg / "arcade.bogus.json").write_text("{nope")
    try:
        p = mock(s, "arcade.lens", "lens.json")
        stop(p)
        # Point the manifest at an executable that no longer exists.
        m = json.loads((reg / "arcade.lens.json").read_text())
        m["executable"] = str(s.root / "missing/arcade-lens")
        (reg / "arcade.lens.json").write_text(json.dumps(m))
        r = s.cli("ls", "--json", check=True)
        rows = {row["id"]: row for row in json.loads(r.stdout)}
        assert "arcade.bogus" not in rows, rows.keys()
        # SPEC §6: a manifest whose executable is gone counts as not installed.
        assert rows.get("arcade.lens", {}).get("state", "not installed") == "not installed", rows["arcade.lens"]
        r = s.cli("describe", "lens")
        assert r.returncode != 0 and "not installed" in r.stderr, r.stderr + r.stdout
        return f"{len(rows)} apps listed; lens without its executable: {r.stderr.strip()}"
    finally:
        (reg / "arcade.bogus.json").unlink(missing_ok=True)
        (reg / "arcade.lens.json").unlink(missing_ok=True)


@check("failure")
def killed_apps_leave_no_live_endpoint_and_restart_cleanly(s):
    """SIGKILL each real app: it shows as not running (its endpoint is stale),
    and a restart takes the endpoint over."""
    done = []
    for app, spec in APPS.items():
        if not (spec["dir"] / spec["bin"]).exists():
            done.append(f"{app}: binary missing (not run)")
            continue
        s.start(app)
        s.kill(app)
        r = s.cli("status", app)
        assert r.returncode != 0 and "not_running" in r.stderr, f"{app}: {r.stderr}{r.stdout}"
        assert s.state(app) != "running", s.state(app)
        s.start(app)  # waits until it serves again
        s.kill(app, signal.SIGTERM)
        done.append(f"{app}: ok")
    return "; ".join(done)
