"""Real fleet adapters shared by the ecosystem checks and production probes."""
from __future__ import annotations

import contextlib
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import threading
import time

import e2e

DRIVER = e2e.LINK / 'target/real-driver/debug/arcade-ecosystem-driver'


def driver_inputs():
    repo = e2e.TOOLS['dir']
    paths = [repo / 'Cargo.toml', repo / 'Cargo.lock', e2e.LINK / 'tools/real_app_driver.rs']
    for root in (repo / 'src', repo / 'crates/arcade-shortcuts', repo / 'resources'):
        paths.extend(p for p in root.rglob('*') if p.is_file() and 'target' not in p.parts and p.suffix in {'.rs', '.toml', '.json'})
    return {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(set(paths))}


def build_driver():
    """Build real Tools libraries from the selected checkout, without editing it."""
    repo = e2e.TOOLS['dir']
    if not (repo / 'crates/arcade-shortcuts/Cargo.toml').is_file():
        raise AssertionError(f'stale or missing build: real Tools hub sources at {repo}; select ARCADE_TOOLS_REPO')
    project = e2e.LINK / 'target/real-driver-project'
    project.mkdir(parents=True, exist_ok=True)
    manifest = '\n'.join([
        '[package]', 'name = "arcade-ecosystem-driver"', 'version = "0.0.0"', 'edition = "2021"',
        '[workspace]', '[dependencies]', 'serde_json = "1"',
        'arcade-tools-core = { path = ' + json.dumps(str(repo)) + ' }',
        'arcade-shortcuts = { path = ' + json.dumps(str(repo / 'crates/arcade-shortcuts')) + ' }',
        '[[bin]]', 'name = "arcade-ecosystem-driver"',
        'path = ' + json.dumps(str(e2e.LINK / 'tools/real_app_driver.rs')), '',
    ])
    (project / 'Cargo.toml').write_text(manifest, encoding='utf-8')
    # Use the fleet's pins, then let Cargo add only the adapter's workspace entry.
    lock = repo / 'Cargo.lock'
    if lock.exists():
        shutil.copyfile(lock, project / 'Cargo.lock')
    inputs = driver_inputs()
    subprocess.run(['cargo', 'build', '--manifest-path', str(project / 'Cargo.toml'),
                    '--target-dir', str(e2e.LINK / 'target/real-driver')], check=True, cwd=e2e.LINK)
    assert inputs == driver_inputs(), 'stale or missing build: Tools adapter inputs changed during compilation; rebuild'
    DRIVER.with_suffix('.inputs.json').write_text(json.dumps(inputs), encoding='utf-8')


def driver(s, command, **params):
    assert DRIVER.is_file(), f'stale or missing build: {DRIVER}; run tools/e2e.py --real-apps to build the Tools adapter'
    stamp = DRIVER.with_suffix('.inputs.json')
    assert stamp.is_file() and json.loads(stamp.read_text(encoding='utf-8')) == driver_inputs(), f'stale or missing build: {DRIVER}; Tools core/shortcut inputs changed; rebuild the adapter'
    result = subprocess.run([str(DRIVER)], input=json.dumps({'command': command, **params}), env=s.env,
                            capture_output=True, text=True, encoding='utf-8', timeout=180)
    (s.root / 'tools-driver.log').open('a', encoding='utf-8').write(result.stderr)
    assert result.returncode == 0, f'Tools core {command}: {result.stdout}{result.stderr}'
    return json.loads(result.stdout)


def wait(predicate, message, timeout=15, interval=.02):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = predicate()
        if last:
            return last
        time.sleep(interval)
    raise AssertionError(f'{message() if callable(message) else message}; last={last!r}')


class Peer:
    """An authenticated connection to a real app; bounded test-time probes only."""
    def __init__(self, s, app):
        endpoint = json.loads(s.endpoint(app).read_text(encoding='utf-8'))
        self.socket = socket.socket(socket.AF_UNIX)
        self.socket.settimeout(2)
        self.socket.connect(endpoint['address'])
        self.reader = self.socket.makefile('rb')
        self.id = 0
        self.call('hello', {'token': endpoint['token'], 'client': {'id': 'arcade.tools', 'version': 'e2e'}, 'protocol': [1]})

    def call(self, method, params=None):
        self.id += 1
        self.socket.sendall((json.dumps({'v': 1, 'id': self.id, 'method': method, 'params': params or {}}) + '\n').encode())
        while True:
            line = self.reader.readline()
            assert line, f'{method}: real app closed the connection'
            reply = json.loads(line)
            if reply.get('id') != self.id:
                continue
            assert 'error' not in reply, f'{method}: {reply.get("error")}'
            return reply['result']

    def close(self):
        self.reader.close()
        self.socket.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def call(s, app, method, params=None):
    with Peer(s, app) as peer:
        return peer.call(method, params)


def tray(s, app):
    return call(s, app, 'app.status').get('status', {}).get('tray')


def fleet(s):
    # Find never scans beyond the throwaway profile.
    settings = s.root / 'find/config/settings.json'
    settings.parent.mkdir(parents=True, exist_ok=True)
    if not settings.exists():
        settings.write_text(json.dumps({'schema': 1, 'roots': [str(s.root / 'home')]}), encoding='utf-8')
    failures = []
    for app in e2e.APP_IDS:
        try:
            s.start(app)
        except Exception as error:
            failures.append(f'{app}: {error}')
    assert not failures, '\n'.join(failures)


def tools_preferences(s, **values):
    path = s.root / 'arcade/.local/share/arcade-tools/tray-preferences.json'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({'hostIcons': True, 'startAtLogin': False, 'shortcutsEnabled': True, **values}), encoding='utf-8')
    # Keep the idle samples away from update-network work.
    (path.parent / 'preferences.json').write_text(json.dumps({'linkEnabled': True, 'updates': {'enabled': False}}), encoding='utf-8')


def package(app):
    spec = e2e.TOOLS if app == 'arcade.tools' else e2e.APPS[app]
    prefix = 'ARCADE_' + app.removeprefix('arcade.').upper()
    override = os.environ.get(prefix + '_APPIMAGE')
    candidates = sorted((spec['dir'] / 'dist').glob('*.AppImage'))
    candidates += sorted((spec['dir'] / 'src-tauri/target/release/bundle/appimage').glob('*.AppImage'))
    candidates += sorted((spec['dir'] / 'target/release/bundle/appimage').glob('*.AppImage'))
    cache = Path(os.environ.get('ARCADE_ARTIFACT_DIR', str(e2e.CODING.parent.parent / '.cache/arcade-orchestrator/artifacts')))
    slug = app.replace('.', '').replace('-', '')
    candidates += [p for p in cache.rglob('*.AppImage') if ''.join(c for c in p.name.lower() if c.isalnum()).startswith(slug)]
    path = Path(override).resolve() if override else max(candidates, key=lambda p: p.stat().st_mtime_ns, default=None)
    assert path, f'stale or missing build: {app} AppImage in {spec["dir"]}/dist; build the Linux package or set {prefix}_APPIMAGE'
    provenance = path.with_suffix(path.suffix + '.provenance.json')
    if provenance.is_file():
        identity = json.loads(provenance.read_text(encoding='utf-8'))
        assert identity['conclusion'] == 'success', f'stale or missing build: {path}; CI run was not green'
        assert hashlib.sha256(path.read_bytes()).hexdigest() == identity['sha256'], f'stale or missing build: {path}; CI checksum mismatch'
        changed = subprocess.run(['git', 'diff', '--name-only', identity['headSha'], '--'], cwd=spec['dir'], capture_output=True, text=True, check=True)
        inputs = [name for name in changed.stdout.splitlines() if e2e.source_input(Path(name))]
        assert not inputs, f'stale or missing build: {path}; source changes since CI {identity["headSha"]}: {inputs}'
        assert os.access(path, os.X_OK), f'stale or missing build: {path}; not executable'
    else:
        e2e.require_current_build({**spec, 'bin': str(path), 'build': ['download a green Linux CI AppImage']})
    signature = path.open('rb').read(11)
    assert signature[:4] == b'\x7fELF' and signature[8:11] in (b'AI\x01', b'AI\x02'), f'{path}: expected a real AppImage (shell/mock wrappers are not accepted)'
    return path


def run_app(s, binary, *args, check=True):
    env = dict(s.env, APPIMAGE_EXTRACT_AND_RUN='1')
    r = subprocess.run([str(binary), *args], env=env, capture_output=True, text=True, encoding='utf-8', timeout=60)
    if check:
        assert r.returncode == 0, f'{binary} {args}: {r.stdout}{r.stderr}'
    return r


def receipt(s, app):
    return json.loads((s.root / f'arcade/installs/{app}.json').read_text(encoding='utf-8'))


def child_session(s, name):
    child = e2e.Session(s.root / name, headless=True, real_apps=True)
    child.env.update({key: s.env[key] for key in ('DISPLAY', 'DBUS_SESSION_BUS_ADDRESS') if key in s.env})
    return child


def index(s, **extra):
    return driver(s, 'index', docs=[str(a['dir'] / 'docs/user/shortcuts.json') for a in e2e.APPS.values()],
                  catalog=str(e2e.CODING / 'Rust/Arcade-catalog/shortcuts'), **extra)


class Releases:
    """Fixture HTTP metadata and real packaged executables, SHA/size verified by Tools."""
    def __init__(self, s):
        self.routes = {}
        self.requests = []
        owner = self
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                owner.requests.append(self.path)
                body = owner.routes.get(self.path)
                self.send_response(200 if body is not None else 404)
                self.send_header('Content-Length', str(len(body or b'')))
                self.end_headers()
                with contextlib.suppress(BrokenPipeError, ConnectionResetError):
                    self.wfile.write(body or b'')
            def log_message(self, *_):
                pass
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = f'http://127.0.0.1:{self.server.server_port}/'
        catalog = json.loads((e2e.TOOLS['dir'] / 'resources/catalog-snapshot/catalog.json').read_text(encoding='utf-8'))
        catalog['updated'] = '2026-10-12T00:00:00Z'
        catalog['bundles'] = [{'id': 'e2e', 'title': 'Real reference apps', 'description': 'Isolated Find and Shelf', 'apps': ['arcade.find', 'arcade.shelf']}]
        self.catalog = catalog
        self.routes['/catalog.json'] = json.dumps(catalog).encode()
        self.previous = {key: s.env.get(key) for key in ('ARCADE_TOOLS_TEST_SOURCE', 'ARCADE_TOOLS_CATALOG_URL', 'APPIMAGE_EXTRACT_AND_RUN')}
        self.s = s
        s.env.update(ARCADE_TOOLS_TEST_SOURCE=self.url, ARCADE_TOOLS_CATALOG_URL=self.url + 'catalog.json', APPIMAGE_EXTRACT_AND_RUN='1')

    def publish(self, app, version=None, protocol=None, marker=b''):
        artifact = package(app)
        if version is None:
            manifest = run_app(self.s, artifact, '--arcade-manifest')
            version = json.loads(manifest.stdout)['version']
        entry = self.catalog['tools'] if app == 'arcade.tools' else next(a for a in self.catalog['apps'] if a['id'] == app)
        body = artifact.read_bytes() + marker
        name = artifact.name
        prefix = f'assets/{app}/{version}/{hashlib.sha256(body).hexdigest()[:12]}/'
        asset = {'os': 'linux', 'arch': 'x64', 'kind': 'appimage', 'file': name,
                 'sha256': hashlib.sha256(body).hexdigest(), 'size': len(body),
                 'installArgs': ['--install', '--silent']}
        release = {'schema': 1, 'id': app, 'version': version, 'channel': 'stable', 'linkProtocol': protocol or [1],
                   'notes': f'https://github.com/{entry["repo"]}/releases', 'assets': [asset]}
        assets = {name: body, 'arcade-release.json': json.dumps(release).encode(), 'SHA256SUMS.txt': f'{asset["sha256"]}  {name}\n'.encode()}
        api_assets = []
        for filename, content in assets.items():
            route = prefix + filename
            self.routes['/' + route] = content
            api_assets.append({'name': filename, 'browser_download_url': self.url + route})
        self.routes[f'/repos/{entry["repo"]}/releases/latest'] = json.dumps({'tag_name': 'v' + version, 'draft': False, 'prerelease': False, 'assets': api_assets}).encode()
        return release

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=3)
        for key, value in self.previous.items():
            if value is None:
                self.s.env.pop(key, None)
            else:
                self.s.env[key] = value

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
