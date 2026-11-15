#!/usr/bin/env python3
"""N-1 gate (PLAN exit gate "the rc talks to the previous released versions of its peers over Link").

    python3 tools/e2e.py run -- python3 tools/gates/n1_app.py --app RC.AppImage --id arcade.box --cli OLD [--cli OLD2]

Runs inside the isolated e2e session (private HOME, XDG_*, ARCADE_HOME, D-Bus and Xvfb). Starts the published rc with
`--background`, then the Link CLI of each previous release, as the released peers ship it, must:
  - list the app as running (`ls`),
  - read its status over Link (`status <id>`: a live call, not the manifest),
  - describe its actions (`describe <id> --json`).
Ends with `--quit` and checks the app exits. Exits non-zero on the first failure.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time


def run(cmd: list[str], timeout: float = 30) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def until(check, what: str, seconds: float) -> None:
    deadline = time.monotonic() + seconds
    last = ''
    while time.monotonic() < deadline:
        ok, last = check()
        if ok:
            return
        time.sleep(0.25)
    raise SystemExit(f'n1: {what} (last output: {last.strip()[-400:]})')


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--app', required=True, help='the published rc AppImage')
    ap.add_argument('--id', required=True, help='the app id, e.g. arcade.box')
    ap.add_argument('--cli', action='append', required=True, help='a previous release\'s arcade-link CLI (repeatable)')
    ap.add_argument('--start-seconds', type=float, default=60)
    a = ap.parse_args()
    if os.environ.get('ARCADE_E2E_INNER') != '1':
        raise SystemExit('n1: run this inside `tools/e2e.py run --`')

    app = subprocess.Popen([a.app, '--background'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                           start_new_session=True)
    try:
        for cli in a.cli:
            version = run([cli, '--version']).stdout.strip() or cli

            def listed():
                r = run([cli, 'ls'])
                return any(a.id in line and ' running' in line for line in r.stdout.splitlines()), r.stdout + r.stderr

            until(listed, f'{version}: `ls` never showed {a.id} running', a.start_seconds)

            def status():
                r = run([cli, 'status', a.id])
                return r.returncode == 0, r.stdout + r.stderr

            until(status, f'{version}: `status {a.id}` failed', 15)
            r = run([cli, 'describe', a.id, '--json'])
            if r.returncode != 0:
                raise SystemExit(f'n1: {version}: `describe {a.id} --json` failed: {r.stderr.strip()}')
            described = json.loads(r.stdout)
            actions = described if isinstance(described, list) else described.get('actions', [])
            print(f'ok: {version} lists {a.id} running, reads its status and describes {len(actions)} actions')
        quit_ = run([a.app, '--quit'], timeout=60)
        if quit_.returncode != 0:
            raise SystemExit(f'n1: `--quit` failed ({quit_.returncode}): {quit_.stderr.strip()}')
        # The launcher may have handed over to an unpacked copy (U1), so ask Link rather than wait on our child.
        def stopped():
            r = run([a.cli[-1], 'ls'])
            return not any(a.id in line and ' running' in line for line in r.stdout.splitlines()), r.stdout
        until(stopped, f'{a.id} was still running 30 s after --quit', 30)
        return 0
    finally:
        if app.poll() is None:
            app.kill()


if __name__ == '__main__':
    sys.exit(main())
