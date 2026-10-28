#!/usr/bin/env python3
"""Download green Linux artifacts (main by default); record every unavailable app.

This fetches packages and optionally sources, never builds or installs an app.
Use ARCADE_E2E_ARTIFACTS=<output>/fleet.json for an isolated real-app run.

    --pick-only          record the chosen run per app in fleet.json, download nothing
    --pinned FLEET.JSON  fetch exactly the runs an earlier --pick-only chose, so
                         several CI jobs test (and build) the same commits
    --only APP[,APP]     limit to some Link ids
"""
from __future__ import annotations

import argparse
import concurrent.futures
import fnmatch
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

import e2e

# The hub work is merged into main: test what users get. Set ARCADE_E2E_BRANCH for a feature branch.
BRANCH = os.environ.get('ARCADE_E2E_BRANCH', 'main')


def command(argv, timeout=300, **kwargs):
    return subprocess.run(argv, capture_output=True, text=True, check=True, timeout=timeout, **kwargs).stdout


class DownloadFailed(Exception):
    """The app is skipped with this reason; other apps continue."""


def download(argv, packages, attempts=2, timeout=300):
    """Large AppImage ZIPs: 300 s per attempt, one retry at most."""
    for attempt in range(attempts):
        if packages.exists():
            shutil.rmtree(packages)  # Never mix bytes from a partial attempt.
        try:
            return command(argv, timeout=timeout)
        except subprocess.TimeoutExpired:
            error = f'timed out after {timeout} s'
        except subprocess.CalledProcessError as failure:
            error = (failure.stderr or failure.stdout or '').strip()[:300]
    shutil.rmtree(packages, ignore_errors=True)
    raise DownloadFailed(f'artifact download failed ({attempts} attempts): {error}')


def reusable(packages, run, artifact):
    """An earlier verified download of this exact run and artifact, if any."""
    for provenance in packages.glob('*.AppImage.provenance.json'):
        identity = json.loads(provenance.read_text())
        path = provenance.with_name(provenance.name.removesuffix('.provenance.json'))
        if (identity.get('databaseId') == run['databaseId'] and identity.get('artifact') == artifact['name']
                and path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == identity.get('sha256')):
            return path
    return None


def select(spec):
    """The newest green run with an unexpired Linux artifact: (runs, chosen)."""
    repo = spec['repo']
    runs = json.loads(command(['gh', 'run', 'list', '-R', repo, '--branch', BRANCH,
                              '--status', 'success', '--limit', '5',
                              '--json', 'databaseId,headSha,headBranch,conclusion,url']))
    # Runs are newest first. Ignore unrelated successful workflows (e.g. tags)
    # and select the newest green packaging run with an unexpired Linux asset.
    for run in runs:
        pages = json.loads(command(['gh', 'api', f'repos/{repo}/actions/runs/{run["databaseId"]}/artifacts', '--paginate', '--slurp']))
        artifacts = [artifact for page in pages for artifact in page['artifacts']]
        artifact = next((a for a in artifacts if not a['expired'] and fnmatch.fnmatchcase(a['name'], spec['artifact'])), None)
        if artifact:
            return runs, (run, artifact)
    return runs, None


def pick(app, spec):
    """fleet.json entry for --pick-only: the chosen run, nothing downloaded."""
    runs, chosen = select(spec)
    result = {'repository': spec['repo']}
    source_run = chosen[0] if chosen else (runs[0] if runs else None)
    if source_run:
        result.update(headSha=source_run['headSha'], runUrl=source_run['url'], run=source_run)
    if chosen:
        result['artifact'] = chosen[1]['name']
    else:
        result['reason'] = no_artifact_reason(spec, runs)
    return result


def no_artifact_reason(spec, runs):
    return (f'no unexpired {spec["artifact"]} AppImage artifact in the latest five successful '
            f'{BRANCH} runs of {spec["repo"]}' if runs else
            f'{spec["repo"]} has no successful {BRANCH} workflow run')


def fetch(app, spec, destination, sources=False, pinned=None):
    repo = spec['repo']
    result = {'repository': repo}
    if pinned is None:
        runs, chosen = select(spec)
    else:
        # Exactly the run another job picked; its artifact name is re-resolved.
        runs = [pinned['run']] if 'run' in pinned else []
        chosen = None
        if 'artifact' in pinned and runs:
            pages = json.loads(command(['gh', 'api', f'repos/{repo}/actions/runs/{runs[0]["databaseId"]}/artifacts', '--paginate', '--slurp']))
            artifact = next((a for page in pages for a in page['artifacts'] if a['name'] == pinned['artifact'] and not a['expired']), None)
            chosen = (runs[0], artifact) if artifact else None
    source_run = chosen[0] if chosen else (runs[0] if runs else None)
    if source_run:
        result.update(headSha=source_run['headSha'], runUrl=source_run['url'], run=source_run)
    if sources and source_run:
        checkout = destination / 'sources'
        if checkout.exists():
            # Reuse an earlier clone only at exactly the selected commit. A
            # directory without its own .git would resolve an enclosing repo.
            head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=checkout, capture_output=True, text=True).stdout.strip() \
                if (checkout / '.git').exists() else ''
            if head != source_run['headSha']:
                raise ValueError(f'{checkout} exists at {head or "no commit"}, not {source_run["headSha"]}; use a fresh output directory')
        else:
            command(['git', 'clone', '--quiet', '--no-checkout', f'https://github.com/{repo}.git', str(checkout)])
            command(['git', 'checkout', '--quiet', '--detach', source_run['headSha']], cwd=checkout)
        result['repoPath'] = str(checkout)
    if chosen is None:
        result['reason'] = (pinned or {}).get('reason') or no_artifact_reason(spec, runs)
        return result
    run, artifact = chosen
    packages = destination / 'packages'
    reused = reusable(packages, run, artifact)
    if reused:
        result.update(path=str(reused), sha256=hashlib.sha256(reused.read_bytes()).hexdigest(), artifact=artifact['name'], reused=True)
        return result
    try:
        download(['timeout', '300', 'gh', 'run', 'download', str(run['databaseId']), '-R', repo, '-n', artifact['name'], '-D', str(packages)], packages)
    except DownloadFailed as error:
        result['reason'] = f'{repo} run {run["databaseId"]}/{artifact["name"]}: {error}'
        return result
    candidates = sorted(packages.rglob('*.AppImage'))
    if len(candidates) != 1:
        result['reason'] = f'{repo} run {run["databaseId"]}/{artifact["name"]}: expected one Linux AppImage; found {len(candidates)}'
        return result
    path = candidates[0]
    with path.open('rb') as stream:
        signature = stream.read(11)
    if signature[:4] != b'\x7fELF' or signature[8:11] not in (b'AI\x01', b'AI\x02'):
        raise ValueError(f'{path}: downloaded file is not an ELF AppImage')
    path.chmod(0o755)  # GitHub ZIP artifacts do not preserve executable bits.
    provenance = {**run, 'repository': repo, 'artifact': artifact['name'],
                  'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
    path.with_suffix('.AppImage.provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    result.update(path=str(path), sha256=provenance['sha256'], artifact=artifact['name'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--sources', action='store_true', help='clone each source at the selected CI commit into output')
    parser.add_argument('--pick-only', action='store_true', help='choose each run and write fleet.json; download nothing')
    parser.add_argument('--pinned', type=Path, help='fleet.json from --pick-only: fetch exactly those runs')
    parser.add_argument('--only', help='comma-separated Link ids')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    pins = json.loads(args.pinned.read_text(encoding='utf-8')) if args.pinned else None
    fleet = {app: spec for app, spec in e2e.FLEET.items() if not args.only or app in args.only.split(',')}
    inventory = {}
    failed = False
    def fetch_one(app, spec):
        nonlocal failed
        destination = output / app
        destination.mkdir(exist_ok=True)
        try:
            if args.pick_only:
                item = pick(app, spec)
            else:
                item = fetch(app, spec, destination, args.sources, None if pins is None else pins.get(app, {}))
        except subprocess.CalledProcessError as error:
            item = {'repository': spec['repo'], 'reason': f'artifact fetch failed: {(error.stderr or error.stdout).strip()[:500]}'}
        except subprocess.TimeoutExpired as error:
            item = {'repository': spec['repo'], 'reason': str(error)}
        except ValueError as error:
            item = {'repository': spec['repo'], 'reason': str(error)}
            failed = True  # Invalid packages and ambiguous reused directories fail.
        return app, item
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        futures = [pool.submit(fetch_one, app, spec) for app, spec in fleet.items()]
        for future in concurrent.futures.as_completed(futures):
            app, item = future.result()
            inventory[app] = item
            state = "SKIP" if "reason" in item else "PICKED" if args.pick_only else "FETCHED"
            print(f'{state} {app}: {item.get("reason", item.get("path", item.get("headSha")))}', flush=True)
            (output / 'fleet.json').write_text(json.dumps(inventory, indent=2) + '\n')
    return int(failed)


if __name__ == '__main__':
    raise SystemExit(main())
