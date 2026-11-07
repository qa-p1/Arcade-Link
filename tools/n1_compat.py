#!/usr/bin/env python3
"""N-1 compatibility: the previous Link release and this one talk in both directions.

    python3 tools/n1_compat.py OLD_CLI NEW_CLI

Each CLI in turn runs the mock app (fixtures/box.json) while the other one lists,
describes, invokes an action on and quits it. Exits non-zero on the first failure.
"""

import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

FIXTURE = Path(__file__).resolve().parent.parent / "fixtures" / "box.json"


def run(cli, home, *args, check=True):
    r = subprocess.run([cli, *args], env={**os.environ, "ARCADE_HOME": home}, capture_output=True, text=True, timeout=30)
    if check and r.returncode != 0:
        raise SystemExit(f"{cli} {' '.join(args)} failed ({r.returncode}): {r.stderr}")
    return r


def pair(server, client):
    with tempfile.TemporaryDirectory() as home:
        mock = subprocess.Popen([server, "mock", "--as", "arcade.box", "--actions", str(FIXTURE)],
                                env={**os.environ, "ARCADE_HOME": home},
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            deadline = time.time() + 10
            while "running" not in run(client, home, "ls", check=False).stdout:
                if time.time() > deadline:
                    raise SystemExit(f"{server} mock never showed as running to {client}")
                time.sleep(0.05)
            actions = json.loads(run(client, home, "describe", "box", "--json").stdout)
            assert actions, "no actions described"
            actions = actions if isinstance(actions, list) else actions["actions"]
            first = actions[0]
            run(client, home, "status", "box")
            image = Path(home) / "n1.png"
            image.write_bytes(b"\x89PNG\r\n\x1a\n")
            run(client, home, "invoke", "box", first["id"], "--file", str(image))
            run(client, home, "quit", "box")
            mock.wait(timeout=10)
            print(f"ok: server {server} ↔ client {client} ({first['id']})")
        finally:
            if mock.poll() is None:
                mock.kill()


def main():
    old, new = sys.argv[1:3]
    pair(old, new)
    pair(new, old)


if __name__ == "__main__":
    main()
