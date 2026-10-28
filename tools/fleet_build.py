#!/usr/bin/env python3
"""Build one fleet app's native development executable from its checkout.

    python3 tools/fleet_build.py arcade.find target/fleet/arcade.find/sources

The *_development_install_suppression checks run these executables (not the
AppImages) at the same commit as the downloaded package, so the nightly job
builds them first instead of reporting "stale or missing build". The output
path is FLEET[app]["bin"] inside the checkout. System packages and toolchains
(Node, Qt) are installed by the workflow; see .github/workflows/e2e.yml.
"""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys

import e2e

RECIPES = {
    "arcade.box": [
        (["npm", "ci"], "apps/desktop/frontend"),
        (["npm", "run", "build"], "apps/desktop/frontend"),
        (["cargo", "build", "--release", "--locked", "-p", "arcade-desktop", "-p", "arcade-box"], "."),
    ],
    "arcade.lens": [(["cargo", "build", "--release", "--locked", "-p", "arcade-lens"], ".")],
    "arcade.find": [(["cargo", "build", "--release", "--locked", "-p", "arcade-find"], ".")],
    "arcade.clipboard": [(["cargo", "build", "--release", "--locked", "-p", "arcade-clipboard-launcher"], ".")],
    # scripts/verify-link.py --build-e2e, without its lint and test steps.
    "arcade.look": [
        (["npm", "ci"], "."),
        (["npm", "run", "build"], "."),
        (["npx", "tauri", "build", "--debug", "--no-bundle", "--features", "e2e"], "."),
    ],
    "arcade.wheel": [
        (["cmake", "-S", ".", "-B", "build", "-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTING=OFF"], "."),
        (["cmake", "--build", "build", "--parallel", "3"], "."),
    ],
    "arcade.shelf": [
        (["cmake", "-S", ".", "-B", "build", "-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_TESTING=OFF"], "."),
        (["cmake", "--build", "build", "--parallel", "3"], "."),
    ],
}
# Look's e2e build keeps its own target directory (scripts/verify-link.py).
TARGET_DIRS = {"arcade.look": "src-tauri/target/link-tests"}
# The development check runs Clipboard's launcher, not its Flutter bundle
# (tools/e2e_checks/real_hub.py).
DEV_BINS = {"arcade.clipboard": "target/release/arcade-clipboard-launcher"}


def dev_bin(app: str) -> str:
    return DEV_BINS.get(app, e2e.FLEET[app]["bin"])


def build(app: str, checkout: Path) -> Path:
    checkout = checkout.resolve()
    env = dict(os.environ, CARGO_BUILD_JOBS="3")
    if app in TARGET_DIRS:
        env["CARGO_TARGET_DIR"] = str(checkout / TARGET_DIRS[app])
    for command, folder in RECIPES[app]:
        print(f"{app}: {' '.join(command)} (in {folder})", flush=True)
        subprocess.run(command, cwd=checkout / folder, env=env, check=True)
    executable = checkout / dev_bin(app)
    if not executable.is_file():
        raise SystemExit(f"{app}: the build finished but {executable} is missing")
    return executable


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[0] not in RECIPES:
        print(__doc__, file=sys.stderr)
        print("apps: " + ", ".join(RECIPES), file=sys.stderr)
        return 2
    print(build(argv[0], Path(argv[1])))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
