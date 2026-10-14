#!/usr/bin/env python3
"""Download green Tools-hub Linux artifacts; record every unavailable app.

This fetches packages and optionally sources, never builds or installs an app.
Use ARCADE_E2E_ARTIFACTS=<output>/fleet.json for an isolated real-app run.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import fnmatch
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

import e2e


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


def fetch(app, spec, destination, sources=False):
    repo = spec['repo']
    result = {'repository': repo}
    runs = json.loads(command(['gh', 'run', 'list', '-R', repo, '--branch', 'arcade/tools-hub',
                              '--status', 'success', '--limit', '5',
                              '--json', 'databaseId,headSha,headBranch,conclusion,url']))
    chosen = None
    # Runs are newest first. Ignore unrelated successful workflows (e.g. tags)
    # and select the newest green packaging run with an unexpired Linux asset.
    for run in runs:
        pages = json.loads(command(['gh', 'api', f'repos/{repo}/actions/runs/{run["databaseId"]}/artifacts', '--paginate', '--slurp']))
        artifacts = [artifact for page in pages for artifact in page['artifacts']]
        artifact = next((a for a in artifacts if not a['expired'] and fnmatch.fnmatchcase(a['name'], spec['artifact'])), None)
        if artifact:
            chosen = run, artifact
            break
    source_run = chosen[0] if chosen else (runs[0] if runs else None)
    if source_run:
        result.update(headSha=source_run['headSha'], runUrl=source_run['url'])
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
        result['reason'] = (f'no unexpired {spec["artifact"]} AppImage artifact in the latest five successful '
                            f'arcade/tools-hub runs of {repo}' if runs else
                            f'{repo} has no successful arcade/tools-hub workflow run')
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
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    inventory = {}
    failed = False
    def fetch_one(app, spec):
        nonlocal failed
        destination = output / app
        destination.mkdir(exist_ok=True)
        try:
            item = fetch(app, spec, destination, args.sources)
        except subprocess.CalledProcessError as error:
            item = {'repository': spec['repo'], 'reason': f'artifact fetch failed: {(error.stderr or error.stdout).strip()[:500]}'}
        except subprocess.TimeoutExpired as error:
            item = {'repository': spec['repo'], 'reason': str(error)}
        except ValueError as error:
            item = {'repository': spec['repo'], 'reason': str(error)}
            failed = True  # Invalid packages and ambiguous reused directories fail.
        return app, item
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        futures = [pool.submit(fetch_one, app, spec) for app, spec in e2e.FLEET.items()]
        for future in concurrent.futures.as_completed(futures):
            app, item = future.result()
            inventory[app] = item
            print(f'{"SKIP" if "reason" in item else "FETCHED"} {app}: {item.get("reason", item.get("path"))}', flush=True)
            (output / 'fleet.json').write_text(json.dumps(inventory, indent=2) + '\n')
    return int(failed)


if __name__ == '__main__':
    raise SystemExit(main())
