#!/usr/bin/env python3
"""Publish, promote and nightly-promote Arcade releases (ARCADE_APP.md REL-1…REL-3).

Run by Link's reusable workflows (app-publish.yml, app-promote.yml,
app-nightly.yml); `gh` must be logged in (GITHUB_TOKEN in Actions) and
`arcade-link` must be on PATH.

    release_flow.py publish  --repo O/R --app ID --version V --tag T --sha SHA DIR [--channel test|nightly] [--draft]
    release_flow.py promote  --repo O/R --rc-tag T [--tag T2] [--draft]
    release_flow.py changed  --repo O/R --sha SHA      # exit 0 if SHA differs from the newest nightly's commit

Payload files are never rebuilt or changed: promotion downloads the rc's files,
checks them against the rc manifest, publishes them under the stable tag with
a newly signed manifest, downloads them again and compares every hash.
`arcade-release.json` and `SHA256SUMS.txt` (read by Tools 0.4.x) are
metadata and are regenerated for the new tag and channel.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
METADATA = {"arcade-release.json", "SHA256SUMS.txt", "manifest.json", "manifest.json.sig"}
RC = re.compile(r"^v(\d+\.\d+\.\d+)-rc\.\d+$")


def sh(*args: str, retries: int = 3) -> str:
    """Runs a command; network commands (gh) are retried with backoff."""
    for attempt in range(retries):
        r = subprocess.run(args, capture_output=True, text=True, timeout=900)
        if r.returncode == 0:
            return r.stdout
        if attempt + 1 < retries and args[0] == "gh":
            time.sleep(2 ** (attempt + 1))
            continue
        raise SystemExit(f"{' '.join(args)} failed ({r.returncode}):\n{r.stdout}\n{r.stderr}")
    return ""


def sha256(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def payload(d: Path) -> list[Path]:
    return sorted(p for p in d.iterdir() if p.is_file() and p.name not in METADATA)


def notes_url(repo: str, tag: str) -> str:
    return f"https://github.com/{repo}/releases/tag/{tag}"


def write_metadata(d: Path, app: str, version: str, tag: str, channel: str, repo: str, sign: bool) -> None:
    files = payload(d)
    if not files:
        raise SystemExit(f"{d} has no release files")
    sh(sys.executable, str(HERE / "arcade-release.py"), "--id", app, "--version", version, "--channel", channel,
       "--notes", notes_url(repo, tag), str(d))
    cmd = ["arcade-link", "release-manifest", "--app", app, "--version", version, "--tag", tag, "--channel", channel,
           "--out", str(d)] + [str(p) for p in files]
    if sign:
        cmd.append("--sign")
    sh(*cmd)


def create(repo: str, tag: str, sha: str, d: Path, prerelease: bool, draft: bool, title: str) -> None:
    cmd = ["gh", "release", "create", tag, "--repo", repo, "--target", sha, "--title", title,
           "--notes", f"See CHANGELOG.md. Manifest: manifest.json{' (signed)' if (d / 'manifest.json.sig').exists() else ''}."]
    cmd += ["--prerelease", "--latest=false"] if prerelease else ["--latest"]
    if draft:
        cmd.append("--draft")
    sh(*cmd, *[str(p) for p in sorted(d.iterdir()) if p.is_file()])


def publish(a: argparse.Namespace) -> None:
    d = Path(a.dir)
    write_metadata(d, a.app, a.version, a.tag, a.channel, a.repo, sign=a.channel == "nightly")
    create(a.repo, a.tag, a.sha, d, prerelease=True, draft=a.draft, title=f"{a.app} {a.tag[1:]} ({a.channel})")
    print(f"published {a.tag} ({a.channel}{', draft' if a.draft else ''})")


def download(repo: str, tag: str, d: Path) -> None:
    d.mkdir(parents=True, exist_ok=True)
    sh("gh", "release", "download", tag, "--repo", repo, "--dir", str(d), "--clobber")


def release_commit(repo: str, tag: str) -> str:
    """The commit a release (draft or published) was created from."""
    out = sh("gh", "release", "view", tag, "--repo", repo, "--json", "targetCommitish", "--jq", ".targetCommitish").strip()
    if not re.fullmatch(r"[0-9a-f]{40}", out):
        out = sh("gh", "api", f"repos/{repo}/commits/{tag}", "--jq", ".sha").strip()
    if not re.fullmatch(r"[0-9a-f]{40}", out):
        raise SystemExit(f"cannot resolve {tag}: {out!r}")
    return out


def promote(a: argparse.Namespace) -> None:
    m = RC.match(a.rc_tag)
    if not m:
        raise SystemExit(f"{a.rc_tag} is not an rc tag (vX.Y.Z-rc.N)")
    tag = a.tag or f"v{m.group(1)}"
    if not os.environ.get("ARCADE_UPDATE_SIGNING_KEY", "").strip():
        raise SystemExit("ARCADE_UPDATE_SIGNING_KEY is not set: refusing to publish an unsigned stable release")
    work = Path(tempfile.mkdtemp(prefix="promote-"))
    rc_dir, out = work / "rc", work / "stable"
    download(a.repo, a.rc_tag, rc_dir)
    rc_manifest = json.loads((rc_dir / "manifest.json").read_text())
    if rc_manifest["channel"] != "test" or rc_manifest["tag"] != a.rc_tag:
        raise SystemExit(f"{a.rc_tag}: manifest says {rc_manifest['tag']} on {rc_manifest['channel']}")
    out.mkdir()
    for f in rc_manifest["files"]:
        shutil.copy2(rc_dir / f["name"], out / f["name"])
    sh("arcade-link", "verify-manifest", str(rc_dir / "manifest.json"), "--unsigned", "--files", str(out))
    sh(sys.executable, str(HERE / "arcade-release.py"), "--id", rc_manifest["app"], "--version", rc_manifest["version"],
       "--channel", "stable", "--notes", notes_url(a.repo, tag), str(out))
    sh("arcade-link", "promote-manifest", str(rc_dir / "manifest.json"), "--tag", tag, "--channel", "stable",
       "--files", str(out), "--out", str(out))
    create(a.repo, tag, release_commit(a.repo, a.rc_tag), out, prerelease=False, draft=a.draft,
           title=f"{rc_manifest['app']} {tag[1:]}")
    check = work / "check"
    if a.draft:
        print("draft: skipping the public re-download; comparing local copies")
        check = out
    else:
        download(a.repo, tag, check)
    for f in rc_manifest["files"]:
        got = sha256(check / f["name"])
        if got != f["sha256"]:
            raise SystemExit(f"{f['name']}: promoted file differs from {a.rc_tag} ({got} != {f['sha256']})")
    print(f"promoted {a.rc_tag} → {tag}: {len(rc_manifest['files'])} files, byte-identical, manifest signed")


def changed(a: argparse.Namespace) -> None:
    tags = sh("gh", "api", f"repos/{a.repo}/tags", "--paginate", "--jq", ".[].name").split()
    nightlies = sorted((t for t in tags if "-nightly." in t), reverse=True)
    if nightlies and sh("gh", "api", f"repos/{a.repo}/commits/{nightlies[0]}", "--jq", ".sha").strip() == a.sha:
        print(f"unchanged since {nightlies[0]}")
        sys.exit(1)
    print("changed")


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    pub = sub.add_parser("publish")
    for opt in ("--repo", "--app", "--version", "--tag", "--sha"):
        pub.add_argument(opt, required=True)
    pub.add_argument("--channel", default="test", choices=["test", "nightly"])
    pub.add_argument("--draft", action="store_true")
    pub.add_argument("dir")
    pro = sub.add_parser("promote")
    pro.add_argument("--repo", required=True)
    pro.add_argument("--rc-tag", required=True)
    pro.add_argument("--tag")
    pro.add_argument("--draft", action="store_true")
    ch = sub.add_parser("changed")
    ch.add_argument("--repo", required=True)
    ch.add_argument("--sha", required=True)
    a = p.parse_args()
    {"publish": publish, "promote": promote, "changed": changed}[a.cmd](a)


if __name__ == "__main__":
    main()
