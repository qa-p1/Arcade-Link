"""§17.2 with real apps, real AppImages and the actual sibling Tools core.

The loopback server controls release metadata, never substitutes mock app code.
Package operations are explicit and confined to each check's disposable profile.
"""
import concurrent.futures
import contextlib
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time
import zipfile

from real_apps import (Peer, Releases, binary, call, child_session, driver, fleet, hub_sources, index,
                       package, receipt, run_app, tools_preferences, tray, wait)
from validate_shortcuts import normalize, validate


# Confirmed product defects outside Link (docs/dev/history/e2e-0.3.md, "Known product
# bugs"). The runner reports XFAIL only when the failure message contains the
# expected Tools rejection; any other failure, or a pass, still fails the run.
BACKUP_REJECTED = 'The app settings contained excluded data or exceeded backup limits.'
# Lens, Wheel and Shelf backups were rejected before Arcade Tools 0.2.0 (fixed in
# qa-p1/Arcade-tools cf8f0c5); every app must now round-trip.
BACKUP_BUGS: dict[str, str] = {}


def known_bug(function, note, match=BACKUP_REJECTED):
    function.known_bug, function.known_bug_match = note, match
    return function


SHELF_MAGIC = b'ARCADE-SHELF-PORTABLE\n'


def export_snapshot(path):
    """Comparable content of one app.settings.export file.

    Exports are not JSON-only: Shelf writes magic, a big-endian u64 manifest
    length, the JSON manifest and owned payloads. Compare its manifest minus
    the export timestamp, plus the payload bytes. Unknown formats compare as
    exact bytes, never as a parse error that hides the real behavior.
    """
    data = Path(path).read_bytes()
    try:
        return json.loads(data)
    except ValueError:
        pass
    if data.startswith(SHELF_MAGIC):
        start = len(SHELF_MAGIC) + 8
        length = int.from_bytes(data[len(SHELF_MAGIC):start], 'big')
        manifest = json.loads(data[start:start + length])
        manifest.pop('createdAt', None)
        return {'shelfManifest': manifest, 'payloadSha256': hashlib.sha256(data[start + length:]).hexdigest()}
    return {'sha256': hashlib.sha256(data).hexdigest(), 'size': len(data)}


def per_app(group, app, suffix, body, known=None):
    """Every app gets its own named result and disposable profile."""
    def run(s):
        child = child_session(s, app.replace('.', '-') + '-' + suffix)
        try:
            return body(child, app)
        finally:
            child.close()
    run.__name__ = app.replace('.', '_') + '_' + suffix
    run.apps = [app] + (['arcade.tools'] if group == 'tools-hub' and app != 'arcade.tools' else [])
    if known:
        known_bug(run, known)
    check(group)(run)


def _install_case(app):
    def lifecycle(s):
        artifact = package(app)
        child = child_session(s, app.replace('.', '-') + '-install')
        try:
            source = child.root / 'home/Downloads' / artifact.name
            source.parent.mkdir(parents=True)
            shutil.copy2(artifact, source)
            status = run_app(child, source, '--integration-status')
            assert json.loads(status.stdout).get('receipt') is None, status.stdout
            run_app(child, source, '--install', '--silent', '--no-launch')
            first = receipt(child, app)
            installed = Path(first['path'])
            assert installed.is_file() and installed.is_relative_to(child.root), first
            assert first['method'] == 'appimage' and first['managedBy'] == 'self', first
            owned = [Path(first['integration'][k]) for k in ('desktopEntry', 'cli')]
            icons = [Path(p) for p in first['integration']['icons']]
            assert icons and all(p.exists() and p.is_relative_to(child.root) for p in owned + icons), first
            assert str(installed) in owned[0].read_text(encoding='utf-8'), first
            assert owned[1].resolve() == installed, first
            assert owned[1].name == FLEET[app]['cli'], first
            assert installed.stat().st_mode & 0o777 == 0o755, installed
            assert (child.root / f'arcade/installs/{app}.json').stat().st_mode & 0o777 == 0o600

            # A receipt and byte-identifiable older installation fixture; the
            # candidate package and installer remain the real current build.
            old_hash = hashlib.sha256(installed.read_bytes()).hexdigest()
            old_receipt = child.root / f'arcade/installs/{app}.json'
            first['version'] = '0.0.1'
            old_receipt.write_text(json.dumps(first), encoding='utf-8')
            shutil.copy2(artifact, source)
            run_app(child, source, '--install', '--silent', '--no-launch')
            updated = receipt(child, app)
            assert updated['version'] != '0.0.1' and updated.get('previous'), updated
            previous = Path(updated['previous']['path'])
            assert hashlib.sha256(previous.read_bytes()).hexdigest() == old_hash, updated

            # The source file actually moves; repair must follow that file and
            # rewrite launcher and CLI, not just claim the receipt is healthy.
            moved = installed.with_name('Moved-' + installed.name)
            installed.rename(moved)
            owned[0].unlink()
            run_app(child, moved, '--repair')
            repaired = receipt(child, app)
            assert Path(repaired['path']) == moved, repaired
            assert Path(repaired['integration']['cli']).resolve() == moved, repaired
            assert str(moved) in Path(repaired['integration']['desktopEntry']).read_text(encoding='utf-8')
            status = json.loads(run_app(child, moved, '--integration-status').stdout)
            assert status.get('entries') and all(e['exists'] for e in status['entries']), status
            run_app(child, moved, '--uninstall', '--silent')
            assert not moved.exists() and not previous.exists() and not old_receipt.exists(), repaired
            for path in owned + icons:
                assert not path.exists() and not path.is_symlink(), path

            # Explicit install without --no-launch must relaunch from the durable
            # package: the downloaded image starts the installed copy and exits,
            # so its own mount goes away. The manifest, not an inferred
            # launcher, proves the path.
            shutil.copy2(artifact, source)
            settings = child.root / 'find/config/settings.json'
            settings.parent.mkdir(parents=True, exist_ok=True)
            settings.write_text(json.dumps({'schema': 1, 'roots': [str(child.root / 'home')]}), encoding='utf-8')
            with (child.root / f'{app}.log').open('ab') as log:
                p = subprocess.Popen([str(source), '--install', '--silent'],
                                     env=dict(child.env, APPIMAGE_EXTRACT_AND_RUN='1'), stdout=log, stderr=log, start_new_session=True)
            assert p.wait(timeout=60) == 0, f'{app} install exited {p.returncode}; log:\n{child.log(app)}'
            child.wait_running(app)
            current = receipt(child, app)
            manifest = json.loads((child.root / f'arcade/apps/{app}.json').read_text(encoding='utf-8'))
            assert manifest['executable'] == current['path'], manifest
            call(child, app, 'app.quit')
            wait(lambda: not child.endpoint(app).exists(), f'{app} did not quit')
            run_app(child, Path(current['path']), '--uninstall', '--silent')
            assert not Path(current['path']).exists()
            return f'{artifact}: install/files/0600 receipt/update older fixture/moved repair/uninstall/installed re-exec'
        finally:
            child.close()
    lifecycle.__name__ = app.replace('.', '_') + '_appimage_install_update_repair_uninstall_reexec'
    lifecycle.apps = [app]
    return lifecycle


for _app in FLEET:
    check('install')(_install_case(_app))


def _development_case(app):
    def development(s):
        spec = FLEET[app]
        # This deliberately invokes the development executable without APPIMAGE.
        cli_spec = {**spec, 'bin': 'target/release/arcade-clipboard-launcher', 'source_suffixes': {'.rs', '.toml'}, 'source_names': {'Cargo.lock'}, 'components': []} if app == 'arcade.clipboard' else spec
        hub_sources(app)
        native = require_current_build(cli_spec)
        child = child_session(s, app.replace('.', '-') + '-dev')
        try:
            child.env.pop('ARCADE_NO_INSTALL_PROMPT', None)
            fleet(child, [app])
            result = run_app(child, native, '--integration-status')
            status = json.loads(result.stdout)
            assert status.get('receipt') is None, (app, status)
            manifest = json.loads((child.root / f'arcade/apps/{app}.json').read_text(encoding='utf-8'))
            # Tools' host manifest does not advertise the optional install
            # label. Its native installer status and absent receipt are the
            # development-build contract; the seven peers publish install=dev.
            if app != 'arcade.tools':
                assert manifest.get('install') == 'dev', (app, manifest.get('install'))
            assert not (child.root / f'arcade/installs/{app}.json').exists(), app
            assert not child.xdotool('search', '--onlyvisible', '--name', 'Install Arcade'), 'development build displayed an install dialog'
            return f'{app}: native development process, install=dev, no receipt or install dialog'
        finally:
            child.close()
    development.__name__ = app.replace('.', '_') + '_development_install_suppression'
    development.apps = [app]
    return development


for _app in FLEET:
    check('install')(_development_case(_app))


@check('trayhost')
def real_apps_hand_back_within_one_second_after_tools_sigkill(s):
    fleet(s)
    tools_preferences(s)
    for app in APP_IDS:
        wait(lambda: tray(s, app) == 'own', f'{app}: expected own tray before Tools')
    s.start('arcade.tools')
    for app in APP_IDS:
        wait(lambda: tray(s, app) == 'hosted', lambda: f'{app}: expected hosted; status={call(s, app, "app.status")}\n{s.log(app)}')
    with contextlib.ExitStack() as stack:
        peers = {app: stack.enter_context(Peer(s, app)) for app in APP_IDS}
        gate = __import__('threading').Event()
        def observe(app):
            gate.wait()
            deadline = time.perf_counter() + 1
            while time.perf_counter() < deadline:
                if peers[app].call('app.status').get('status', {}).get('tray') == 'own':
                    return (time.perf_counter() - began) * 1000
                time.sleep(.002)
            raise AssertionError(f'{app}: status.tray did not return to own within 1 s; {s.log(app)}')
        with concurrent.futures.ThreadPoolExecutor(max_workers=len(APP_IDS)) as pool:
            pending = {app: pool.submit(observe, app) for app in APP_IDS}
            began = time.perf_counter()
            s.procs['arcade.tools'].kill()  # SIGKILL; only this run's Tools PID.
            gate.set()
            failures, latencies = [], {}
            for app, future in pending.items():
                try:
                    latencies[app] = round(future.result(timeout=2), 3)
                except Exception as error:
                    failures.append(str(error))
            assert not failures, '\n'.join(failures)
        s.kill('arcade.tools')
    (s.root / 'tray-handback.json').write_text(json.dumps(latencies), encoding='utf-8')
    return f'real Tools SIGKILL → real app status.tray=own (ms): {latencies}'


@check('trayhost')
def tools_first_restart_and_clean_quit_preserve_real_app_trays(s):
    for app in APP_IDS:
        s.kill(app)
    s.kill('arcade.tools')
    tools_preferences(s)
    s.start('arcade.tools')
    fleet(s)
    for app in APP_IDS:
        wait(lambda: tray(s, app) == 'hosted', lambda: f'{app}: app started after Tools did not become hosted; '
             f'status={call(s, app, "app.status")}\n{s.log(app)}\nTools:\n{s.log("arcade.tools")}')
    response = call(s, 'arcade.tools', 'app.restart')
    assert response.get('restarting'), response
    # Continuously sample every real app during successor handover. No mock
    # watcher stands in for app.status; observations are protocol state only.
    deadline = time.monotonic() + 3
    samples = 0
    while time.monotonic() < deadline:
        states = {app: tray(s, app) for app in APP_IDS}
        assert all(v == 'hosted' for v in states.values()), f'tray state flashed during restarting: {states}'
        samples += 1
        time.sleep(.01)
    # The restarted parent exits by design; follow the successor's endpoint.
    s.procs.pop('arcade.tools', None)
    s.wait_running('arcade.tools')
    call(s, 'arcade.tools', 'app.quit')
    for app in APP_IDS:
        wait(lambda: tray(s, app) == 'own', f'{app}: clean Tools quit did not hand back', timeout=1)
    return f'Tools first; real Tools restarting handover ({samples} state snapshots, no own state); clean quit'


@check('trayhost')
def real_tools_per_app_opt_out_keeps_the_apps_own_icon(s):
    s.kill('arcade.tools')
    tools_preferences(s, excluded=APP_IDS)
    s.start('arcade.tools')
    fleet(s)
    for app in APP_IDS:
        wait(lambda: tray(s, app) == 'own', f'{app}: per-app exclusion failed')
    manifest = json.loads((s.root / 'arcade/apps/arcade.tools.json').read_text(encoding='utf-8'))
    assert set(manifest['settings']['trayHost']['excluded']) == set(APP_IDS), manifest
    return f'{len(APP_IDS)} excluded real apps retain own tray while Tools runs'


@check('trayhost')
def hosted_real_apps_remove_their_status_notifier_items(s):
    # Caelestia and Waybar draw Passive items, so app.status alone cannot
    # prove the icon is gone: a real watcher must lose every app's item.
    from sni_watcher import Watcher, until
    for app in FLEET:
        binary(app)
    for app in APP_IDS:
        s.kill(app)
    s.kill('arcade.tools')
    watcher = Watcher()
    try:
        tools_preferences(s)
        fleet(s)
        apps = {}
        for app in APP_IDS:
            until(lambda: tray(s, app) == 'own', f'{app}: expected own tray before Tools')
            apps[call(s, app, 'app.status')['pid']] = app
        def owners():
            return sorted(apps.get(pid, 'arcade.tools' if pid == tools else str(pid)) for pid in watcher.owner_pids())
        tools = None
        until(lambda: owners() == sorted(APP_IDS), lambda: f'own: expected one item per app, got {owners()}')
        s.start('arcade.tools')
        tools = s.procs['arcade.tools'].pid
        for app in APP_IDS:
            until(lambda: tray(s, app) == 'hosted', f'{app}: expected hosted')
        until(lambda: not set(owners()) & set(APP_IDS),
              lambda: f'hosted apps still registered (Passive items stay visible): {owners()}', timeout=2)
        hosted = owners()
        s.procs['arcade.tools'].kill()  # SIGKILL; only this run's Tools PID.
        until(lambda: sorted(a for a in owners() if a in APP_IDS) == sorted(APP_IDS),
              lambda: f'items did not return after Tools SIGKILL: {owners()}', timeout=2)
        s.kill('arcade.tools')
        return f'{len(APP_IDS)} real apps: one SNI item each when own, none while hosted ({hosted}), all back after Tools SIGKILL'
    finally:
        watcher.close()


@check('trayhost')
def disabling_link_in_find_keeps_its_own_icon(s):
    s.kill('arcade.find')
    path = s.root / 'find/config/settings.json'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({'schema': 1, 'roots': [str(s.root / 'home')], 'link': {'enabled': False}}), encoding='utf-8')
    s.kill('arcade.tools')
    tools_preferences(s)
    s.start('arcade.tools')
    s.start('arcade.find', wait=False)
    executable = binary('arcade.find')
    def own():
        result = run_app(s, executable, '--status', check=False)
        return json.loads(result.stdout) if result.returncode == 0 else None
    status = wait(lambda: (value if value and value.get('tray') == 'own' else None) if (value := own()) is not None else None,
                  lambda: f'Find with Link disabled did not report own tray: {own()}', timeout=5)
    # The earlier SIGKILL may leave a stale endpoint file; no live process
    # (least of all this Find) may own it while Link is disabled.
    if s.endpoint('arcade.find').exists():
        owner = json.loads(s.endpoint('arcade.find').read_text())['pid']
        assert owner != status.get('pid') and not Path(f'/proc/{owner}').exists(), (owner, status)
    assert status.get('link', {}).get('listening') is False, status
    # The native status includes tray even with Link switched off.
    assert status.get('tray') == 'own' or status.get('link', {}).get('tray') == 'own', status
    return 'real Find, disabled Link: own tray, no Link endpoint while Tools hosts'


def _operate(s, server, app, operation, timeout=60):
    # Tools correctly refuses to replace an app that reports a running job
    # (e.g. Find's first index scan right after install); wait it out.
    deadline = time.monotonic() + timeout
    while True:
        try:
            return driver(s, 'operate', source=server.url, request={'id': app, 'operation': operation, 'mode': 'background'})
        except AssertionError as error:
            if 'busy' not in str(error) or time.monotonic() > deadline:
                raise
            time.sleep(1)


@check('tools-hub')
def loopback_catalog_is_loaded_by_the_real_tools_core(s):
    with Releases(s) as server:
        result = driver(s, 'catalog', source=server.url)
        assert result['updated'] == server.catalog['updated'] and '/catalog.json' in server.requests, (result, server.requests)
        assert {a['id'] for a in result['apps']} == set(APP_IDS), result
        return f'actual Tools core fetched loopback catalog: {len(result["apps"])} apps + Tools + bundle'


@check('tools-hub')
def loopback_catalog_bundle_install_real_packages(s):
    for app in APP_IDS:
        package(app)
    with Releases(s) as server:
        for app in APP_IDS:
            server.publish(app)
        result = driver(s, 'catalog', source=server.url)
        assert result['updated'] == server.catalog['updated'] and '/catalog.json' in server.requests, (result, server.requests)
        result = driver(s, 'bundle', source=server.url, bundle='e2e')
        assert len(result) == len(APP_IDS), result
        for app in APP_IDS:
            installed = receipt(s, app)
            assert installed['managedBy'] == 'tools' and Path(installed['path']).is_file(), installed
            s.wait_running(app)
        return f'actual Tools core fetched loopback catalog and installed all {len(APP_IDS)} apps through their real installers'


def _updates(s, app):
    with Releases(s) as server:
        server.publish(app)
        if not (s.root / f'arcade/installs/{app}.json').exists():
            _operate(s, server, app, 'install')
        before = receipt(s, app)
        path = Path(before['path'])
        initial = hashlib.sha256(path.read_bytes()).hexdigest()
        server.publish(app, marker=b'\nArcade e2e update fixture\n')
        _operate(s, server, app, 'update')
        assert hashlib.sha256(path.read_bytes()).hexdigest() != initial
        assert receipt(s, app).get('previous'), receipt(s, app)
        _operate(s, server, app, 'rollback')
        assert hashlib.sha256(path.read_bytes()).hexdigest() == initial
        server.publish(app, version='99.0.0', protocol=[2])
        state = driver(s, 'updates', source=server.url)
        row = state['apps'][app]
        assert 'held' in json.dumps(row).lower() or 'incompatible' in json.dumps(row).lower(), row
        try:
            _operate(s, server, app, 'update')
        except AssertionError as error:
            assert 'protocol' in str(error).lower() or 'incompatible' in str(error).lower(), error
        else:
            raise AssertionError('Tools applied an incompatible release')
        assert hashlib.sha256(path.read_bytes()).hexdigest() == initial
        return f'{app}: real AppImage bytes updated → rollback; protocol-2 update held before replacement'


def _adopt(s, app):
    child = child_session(s, app.replace('.', '-') + '-adopt')
    try:
        with Releases(child) as server:
            source = child.root / ('home/Manual-' + app.removeprefix('arcade.') + '.AppImage')
            shutil.copy2(package(app), source)
            manifest = json.loads(run_app(child, source, '--arcade-manifest').stdout)
            registry = child.root / f'arcade/apps/{app}.json'
            registry.parent.mkdir(parents=True, exist_ok=True)
            registry.write_text(json.dumps(manifest), encoding='utf-8')
            assert not (child.root / f'arcade/installs/{app}.json').exists()
            listed = driver(child, 'list', source=server.url, includeTools=True)
            assert next(a for a in listed if a['id'] == app)['installationClass'] == 'manual', listed
            _operate(child, server, app, 'adopt')
            assert receipt(child, app)['managedBy'] == 'tools'
            return f'{app}: Tools adopted a manual AppImage from its real manifest and created a managed receipt'
    finally:
        child.close()


def _bundle(s, app):
    package(app)
    with Releases(s, apps=[app]) as server:
        server.publish(app)
        result = driver(s, 'bundle', source=server.url, bundle='e2e')
        assert len(result) == 1, result
        installed = receipt(s, app)
        assert installed['managedBy'] == 'tools' and Path(installed['path']).is_relative_to(s.root), installed
        s.wait_running(app)
        return f'{app}: catalog bundle → own installer → Tools-managed receipt and running Link service'


for _app in APP_IDS:
    per_app('tools-hub', _app, 'bundle_install', _bundle)
    per_app('tools-hub', _app, 'update_rollback_incompatible_hold', _updates)
# Tools installs itself from its own AppImage and never offers to adopt
# itself ("Let Tools manage it" is for the other apps), so it is left out.
for _app in APP_IDS:
    per_app('tools-hub', _app, 'manual_appimage_adoption', _adopt)


@check('tools-hub')
def tools_self_update_stages_and_applies_a_real_appimage(s):
    child = child_session(s, 'self-update')
    try:
        with Releases(child) as server:
            # Tools installs itself the way a user does: run its AppImage.
            source = child.root / 'home/Downloads' / package('arcade.tools').name
            source.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(package('arcade.tools'), source)
            run_app(child, source, '--install', '--silent')
            child.wait_running('arcade.tools')
            old = receipt(child, 'arcade.tools')
            old_hash = hashlib.sha256(Path(old['path']).read_bytes()).hexdigest()
            server.publish('arcade.tools', marker=b'\nArcade e2e self-update fixture\n')
            staged = driver(child, 'self-stage', source=server.url)
            assert Path(staged['file']).exists() and staged['plan']['parentPid'] > 0, staged
            call(child, 'arcade.tools', 'app.quit')
            wait(lambda: not child.endpoint('arcade.tools').exists(), 'Tools did not stop before helper')
            driver(child, 'self-finish', source=server.url, plan=staged['plan'])
            wait(lambda: child.endpoint('arcade.tools').exists(), 'self-update did not restart Tools')
            assert hashlib.sha256(Path(receipt(child, 'arcade.tools')['path']).read_bytes()).hexdigest() != old_hash
            new_hash = hashlib.sha256(Path(receipt(child, 'arcade.tools')['path']).read_bytes()).hexdigest()
            server.publish('arcade.tools', version='99.0.0', protocol=[2])
            try:
                driver(child, 'self-stage', source=server.url)
            except AssertionError as error:
                assert 'protocol' in str(error).lower() or 'incompatible' in str(error).lower(), error
            else:
                raise AssertionError('Tools staged an incompatible self-update')
            assert hashlib.sha256(Path(receipt(child, 'arcade.tools')['path']).read_bytes()).hexdigest() == new_hash
            return 'actual Tools staged/applied a Tools AppImage update in a separate helper process'
    finally:
        child.close()


@check('tools-hub')
def diagnostics_zip_contains_real_app_state_and_redacts_secrets(s):
    fleet(s, FLEET)
    # Running apps may republish their manifests (Box does); stop them so
    # the seeded declarations below are what Tools reads.
    for app in FLEET:
        s.kill(app)
    # Seed a declared log in this profile; it is never an outbound upload.
    for app in FLEET:
        path = s.root / f'arcade/apps/{app}.json'
        manifest = json.loads(path.read_text(encoding='utf-8'))
        logs = s.root / f'home/{app}-logs'
        logs.mkdir(exist_ok=True)
        (logs / 'e2e.log').write_text(f'e2e-log-marker-{app} home={s.env["ARCADE_HOME"]} token=e2e-token-secret password=e2e-password-secret\n', encoding='utf-8')
        manifest.update(logs=str(logs), token='e2e-manifest-secret')
        path.write_text(json.dumps(manifest), encoding='utf-8')
    archive = s.root / 'diagnostics.zip'
    driver(s, 'diagnostics', path=str(archive))
    with zipfile.ZipFile(archive) as z:
        names = z.namelist()
        contents = '\n'.join(z.read(name).decode('utf-8', errors='replace') for name in names)
    assert 'diagnostics.json' in names and any('arcade.find' in name for name in names), names
    for secret in ('e2e-token-secret', 'e2e-password-secret', 'e2e-manifest-secret', s.env['ARCADE_HOME']):
        assert secret not in contents, f'diagnostics leaked {secret}'
    for app in FLEET:
        assert any(name.startswith(f'logs/{app}/') for name in names) and f'e2e-log-marker-{app}' in contents, (app, names)
    return f'{len(names)} real diagnostics ZIP entries; declared logs included, home/token/password redacted'


@check('tools-hub')
def portable_backup_restores_real_app_settings_in_a_second_arcade_home(s):
    # Keep managed installs from earlier checks out of this settings round trip.
    s.stop_apps()
    first = child_session(s, 'backup-source')
    second = child_session(s, 'backup-target')
    try:
        fleet(first)
        tools_preferences(first)
        prefs_path = first.root / TOOLS_DATA / 'preferences.json'
        prefs = json.loads(prefs_path.read_text(encoding='utf-8'))
        prefs.update(channels={'arcade.find': 'nightly'}, updates={'enabled': False, 'autoUpdate': {'arcade.find': True}})
        prefs_path.write_text(json.dumps(prefs), encoding='utf-8')
        originals = {}
        for app in APP_IDS:
            exported = call(first, app, 'app.settings.export')
            assert exported.get('outputs'), (app, exported)
            originals[app] = [export_snapshot(o['path']) for o in exported['outputs']]
        archive = first.root / 'portable.zip'
        saved = driver(first, 'backup', path=str(archive))
        # Only the documented product defects may be left out; every other
        # app must still round-trip before the known failure is raised.
        rejected = {row['appId'] for row in saved['notIncluded'] if row['reason'] == BACKUP_REJECTED and row['appId'] in BACKUP_BUGS}
        assert len(saved['notIncluded']) == len(rejected), saved
        portable = [app for app in APP_IDS if app not in rejected]
        assert saved['includedSettings'] >= len(portable), saved
        with zipfile.ZipFile(archive) as z:
            assert 'backup.json' in z.namelist(), z.namelist()
            backup_document = json.loads(z.read('backup.json'))
            raw = '\n'.join(z.read(name).decode('utf-8', errors='replace') for name in z.namelist())
            assert 'privateKey' not in raw and 'deviceIdentity' not in raw, 'backup includes device secrets'
        # The destination is inside the second isolated root, independent of
        # the original ARCADE_HOME, HOME/XDG roots and all per-app overrides.
        destination = second.root / 'portable.zip'
        shutil.copy2(archive, destination)
        # Native singleton services are scoped to the private D-Bus session.
        # Close the source before opening the destination on that same bus.
        first.close()
        fleet(second)
        result = driver(second, 'restore', path=str(destination))
        restored = result['restored']
        assert restored['toolsSettingsRestored'] and all(a['imported'] for a in restored['apps'] if a['appId'] in portable), result
        assert {a['appId'] for a in restored['apps'] if a['imported']} == set(portable), result
        restored_prefs = json.loads((second.root / TOOLS_DATA / 'preferences.json').read_text(encoding='utf-8'))
        expected_prefs = backup_document['toolsPreferences']
        assert restored_prefs['linkEnabled'] == expected_prefs['linkEnabled'] and restored_prefs['updates']['enabled'] == expected_prefs['updates']['enabled'], restored_prefs
        assert restored_prefs['channels']['arcade.find'] == 'nightly' and restored_prefs['updates']['autoUpdate']['arcade.find'], restored_prefs
        for app in portable:
            exported = call(second, app, 'app.settings.export')
            actual = [export_snapshot(o['path']) for o in exported['outputs']]
            assert actual == originals[app], (app, originals[app], actual)
        if rejected:
            raise AssertionError(f'{BACKUP_REJECTED} Rejected by Tools: {sorted(rejected)}; '
                                 f'{len(portable)} other apps round-tripped: {portable}')
        return f'{len(APP_IDS)} app exports + nondefault Tools preferences → portable ZIP → verified in a second ARCADE_HOME'
    finally:
        first.close()
        second.close()


@check('shortcuts')
def real_shortcut_documents_index_globals_and_in_app_keys(s):
    fleet(s)
    # Look deliberately has no default Linux global. Enable a real binding in
    # this profile so all seven global sources can be exercised.
    look_path = s.root / 'arcade/apps/arcade.look.json'
    if not json.loads(look_path.read_text(encoding='utf-8')).get('shortcuts'):
        call(s, 'arcade.look', 'app.shortcuts.set', {'id': 'preview-selection', 'accelerator': 'Ctrl+Alt+Shift+F9'})
    for app, spec in FLEET.items():
        path = spec['dir'] / 'docs/user/shortcuts.json'
        assert path.is_file(), f'{app}: missing shortcut document {path}'
        validate(json.loads(path.read_text(encoding='utf-8')))
    rebuilt = index(s)
    data = rebuilt['index']
    assert set(APP_IDS) <= {g['app'] for g in data['arcade']}, f'global index incomplete: {data["arcade"]}'
    for app in FLEET:
        sheet = next((a for a in data['apps'] if a['id'] == app), None)
        assert sheet and any(g['context'] != 'global' and g['shortcuts'] for g in sheet['groups']), f'{app}: missing in-app keys: {sheet}'
    return f'{len(APP_IDS)} real manifests + shortcut docs; full Tools index in {rebuilt["elapsedMs"]:.3f} ms'


@check('shortcuts')
def tools_evaluates_hyprland_fixture(s):
    hub_sources('arcade.tools')
    fixture = TOOLS['dir'] / 'crates/arcade-shortcuts/tests/fixtures/hypr/caelestia'
    target = s.root / 'hypr-fixture'
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(fixture, target)
    runtime = TOOLS['dir'] / 'crates/arcade-shortcuts/tests/fixtures/hypr/hyprctl-binds-caelestia.json'
    result = index(s, hyprland=str(target / 'hyprland.lua'), runtime=str(runtime))
    wm = result['index']['wm']
    assert wm['groups'] and not any('limit exceeded' in w for w in wm['warnings']), wm
    count = sum(len(g['shortcuts']) for g in wm['groups'])
    actual = {(entry['chord'], entry.get('submap')) for group in wm['groups'] for entry in group['shortcuts']}
    expected = {(entry['chord'], entry.get('submap')) for entry in result['runtimeBinds'] if entry.get('chord')}
    assert expected <= actual, f'Hyprland runtime chords missing: {expected - actual}'
    return f'{count} Hyprland fixture bindings evaluated and merged by the actual Tools builder'


@check('shortcuts')
def tools_detects_conflicts_between_real_app_manifests(s):
    fleet(s, ['arcade.find', 'arcade.lens'])
    # Deterministic conflict across two real, registered manifest snapshots.
    copies = []
    try:
        for app in ('arcade.find', 'arcade.lens'):
            path = s.root / f'arcade/apps/{app}.json'
            copies.append((path, path.read_bytes()))
            manifest = json.loads(path.read_text(encoding='utf-8'))
            manifest['shortcuts'][0]['accelerator'] = 'Ctrl+Alt+F24'
            path.write_text(json.dumps(manifest), encoding='utf-8')
        conflicts = index(s)['index']['conflicts']
        assert any(c['chord'] == 'Ctrl+Alt+F24' and {c['a']['owner'], c['b']['owner']} == {'arcade.find', 'arcade.lens'} for c in conflicts), conflicts
    finally:
        for path, original in copies:
            path.write_bytes(original)
    return 'actual Tools builder detects the two-app Ctrl+Alt+F24 conflict'


def _rebind_case(app, number):
    def rebind(s):
        s.start(app)
        described = call(s, app, 'describe')
        assert 'app.shortcuts.set' in described.get('methods', []), (app, described.get('methods'))
        path = s.root / f'arcade/apps/{app}.json'
        manifest = json.loads(path.read_text(encoding='utf-8'))
        shortcuts = manifest.get('shortcuts', [])
        assert shortcuts or app == 'arcade.look', f'{app}: no effective global shortcuts in manifest'
        if not shortcuts:
            call(s, app, 'app.shortcuts.set', {'id': 'preview-selection', 'accelerator': 'Ctrl+Alt+Shift+L'})
            wait(lambda: json.loads(path.read_text(encoding='utf-8')).get('shortcuts'), f'{app}: initial binding was not published')
            shortcuts = json.loads(path.read_text(encoding='utf-8'))['shortcuts']
        binding = shortcuts[0]
        original = binding['accelerator']
        changed = 'Ctrl+Alt+Shift+' + chr(ord('A') + number)
        try:
            result = call(s, app, 'app.shortcuts.set', {'id': binding['id'], 'accelerator': changed})
            assert isinstance(result.get('applied'), bool) and result.get('via') in ('native', 'portal', 'hyprland-runtime', 'manual'), (app, result)
            if result['via'] == 'manual':
                assert result.get('hint'), (app, result)
            else:
                assert result['applied'], (app, result)
            wait(lambda: any(v['id'] == binding['id'] and normalize(v['accelerator']) == changed for v in json.loads(path.read_text(encoding='utf-8'))['shortcuts']), f'{app}: manifest did not publish rebind')
        finally:
            call(s, app, 'app.shortcuts.set', {'id': binding['id'], 'accelerator': original})
        # A new process must see the saved binding after restoring it.
        s.kill(app)
        s.start(app)
        saved = next((v['accelerator'] for v in json.loads(path.read_text(encoding='utf-8'))['shortcuts'] if v['id'] == binding['id']), '')
        assert (normalize(saved) if saved else '') == (normalize(original) if original else ''), (app, saved, original)
        return f'{app}/{binding["id"]}: {original} → {changed} → {original}; persisted across relaunch; via={result["via"]}, applied={result["applied"]}'
    rebind.__name__ = app.replace('.', '_') + '_global_shortcut_round_trip'
    rebind.apps = [app]
    return rebind


for _number, _app in enumerate(APP_IDS):
    per_app('shortcuts', _app, 'global_shortcut_round_trip',
            lambda child, app, number=_number: _rebind_case(app, number)(child))


@check('shortcuts')
def real_overlay_paints_its_first_frame_under_xvfb(s):
    from real_bench import overlay_frames
    path = s.root / 'arcade/.cache/arcade/tools/shortcuts-index.json'
    index(s, output=str(path))
    samples = overlay_frames(s, runs=3, hotkey=False)
    assert samples['samplesMs'] and samples['backend'] in ('x11', 'desktop'), samples
    return f'real native overlay first_frame callbacks: {samples}'


def process_tree(root):
    """root and every descendant PID, from /proc."""
    children = {}
    for stat in Path('/proc').glob('[0-9]*/stat'):
        with contextlib.suppress(OSError, ValueError, IndexError):
            fields = stat.read_text().rsplit(')', 1)[1].split()
            children.setdefault(int(fields[1]), []).append(int(stat.parent.name))
    tree, pending = set(), [root]
    while pending:
        pid = pending.pop()
        if pid not in tree:
            tree.add(pid)
            pending.extend(children.get(pid, []))
    return tree


def _shortcut_document(s, app):
    path = FLEET[app]['dir'] / 'docs/user/shortcuts.json'
    if not path.is_file():
        raise SkipCheck(f'{app}: shortcut document unavailable at {path}; provide ARCADE_{app.removeprefix("arcade.").upper()}_REPO')
    document = json.loads(path.read_text(encoding='utf-8'))
    validate(document)
    assert document['app'] == app, document
    assert any(g['context'] == 'global' and g['shortcuts'] for g in document['groups']), document
    assert any(g['context'] != 'global' and g['shortcuts'] for g in document['groups']), document
    s.cli('shortcuts', 'validate', str(path), check=True)
    return f'{app}: global + in-app shortcut document validated by Python and the released Link CLI'


def _own_sni(s, app):
    from sni_watcher import Watcher, until
    binary(app)
    watcher = Watcher()
    try:
        fleet(s, [app])
        pid = call(s, app, 'app.status')['pid']
        expected = FLEET[app]['tray']['own']
        until(lambda: watcher.owner_pids().count(pid) == expected,
              lambda: f'{app}: expected {expected} own SNI item; owners={watcher.owner_pids()}')
        if FLEET[app]['tray']['hostable']:
            assert tray(s, app) == 'own', call(s, app, 'app.status')
        return f'{app}: Link status works; {expected} own StatusNotifierItem (Tools is the host)'
    finally:
        watcher.close()


for _app in FLEET:
    per_app('shortcuts', _app, 'shortcut_document', _shortcut_document)
    per_app('trayhost', _app, 'standalone_status_notifier_item', _own_sni)


@check('shortcuts')
def tools_overlay_shortcut_is_explicitly_fixed(s):
    path = TOOLS['dir'] / 'docs/user/shortcuts.json'
    if not path.is_file():
        raise SkipCheck(f'Tools shortcut document unavailable at {path}')
    shortcut = next(k for g in json.loads(path.read_text())['groups'] if g['context'] == 'global' for k in g['shortcuts'])
    assert shortcut['id'] == 'cheat-sheet' and shortcut['rebindable'] is False, shortcut
    return 'Tools cheat-sheet is documented as nonrebindable; app.shortcuts.set round trips apply to the seven apps'


def _hosted_cycle(s, app):
    from sni_watcher import Watcher, until
    fleet(s, [app])
    binary('arcade.tools')
    s.kill(app)
    watcher = Watcher()
    try:
        tools_preferences(s)
        fleet(s, [app])
        pid = call(s, app, 'app.status')['pid']
        until(lambda: watcher.owner_pids().count(pid) == FLEET[app]['tray']['own'], f'{app}: own icon missing')
        s.start('arcade.tools')
        tools_pid = call(s, 'arcade.tools', 'app.status')['pid']
        until(lambda: tray(s, app) == 'hosted', f'{app}: did not become hosted')
        until(lambda: watcher.owner_pids().count(pid) == FLEET[app]['tray']['hosted'], f'{app}: hosted SNI item remained')
        until(lambda: watcher.owner_pids().count(tools_pid) == 1, 'Tools own host icon missing')
        began = time.perf_counter()
        s.procs['arcade.tools'].kill()
        until(lambda: tray(s, app) == 'own', f'{app}: hand-back exceeded 1 s', timeout=1)
        elapsed = (time.perf_counter() - began) * 1000
        until(lambda: watcher.owner_pids().count(pid) == 1, f'{app}: icon did not return')
        s.kill('arcade.tools')
        tools_preferences(s, excluded=[app])
        s.start('arcade.tools')
        # Establish a real live Tools host before accepting the opt-out state.
        call(s, 'arcade.tools', 'app.status')
        until(lambda: tray(s, app) == 'own' and watcher.owner_pids().count(pid) == 1, f'{app}: opt-out lost own icon')
        return f'{app}: own SNI → hosted removal; Tools host SNI present; SIGKILL → own in {elapsed:.3f} ms; per-app opt-out'
    finally:
        watcher.close()


def _link_off(s, app):
    from sni_watcher import Watcher, until
    binary('arcade.tools')
    fleet(s, [app])  # Initializes only this disposable app's native settings.
    s.kill(app)
    if app == 'arcade.box':
        with sqlite3.connect(s.root / 'arcade/dev.arcadebox.app/arcade.sqlite3') as db:
            db.execute("INSERT OR REPLACE INTO settings(key,value) VALUES('link_enabled','false')")
    elif app == 'arcade.shelf':
        with sqlite3.connect(s.root / 'shelf/shelf.sqlite3') as db:
            row = db.execute("SELECT value FROM meta WHERE key='settings'").fetchone()
            value = json.loads(row[0]) if row else {}
            value['linkEnabled'] = False
            db.execute("INSERT OR REPLACE INTO meta(key,value) VALUES('settings',?)", (json.dumps(value),))
    elif app == 'arcade.lens':
        path = s.root / 'lens/config/settings.toml'
        path.parent.mkdir(parents=True, exist_ok=True)
        # Default values plus an explicit disabled Link section; no real config
        # is read or serialized through a third-party TOML writer.
        path.write_text('[link]\nenabled = false\n', encoding='utf-8')
    else:
        paths = {
            'arcade.find': ('find/config/settings.json', {'schema': 1, 'roots': [str(s.root / 'home')], 'link': {'enabled': False}}),
            'arcade.look': ('config/arcade-look/config.json', {'linkEnabled': False}),
            'arcade.wheel': ('config/Arcade Wheel/Arcade Wheel/config.json', {'link': {'enabled': False}}),
            'arcade.clipboard': ('data/dev.arcade.clipboard/shared_preferences.json', {'flutter.link_enabled': False}),
        }
        relative, changes = paths[app]
        path = s.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        existing = json.loads(path.read_text()) if path.is_file() else {}
        existing.update(changes)
        path.write_text(json.dumps(existing), encoding='utf-8')
    tools_preferences(s)
    watcher = Watcher()
    try:
        s.start('arcade.tools')
        process = s.start(app, wait=False)
        # Under APPIMAGE_EXTRACT_AND_RUN the app is a child of the runtime PID.
        until(lambda: sum(pid in process_tree(process.pid) for pid in watcher.owner_pids()) == 1,
              lambda: f'{app}: Link-off own icon missing; {s.log(app)}')
        manifest_path = s.root / f'arcade/apps/{app}.json'
        until(lambda: manifest_path.exists() and json.loads(manifest_path.read_text())['settings']['linkEnabled'] is False,
              f'{app}: disabled Link was not published')
        if s.endpoint(app).exists():
            endpoint = json.loads(s.endpoint(app).read_text())
            assert not Path(f'/proc/{endpoint["pid"]}').exists(), f'{app}: disabled Link still has a live endpoint owner'
        return f'{app}: native Link preference disabled; own SNI while Tools runs; no live Link endpoint (a SIGKILL may leave a stale file)'
    finally:
        watcher.close()


for _app in APP_IDS:
    per_app('trayhost', _app, 'hosting_crash_opt_out', _hosted_cycle)
    per_app('trayhost', _app, 'link_disabled_own_icon', _link_off)


def _diagnostics_one(s, app):
    fleet(s, [app])
    # A running app may republish its manifest; stop it so the seeded
    # declarations below are what Tools reads.
    s.kill(app)
    path = s.root / f'arcade/apps/{app}.json'
    manifest = json.loads(path.read_text())
    logs = s.root / 'home/logs'
    logs.mkdir(exist_ok=True)
    (logs / 'e2e.log').write_text(f'log-marker-{app} home={s.env["ARCADE_HOME"]} token=e2e-token-secret password=e2e-password-secret\n')
    manifest.update(logs=str(logs), token='e2e-manifest-secret')
    path.write_text(json.dumps(manifest))
    archive = s.root / 'diagnostics.zip'
    driver(s, 'diagnostics', path=str(archive))
    with zipfile.ZipFile(archive) as z:
        names = z.namelist()
        raw = '\n'.join(z.read(name).decode('utf-8', errors='replace') for name in names)
    assert f'manifests/{app}.json' in names and any(name.startswith(f'logs/{app}/') for name in names), names
    assert f'log-marker-{app}' in raw, names
    for secret in ('e2e-token-secret', 'e2e-password-secret', 'e2e-manifest-secret', s.env['ARCADE_HOME']):
        assert secret not in raw, f'{app}: diagnostics leaked {secret}'
    return f'{app}: real manifest + declared logs in Tools ZIP; home/token/password redacted'


def _backup_one(s, app):
    first, second = child_session(s, 'source'), child_session(s, 'target')
    try:
        fleet(first, [app])
        manifest = json.loads((first.root / f'arcade/apps/{app}.json').read_text())
        shortcuts = manifest.get('shortcuts', [])
        binding = shortcuts[0]['id'] if shortcuts else 'preview-selection'
        # A key every X keymap has (Xvfb maps no F23 keycode).
        call(first, app, 'app.shortcuts.set', {'id': binding, 'accelerator': 'Ctrl+Alt+Shift+F10'})
        exported = call(first, app, 'app.settings.export')
        assert exported.get('outputs'), (app, exported)
        originals = [export_snapshot(o['path']) for o in exported['outputs']]
        archive = first.root / 'portable.zip'
        saved = driver(first, 'backup', path=str(archive))
        assert saved['includedSettings'] >= 1 and not saved['notIncluded'], (app, saved, exported)
        with zipfile.ZipFile(archive) as z:
            assert 'backup.json' in z.namelist(), z.namelist()
            raw = '\n'.join(z.read(name).decode('utf-8', errors='replace') for name in z.namelist())
            assert 'privateKey' not in raw and 'deviceIdentity' not in raw, 'device secrets in portable backup'
        destination = second.root / 'portable.zip'
        shutil.copy2(archive, destination)
        first.close()
        fleet(second, [app])
        restored = driver(second, 'restore', path=str(destination))['restored']
        rows = [row for row in restored['apps'] if row['appId'] == app]
        assert len(rows) == 1 and rows[0]['imported'], restored
        exported = call(second, app, 'app.settings.export')
        actual = [export_snapshot(o['path']) for o in exported['outputs']]
        assert actual == originals, (app, originals, actual)
        return f'{app}: nondefault binding in real exported settings → Tools ZIP → exact export match in a second ARCADE_HOME'
    finally:
        first.close()
        second.close()


for _app in FLEET:
    per_app('tools-hub', _app, 'diagnostics_redaction', _diagnostics_one)
for _app in APP_IDS:
    per_app('tools-hub', _app, 'portable_settings_restore', _backup_one, known=BACKUP_BUGS.get(_app))


@check('tools-hub')
def tools_preferences_restore_without_any_app_binary(s):
    first, second = child_session(s, 'tools-prefs-source'), child_session(s, 'tools-prefs-target')
    try:
        tools_preferences(first)
        path = first.root / TOOLS_DATA / 'preferences.json'
        preferences = json.loads(path.read_text())
        # Tools intentionally restores connection/update policy, channels and
        # shortcut labels. Theme/notifications/onboarding are not portable.
        preferences.update(channels={'arcade.find': 'nightly'}, updates={'enabled': False, 'autoUpdate': {'arcade.find': True}})
        path.write_text(json.dumps(preferences))
        archive = first.root / 'preferences.zip'
        driver(first, 'backup', path=str(archive))
        with zipfile.ZipFile(archive) as z:
            document = json.loads(z.read('backup.json'))
        destination = second.root / 'preferences.zip'
        shutil.copy2(archive, destination)
        restored = driver(second, 'restore', path=str(destination))['restored']
        assert restored['toolsSettingsRestored'], restored
        actual = json.loads((second.root / TOOLS_DATA / 'preferences.json').read_text())
        saved = document['toolsPreferences']
        assert actual['linkEnabled'] == saved['linkEnabled'] and actual['updates']['enabled'] == saved['updates']['enabled'], (preferences, actual)
        assert actual['channels']['arcade.find'] == 'nightly' and actual['updates']['autoUpdate']['arcade.find'], actual
        return 'actual Tools core: Link/update policy + nondefault channel/auto-update restored across two homes; theme/notification/onboarding are outside the portable subset'
    finally:
        first.close()
        second.close()
