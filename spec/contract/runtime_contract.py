#!/usr/bin/env python3
"""The runtime contract (ARCADE_APP.md RT-1…RT-4) checked against any built app,
whatever it is written in. Run inside a desktop session (tools/desktop/tier2.sh
x11, or session.sh for Hyprland):

    runtime_contract.py <AppImage-or-binary> [--background ARG] [--json]

Each check prints PASS/FAIL with its rule ID; exit 1 if any fails. Static and
no-display rules are `arcade-link conformance`'s job.
"""
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

app = sys.argv[1]
bg = sys.argv[sys.argv.index("--background") + 1] if "--background" in sys.argv else "--background"
home = Path(tempfile.mkdtemp(prefix="contract-"))
env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": f"{home}/.config", "XDG_DATA_HOME": f"{home}/.local/share",
       "XDG_CACHE_HOME": f"{home}/.cache", "XDG_STATE_HOME": f"{home}/.local/state", "ARCADE_HOME": f"{home}/arcade-link"}
results = []


def check(rule, ok, detail):
    results.append({"id": rule, "status": "pass" if ok else "fail", "detail": detail})
    print(f"{'PASS' if ok else 'FAIL'}  {rule:<5} {detail}", flush=True)


def status():
    r = subprocess.run([app, "--status"], env=env, capture_output=True, text=True, timeout=30)
    try:
        return json.loads(r.stdout.strip().splitlines()[0])
    except (json.JSONDecodeError, IndexError):
        return {}


def wait(pred, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(0.2)
    return None


def pid_of():
    """The resident's PID: from --status, or the oldest process started from the app."""
    s = status()
    if not s.get("running"):
        return None
    if s.get("pid"):
        return s["pid"]
    r = subprocess.run(["pgrep", "-o", "-f", Path(app).name], capture_output=True, text=True)
    return int(r.stdout.split()[0]) if r.stdout.strip() else None


def zombies_of(pid):
    out = []
    for d in Path("/proc").iterdir():
        if d.name.isdigit():
            try:
                stat = (d / "stat").read_text()
            except OSError:
                continue
            fields = stat[stat.rindex(")") + 2:].split()
            if fields[0] == "Z" and int(fields[1]) == pid:
                out.append(int(d.name))
    return out


def crash_reports():
    return sorted(p for p in home.rglob("crashes/*") if p.is_file())


subprocess.run([app, "--install", "--silent", "--no-launch"], env=env, timeout=120, capture_output=True)
first = subprocess.Popen([app, bg], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
pid = wait(pid_of, 30)
check("RT-1", pid is not None, f"resident instance pid {pid}" if pid else "--status never reported running")

if pid:
    t = time.time()
    second = subprocess.run([app, bg], env=env, capture_output=True, timeout=30)
    took = time.time() - t
    check("RT-1", second.returncode == 0 and took < 10 and pid_of() == pid,
          f"second launch exit {second.returncode} after {took:.1f} s; resident pid {pid_of()} (was {pid})")

    restart = subprocess.run([app, "--restart"], env=env, capture_output=True, timeout=30)
    new = wait(lambda: (p := pid_of()) and p != pid and p, 30)
    alive_later = new and (time.sleep(5) or pid_of() == new)
    check("RT-2", bool(new and alive_later), f"--restart exit {restart.returncode}: successor pid {new}, alive 5 s later: {bool(alive_later)}")
    pid = new or pid_of() or pid

    time.sleep(10)
    z = zombies_of(pid)
    check("RT-3", not z, f"zombie children of {pid}: {z}" if z else f"no zombie children of {pid}")

    before = crash_reports()
    os.kill(pid, signal.SIGSEGV)
    time.sleep(3)
    after = crash_reports()
    check("RT-4", len(after) > len(before), f"crash reports after SIGSEGV: {[str(p.relative_to(home)) for p in after]}")

subprocess.run([app, "--quit"], env=env, capture_output=True, timeout=30)
try:
    first.wait(10)
except subprocess.TimeoutExpired:
    first.kill()
if "--json" in sys.argv:
    print(json.dumps(results))
sys.exit(1 if any(r["status"] == "fail" for r in results) else 0)
