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
import subprocess
import time
import zipfile

from real_apps import (Peer, Releases, call, child_session, driver, fleet, index,
                       package, receipt, run_app, tools_preferences, tray, wait)
from validate_shortcuts import normalize, validate


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

            # Explicit install without --no-launch must re-exec from the durable
            # package. Its manifest, not an inferred launcher, proves the path.
            shutil.copy2(artifact, source)
            settings = child.root / 'find/config/settings.json'
            settings.parent.mkdir(parents=True, exist_ok=True)
            settings.write_text(json.dumps({'schema': 1, 'roots': [str(child.root / 'home')]}), encoding='utf-8')
            with (child.root / f'{app}.log').open('ab') as log:
                p = subprocess.Popen([str(source), '--install', '--silent', '--background'],
                                     env=dict(child.env, APPIMAGE_EXTRACT_AND_RUN='1'), stdout=log, stderr=log, start_new_session=True)
            child.procs[app] = p
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
    return lifecycle


for _app in APP_IDS:
    check('install')(_install_case(_app))


@check('install')
def development_builds_never_offer_an_install(s):
    fleet(s)
    for app in APP_IDS:
        spec = APPS[app]
        # This deliberately invokes the development executable without APPIMAGE.
        result = run_app(s, require_current_build(spec), '--integration-status')
        status = json.loads(result.stdout)
        assert status.get('receipt') is None, (app, status)
        manifest = json.loads(run_app(s, require_current_build(spec), '--arcade-manifest').stdout)
        assert manifest.get('install') == 'dev', (app, manifest.get('install'))
        assert not (s.root / f'arcade/installs/{app}.json').exists(), app
    assert not s.xdotool('search', '--onlyvisible', '--name', 'Install Arcade'), 'development build displayed an install dialog'
    return f'{len(APP_IDS)} real development apps: install=dev, no receipt or install dialog'


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
        wait(lambda: tray(s, app) == 'hosted', f'{app}: app started after Tools did not become hosted')
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
def disabling_link_in_find_keeps_its_own_icon(s):
    s.kill('arcade.find')
    path = s.root / 'find/config/settings.json'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({'schema': 1, 'roots': [str(s.root / 'home')], 'link': {'enabled': False}}), encoding='utf-8')
    s.kill('arcade.tools')
    tools_preferences(s)
    s.start('arcade.tools')
    s.start('arcade.find', wait=False)
    binary = require_current_build(APPS['arcade.find'])
    def own():
        result = run_app(s, binary, '--status', check=False)
        return json.loads(result.stdout) if result.returncode == 0 else None
    status = wait(own, 'Find with Link disabled did not start')
    assert not s.endpoint('arcade.find').exists(), status
    # The native status includes tray even with Link switched off.
    assert status.get('tray') == 'own' or status.get('link', {}).get('tray') == 'own', status
    return 'real Find, disabled Link: own tray, no Link endpoint while Tools hosts'


def _operate(s, server, app, operation):
    return driver(s, 'operate', source=server.url, request={'id': app, 'operation': operation, 'mode': 'background'})


@check('tools-hub')
def loopback_catalog_bundle_install_real_packages(s):
    with Releases(s) as server:
        for app in ('arcade.find', 'arcade.shelf'):
            server.publish(app)
        result = driver(s, 'catalog', source=server.url)
        assert result['updated'] == server.catalog['updated'] and '/catalog.json' in server.requests, (result, server.requests)
        result = driver(s, 'bundle', source=server.url, bundle='e2e')
        assert len(result) == 2, result
        for app in ('arcade.find', 'arcade.shelf'):
            installed = receipt(s, app)
            assert installed['managedBy'] == 'tools' and Path(installed['path']).is_file(), installed
            s.wait_running(app)
        return 'actual Tools core fetched loopback catalog and installed the Find + Shelf bundle via real app installers'


@check('tools-hub')
def tools_updates_rolls_back_and_holds_incompatible_packages(s):
    with Releases(s) as server:
        app = 'arcade.find'
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
        assert 'held' in json.dumps(state).lower() or 'incompatible' in json.dumps(state).lower(), state
        try:
            _operate(s, server, app, 'update')
        except AssertionError as error:
            assert 'protocol' in str(error).lower() or 'incompatible' in str(error).lower(), error
        else:
            raise AssertionError('Tools applied an incompatible release')
        assert hashlib.sha256(path.read_bytes()).hexdigest() == initial
        return 'real Find bytes updated → user rollback; protocol-2 update held before replacement'


@check('tools-hub')
def tools_adopts_a_manual_appimage_and_self_updates(s):
    child = child_session(s, 'adopt-self')
    try:
        with Releases(child) as server:
            app = 'arcade.lens'
            source = child.root / 'home/Manual-Lens.AppImage'
            shutil.copy2(package(app), source)
            run_app(child, source, '--install', '--silent', '--no-launch')
            manifest = json.loads(run_app(child, Path(receipt(child, app)['path']), '--arcade-manifest').stdout)
            registry = child.root / f'arcade/apps/{app}.json'
            registry.parent.mkdir(parents=True, exist_ok=True)
            registry.write_text(json.dumps(manifest), encoding='utf-8')
            assert receipt(child, app)['managedBy'] == 'self'
            _operate(child, server, app, 'adopt')
            assert receipt(child, app)['managedBy'] == 'tools'
            server.publish('arcade.tools')
            _operate(child, server, 'arcade.tools', 'install')
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
            return 'actual Tools adopted a self-managed Lens AppImage and staged/applied a Tools update in a separate helper process'
    finally:
        child.close()


@check('tools-hub')
def diagnostics_zip_contains_real_app_state_and_redacts_secrets(s):
    fleet(s)
    # Seed a declared log in this profile; it is never an outbound upload.
    path = s.root / 'arcade/apps/arcade.find.json'
    manifest = json.loads(path.read_text(encoding='utf-8'))
    logs = s.root / 'home/find-logs'
    logs.mkdir(exist_ok=True)
    (logs / 'e2e.log').write_text(f'home={s.env["HOME"]} token=e2e-token-secret password=e2e-password-secret\n', encoding='utf-8')
    manifest.update(logs=str(logs), token='e2e-manifest-secret')
    path.write_text(json.dumps(manifest), encoding='utf-8')
    archive = s.root / 'diagnostics.zip'
    driver(s, 'diagnostics', path=str(archive))
    with zipfile.ZipFile(archive) as z:
        names = z.namelist()
        contents = '\n'.join(z.read(name).decode('utf-8', errors='replace') for name in names)
    assert 'diagnostics.json' in names and any('arcade.find' in name for name in names), names
    for secret in ('e2e-token-secret', 'e2e-password-secret', 'e2e-manifest-secret', s.env['HOME']):
        assert secret not in contents, f'diagnostics leaked {secret}'
    assert any('e2e.log' in name for name in names), names
    return f'{len(names)} real diagnostics ZIP entries; declared logs included, home/token/password redacted'


@check('tools-hub')
def portable_backup_restores_real_app_settings_in_a_second_arcade_home(s):
    # Keep managed installs from earlier checks out of this settings round trip.
    first = child_session(s, 'backup-source')
    second = child_session(s, 'backup-target')
    try:
        fleet(first)
        fleet(second)
        originals = {}
        for app in APP_IDS:
            exported = call(first, app, 'app.settings.export')
            assert exported.get('outputs'), (app, exported)
            originals[app] = [json.loads(Path(o['path']).read_text(encoding='utf-8')) for o in exported['outputs']]
        archive = first.root / 'portable.zip'
        saved = driver(first, 'backup', path=str(archive))
        assert saved['includedSettings'] >= len(APP_IDS), saved
        assert not saved['notIncluded'], saved
        with zipfile.ZipFile(archive) as z:
            assert 'backup.json' in z.namelist(), z.namelist()
            raw = '\n'.join(z.read(name).decode('utf-8', errors='replace') for name in z.namelist())
            assert 'privateKey' not in raw and 'deviceIdentity' not in raw, 'backup includes device secrets'
        # The destination is inside the second isolated root, independent of
        # the original ARCADE_HOME, HOME/XDG roots and all per-app overrides.
        destination = second.root / 'portable.zip'
        shutil.copy2(archive, destination)
        result = driver(second, 'restore', path=str(destination))
        restored = result['restored']
        assert restored['toolsSettingsRestored'] and all(a['imported'] for a in restored['apps']), result
        assert {a['appId'] for a in restored['apps']} == set(APP_IDS), result
        for app in APP_IDS:
            exported = call(second, app, 'app.settings.export')
            actual = [json.loads(Path(o['path']).read_text(encoding='utf-8')) for o in exported['outputs']]
            # Paths within one profile are not portable identity. Normalize only
            # those two known temporary prefixes; all setting values still match.
            normalize_root = lambda value, child: json.loads(json.dumps(value).replace(str(child.root), '$PROFILE'))
            assert normalize_root(actual, second) == normalize_root(originals[app], first), (app, originals[app], actual)
        return f'{len(APP_IDS)} real app exports → actual Tools portable ZIP → imports in a second isolated ARCADE_HOME'
    finally:
        first.close()
        second.close()


@check('shortcuts')
def real_shortcut_documents_index_globals_and_in_app_keys(s):
    fleet(s)
    for app, spec in APPS.items():
        path = spec['dir'] / 'docs/user/shortcuts.json'
        assert path.is_file(), f'{app}: missing shortcut document {path}'
        validate(json.loads(path.read_text(encoding='utf-8')))
    rebuilt = index(s)
    data = rebuilt['index']
    assert set(APP_IDS) <= {g['app'] for g in data['arcade']}, f'global index incomplete: {data["arcade"]}'
    for app in APP_IDS:
        sheet = next((a for a in data['apps'] if a['id'] == app), None)
        assert sheet and any(g['context'] != 'global' and g['shortcuts'] for g in sheet['groups']), f'{app}: missing in-app keys: {sheet}'
    return f'{len(APP_IDS)} real manifests + shortcut docs; full Tools index in {rebuilt["elapsedMs"]:.3f} ms'


@check('shortcuts')
def tools_evaluates_hyprland_fixture_and_detects_conflicts(s):
    fleet(s)
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
    assert count >= len(json.loads(runtime.read_text(encoding='utf-8'))), (count, wm)
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
    return f'{count} Hyprland fixture bindings; actual Tools builder detects two-app conflict'


def _rebind_case(app, number):
    def rebind(s):
        s.start(app)
        described = call(s, app, 'describe')
        assert 'app.shortcuts.set' in described.get('methods', []), (app, described.get('methods'))
        path = s.root / f'arcade/apps/{app}.json'
        manifest = json.loads(path.read_text(encoding='utf-8'))
        shortcuts = manifest.get('shortcuts', [])
        assert shortcuts, f'{app}: no effective global shortcuts in manifest'
        binding = shortcuts[0]
        original = binding['accelerator']
        changed = 'Ctrl+Alt+F' + str(13 + number)
        try:
            result = call(s, app, 'app.shortcuts.set', {'id': binding['id'], 'accelerator': changed})
            assert result.get('applied') and result.get('via') in ('native', 'portal', 'hyprland-runtime', 'manual'), (app, result)
            if result['via'] == 'manual':
                assert result.get('hint'), (app, result)
            wait(lambda: any(v['id'] == binding['id'] and normalize(v['accelerator']) == changed for v in json.loads(path.read_text(encoding='utf-8'))['shortcuts']), f'{app}: manifest did not publish rebind')
        finally:
            call(s, app, 'app.shortcuts.set', {'id': binding['id'], 'accelerator': original})
        # A new process must see the saved binding after restoring it.
        s.kill(app)
        s.start(app)
        assert any(v['id'] == binding['id'] and normalize(v['accelerator']) == normalize(original) for v in json.loads(path.read_text(encoding='utf-8'))['shortcuts'])
        return f'{app}/{binding["id"]}: {original} → {changed} → {original}; persisted across relaunch'
    rebind.__name__ = app.replace('.', '_') + '_global_shortcut_round_trip'
    return rebind


for _number, _app in enumerate(APP_IDS):
    check('shortcuts')(_rebind_case(_app, _number))


@check('shortcuts')
def real_overlay_paints_its_first_frame_under_xvfb(s):
    from real_bench import overlay_frames
    fleet(s)
    path = s.root / 'arcade/cache/arcade/tools/shortcuts-index.json'
    index(s, output=str(path))
    samples = overlay_frames(s, runs=3, hotkey=False)
    assert samples['samplesMs'] and samples['backend'] == 'x11', samples
    return f'real native overlay first_frame callbacks: {samples}'
