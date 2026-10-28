#!/usr/bin/env python3
"""Copy or verify a commit-pinned Qt bundle (stdlib only, no network).

  python3 tools/vendor_qt.py --destination /path/to/app/vendor/arcade-link
  python3 tools/vendor_qt.py --destination /path/to/app/vendor/arcade-link --check

The source is read with git show, so uncommitted edits cannot masquerade as a
commit pin. Existing files may be replaced only when an earlier VENDORED.json
owns them. The directory layout supports add_subdirectory(.../qt). The QML
design kit (design/qml/Arcade, with Inter in design/fonts) comes along under
the same pin.
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def bundle(destination: Path, revision="HEAD", check=False):
    destination = destination.resolve()
    if destination == ROOT or ROOT in destination.parents:
        raise ValueError("vendor into an app or temporary directory outside Link")
    pin = destination / "VENDORED.json"
    old = json.loads(pin.read_text(encoding="utf-8")) if pin.exists() else {}
    if check:
        if not old or len(old.get("commit", "")) != 40:
            raise ValueError("missing full commit pin")
        for name, digest in old["files"].items():
            path = Path(name)
            if path.is_absolute() or ".." in path.parts:
                raise ValueError("invalid vendor path")
            source = git("show", f"{old['commit']}:{name}")
            if hashlib.sha256(source).hexdigest() != digest or not (destination / name).is_file() or hashlib.sha256((destination / name).read_bytes()).hexdigest() != digest:
                raise ValueError(f"{name}: differs from the pin")
        return old
    commit = git("rev-parse", "--verify", f"{revision}^{{commit}}").decode().strip()
    names = git("ls-tree", "-r", "--name-only", commit).decode().splitlines()
    names = [n for n in names if n.startswith(("qt/", "spec/vectors/", "fixtures/", "design/qml/")) or n in (
        "design/fonts/InterVariable.ttf", "design/fonts/OFL.txt",
        "SPEC.md", "Cargo.toml", "tools/arcade-release.py", "tools/validate_shortcuts.py",
        "spec/shortcuts.schema.json", "spec/shortcut-sheet.schema.json", "spec/receipt.schema.json",
        "spec/arcade-release.schema.json", "assets/strings/install.json")]
    contents = {n: git("show", f"{commit}:{n}") for n in names}
    for name in names:
        path = destination / name
        if path.exists() and name not in old.get("files", {}):
            raise ValueError(f"{name}: existing file is not owned by the previous pin")
    metadata = {"repository": "https://github.com/qa-p1/Arcade-Link", "arcadeLink": "v0.3.0 (protocol 1)",
                "commit": commit, "license": "MIT OR Apache-2.0",
                "files": {n: hashlib.sha256(data).hexdigest() for n, data in contents.items()}}
    for name, data in contents.items():
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    pin.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return metadata


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--revision", default="HEAD")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        pin = bundle(args.destination, args.revision, args.check)
    except (ValueError, OSError, subprocess.CalledProcessError) as e:
        parser.exit(1, f"vendor: {e}\n")
    print(f"Arcade Link {pin['commit']}: {len(pin['files'])} files {'verified' if args.check else 'copied'}")


if __name__ == "__main__":
    main()
