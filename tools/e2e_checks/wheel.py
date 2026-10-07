"""Wheel's real Settings confirmation, cancellation, and consumer flows."""
import json
import os
import signal
import sqlite3
import subprocess
import sys
import time
from pathlib import Path

REPO = APPS['arcade.wheel']['dir']
FIXTURES = REPO / 'tests/fixtures'


def _wait(predicate, message, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.05)
    raise AssertionError(message)


def _status(s):
    return json.loads(_wheel_command(s, '--status').stdout)


def _calls(path, action=None):
    rows = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    return [row for row in rows if action is None or row['action'] == action]


def _mock(s, app, fixture, log):
    s.kill(app, signal.SIGTERM)
    if isinstance(fixture, dict):
        path = s.root / f'{app}-wheel-fixture.json'
        path.write_text(json.dumps(fixture))
    else:
        path = fixture
    p = subprocess.Popen([str(CLI), 'mock', '--as', app, '--actions', str(path)],
                         env=dict(s.env, ARCADE_MOCK_LOG=str(log)), stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, start_new_session=True)
    s.procs[app] = p
    s.wait_running(app)
    return p


def _wheel_command(s, command):
    spec = APPS['arcade.wheel']
    return subprocess.run([str(spec['dir'] / spec['bin']), command], env=s.env,
                          capture_output=True, text=True, timeout=10)


def _config(s):
    paths = list((s.root / 'config' / 'Arcade Wheel').rglob('config.json'))
    assert len(paths) == 1, paths
    return paths[0]


def _click(s, win, x, y):
    s.xdotool('windowraise', win, 'windowfocus', win, 'mousemove', '--window', win, str(x), str(y), 'click', '1')
    time.sleep(.2)


def _place(s, win, shot=None):
    # The incoming draft is already open. Choosing a slot enables its single
    # confirmation button; neither the choice nor a pending draft writes data.
    s.xdotool('windowsize', win, '1240', '820')
    _click(s, win, 825, 187)
    s.xdotool('key', 'Home', 'Return')
    time.sleep(.4)
    s.screenshot(shot or 'wheel-placement-current', win)
    _click(s, win, 960, 757)   # Add to selected slot


def _bind(s, app, action, input_mode='none', options=None, title='Wheel test', shot=None):
    request = {'app': app, 'action': action, 'version': 1, 'title': title,
               'input': input_mode, 'options': options or {}}
    add = s.invoke('wheel', 'wheel.add_action', '--input-json',
                   json.dumps({'type': 'structured/arcade-action', 'data': request}), background=True)
    win = s.wait_window('Arcade Wheel.*Settings')
    time.sleep(.3)
    _place(s, win, shot)
    out, err = add.communicate(timeout=15)
    assert add.returncode == 0, out + err
    saved = json.loads(_config(s).read_text())['decks'][0]['actions'][0]
    assert saved['payload']['action'] == action and saved['payload']['options'] == (options or {}), saved
    return win


def _run_slot(s):
    width, height = map(int, s.xdotool('getdisplaygeometry').split())
    s.xdotool('mousemove', str(width // 2), str(height // 2))
    _wheel_command(s, '--cancel')
    assert _wheel_command(s, '--show').returncode == 0
    _wait(lambda: _status(s)['overlayRevealed'], 'Wheel overlay did not reveal')
    # Xvfb has no window manager to honor WindowStaysOnTopHint for a reused
    # surface. Raise Wheel's full-screen window explicitly in the private session.
    for win in s.xdotool('search', '--onlyvisible', '--pid', str(s.procs['arcade.wheel'].pid)).splitlines():
        geom = dict(line.split('=', 1) for line in s.xdotool('getwindowgeometry', '--shell', win).splitlines() if '=' in line)
        if int(geom.get('WIDTH', 0)) == width and int(geom.get('HEIGHT', 0)) == height:
            s.xdotool('windowraise', win)
    s.xdotool('mousemove', str(width // 2), str(height // 2 - 148))
    time.sleep(.2)
    s.screenshot('wheel-overlay-activation')
    before = _status(s)
    s.xdotool('click', '1')
    time.sleep(.3)
    after = _status(s)
    assert not after['overlayVisible'], (before, after)


def _incoming_as(s, content, source):
    """Invoke through the frozen wire contract using a named peer client."""
    script = '''
import json, socket, sys
endpoint = json.load(open(sys.argv[1]))
sock = socket.socket(socket.AF_UNIX)
sock.settimeout(20)
sock.connect(endpoint['address'])
stream = sock.makefile('rb')
def send(i, method, params):
    sock.sendall((json.dumps({'v':1,'id':i,'method':method,'params':params})+'\\n').encode())
send(1, 'hello', {'token':endpoint['token'],'protocol':[1],'client':{'id':sys.argv[3],'version':'1'}})
hello = json.loads(stream.readline())
assert 'result' in hello, hello
send(2, 'invoke', {'action':'wheel.add_action','inputs':[json.loads(sys.argv[2])],
                 'context':{'source':sys.argv[3],'interactive':True,'reason':'user-click'}})
for line in stream:
    m = json.loads(line)
    if m.get('id') == 2 and 'error' in m:
        print(m); sys.exit(1)
    if m.get('method') == 'job.done':
        print(json.dumps(m['params']))
        sys.exit(0 if m['params']['status'] == 'success' else 1)
'''
    return subprocess.Popen([sys.executable, '-c', script, str(s.endpoint('arcade.wheel')),
                             json.dumps(content), source], env=s.env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)


def _surface_text(s, win):
    shot = s.screenshot('wheel-check', win)
    r = subprocess.run(['tesseract', str(shot), 'stdout', '--psm', '6'],
                       env=s.env, capture_output=True, text=True, timeout=10)
    assert r.returncode == 0, r.stderr
    return r.stdout


def _activity_text(s):
    win = s.wait_window('Arcade Wheel.*Activity')
    return _surface_text(s, win)


def _close_activity(s):
    wins = s.xdotool('search', '--onlyvisible', '--name', 'Arcade Wheel.*Activity')
    if wins:
        # A private session without a window manager still delivers WM_DELETE.
        s.xdotool('windowfocus', wins.splitlines()[0], 'key', 'alt+F4')


def _set_clipboard_text(s, text):
    p = s.invoke('wheel', 'wheel.add_action', '--text', text, '--hint', 'command', background=True)
    win = s.wait_window('Arcade Wheel.*Settings')
    time.sleep(.35)
    _click(s, win, 650, 418)
    s.xdotool('key', 'ctrl+a', 'ctrl+c', 'Escape')
    out, err = p.communicate(timeout=10)
    assert p.returncode != 0, out + err


@check('wheel')
def wheel_show_confirm_and_cancel(s):
    if 'arcade.wheel' not in s.procs:
        s.start('arcade.wheel')
    code, result = s.invoke('wheel', 'wheel.show')
    assert code == 0 and result['message'] == 'The Wheel is open', result
    status = json.loads(_wheel_command(s, '--status').stdout)
    assert status['overlayVisible'], status
    _wheel_command(s, '--cancel')

    path = _config(s)
    before = path.read_bytes()
    _mock(s, 'arcade.lens', FIXTURES / 'lens.json', s.root / 'wheel-placement-calls.jsonl')
    time.sleep(.4)
    add = _incoming_as(s, {'type': 'text/url', 'text': 'https://wheel-test.example/confirm'}, 'arcade.lens')
    win = s.wait_window('Arcade Wheel.*Settings')
    time.sleep(.4)
    assert 'Arcade Lens wants to add' in _surface_text(s, win), 'known caller display name missing'
    assert add.poll() is None, 'add_action must wait for explicit confirmation'
    assert path.read_bytes() == before, 'pending request changed the deck'
    _place(s, win, 'wheel-add-action-prefilled-confirm')
    out, err = add.communicate(timeout=15)
    assert add.returncode == 0, out + err
    assert 'Added to' in out, out
    config = json.loads(path.read_text())
    action = config['decks'][0]['actions'][0]
    assert action['type'] == 'url' and action['payload']['url'] == 'https://wheel-test.example/confirm', action

    s.kill('arcade.lens', signal.SIGTERM)
    (s.root / 'arcade/apps/arcade.lens.json').unlink()
    saved = path.read_bytes()
    cancel = s.invoke('wheel', 'wheel.add_action', '--url', 'https://wheel-test.example/cancel', background=True)
    time.sleep(.5)
    assert cancel.poll() is None, 'second add should remain pending'
    s.xdotool('windowfocus', win, 'key', 'Escape')
    out, err = cancel.communicate(timeout=10)
    assert cancel.returncode != 0 and 'Cancelled.' in err + out and '[denied:' in err + out, out + err
    assert path.read_bytes() == saved, 'cancel changed the deck'

    close = s.invoke('wheel', 'wheel.add_action', '--url', 'https://wheel-test.example/close', background=True)
    time.sleep(.5)
    s.xdotool('windowfocus', win, 'key', 'ctrl+w')
    out, err = close.communicate(timeout=10)
    assert close.returncode != 0 and 'Cancelled.' in err + out and '[denied:' in err + out, out + err
    assert path.read_bytes() == saved, 'closing Settings changed the deck'
    return 'wheel.show: overlay visible; add_action: pending without writes, saved URL, Cancel and closing Settings -> user_cancelled'


@check('wheel')
def connected_apps_and_command_note(s):
    _wheel_command(s, '--settings')
    win = s.wait_window('Arcade Wheel.*Settings')
    time.sleep(.4)  # finish the previous modal's close transition
    s.xdotool('windowsize', win, '1240', '820')
    _click(s, win, 90, 430)  # Connected apps navigation row
    time.sleep(.3)
    s.screenshot('wheel-connected-apps', win)
    path = _config(s)
    command = s.invoke('wheel', 'wheel.add_action', '--text', '/usr/bin/printf "$HOME | literal"',
                       '--hint', 'command', background=True)
    time.sleep(.4)
    assert command.poll() is None
    before = path.read_bytes()
    _place(s, win, 'wheel-command-not-a-shell')
    out, err = command.communicate(timeout=15)
    assert command.returncode == 0, out + err
    action = json.loads(path.read_text())['decks'][0]['actions'][0]
    assert action['type'] == 'command' and action['payload']['command'] == '/usr/bin/printf "$HOME | literal"', action
    assert path.read_bytes() != before
    s.xdotool('windowfocus', win, 'key', 'ctrl+w')
    return 'Connected apps rows render; command draft shows direct execution note and saves only after explicit confirmation'
@check('wheel')
def real_box_pipeline_and_look_preview(s):
    s.start('arcade.box')
    s.kill('arcade.box', signal.SIGTERM)
    pipeline = {'id': 'wheel-uppercase', 'name': 'Wheel uppercase', 'version': 1,
                'nodes': [{'id': 'upper', 'toolId': 'arcade.text.case',
                           'inputs': [{'kind': 'external', 'index': 0}], 'options': {'mode': 'upper'}}],
                'outputNodes': ['upper']}
    db = Path(s.env['XDG_DATA_HOME']) / 'dev.arcadebox.app/arcade.sqlite3'
    with sqlite3.connect(db) as conn:
        conn.execute('INSERT OR REPLACE INTO pipelines(id, version, definition_json) VALUES(?, ?, ?)',
                     (pipeline['id'], 1, json.dumps(pipeline)))
    s.start('arcade.box')
    code, listing = s.invoke('box', 'box.pipelines')
    assert code == 0 and any(p['id'] == 'wheel-uppercase' for p in listing['outputs'][0]['data']), listing
    s.start('arcade.look')
    # Wheel's Clipboard input consumes text selected by the user, with no shell.
    _set_clipboard_text(s, 'hello pipeline')
    _bind(s, 'arcade.box', 'box.pipeline.run', 'clipboard', {'pipeline': 'wheel-uppercase'}, 'Wheel uppercase')
    time.sleep(.5)
    _run_slot(s)
    _wait(lambda: 'HELLO PIPELINE' in _activity_text(s), 'real Box pipeline output missing', timeout=12)
    s.screenshot('wheel-real-box-pipeline', s.wait_window('Arcade Wheel.*Activity'))
    # Linux has no file-manager resolver; verify the real file preview separately.
    image = s.root / 'wheel-real-preview.png'
    # Small PNG already accepted by Look's ordinary preview path.
    subprocess.run(['convert', '-size', '12x7', 'xc:red', str(image)], env=s.env, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    code, result = s.invoke('look', 'look.preview', '--file', str(image))
    assert code == 0 and 'Previewing' in result['message'], result
    s.kill('arcade.look', signal.SIGTERM)
    s.kill('arcade.box', signal.SIGTERM)
    return 'Wheel clipboard slot -> real saved Box pipeline -> HELLO PIPELINE; real Look preview serves; Linux selection resolver remains absent'


@check('wheel')
def mock_pipeline_picker_selection_and_failures(s):
    log = s.root / 'wheel-consumer-calls.jsonl'
    _mock(s, 'arcade.box', FIXTURES / 'box.json', log)
    time.sleep(.6)
    win = _bind(s, 'arcade.box', 'box.pipeline.run', options={'pipeline': 'p-optimized-screenshot'},
                title='Send optimized screenshot', shot='wheel-pipeline-slot')
    _run_slot(s)
    _wait(lambda: _calls(log, 'box.pipeline.run'), 'pipeline slot did not invoke Box')
    assert _calls(log, 'box.pipeline.run')[-1]['options']['pipeline'] == 'p-optimized-screenshot'
    assert _calls(log, 'box.pipeline.run')[-1]['inputs'] == []
    time.sleep(.4)
    # Search the real picker cache. The pipeline's distinct options survive selection.
    _click(s, win, 1040, 275)
    _click(s, win, 1030, 580)
    time.sleep(.3)
    _click(s, win, 650, 340)
    s.xdotool('type', '--clearmodifiers', 'optimized')
    time.sleep(.3)
    s.screenshot('wheel-action-picker-arcade-search', win)
    assert 'optimized' in _surface_text(s, win).lower()
    s.xdotool('key', 'Escape')
    time.sleep(.3)

    _click(s, win, 90, 292)  # Trigger
    _click(s, win, 420, 260)
    s.xdotool('key', 'ctrl+alt+space')
    time.sleep(.3)
    s.screenshot('wheel-shortcut-clash', win)
    text = _surface_text(s, win)
    assert 'Used by Arcade Box' in text, text
    assert json.loads(_config(s).read_text())['trigger']['shortcut'] == 'F8'

    # Connected apps toggles hide the Box offers and preserve saved slots.
    _click(s, win, 90, 430)
    time.sleep(.3)
    s.screenshot('wheel-connected-apps-installed', win)
    _click(s, win, 450, 200)
    _wait(lambda: 'arcade.box' in json.loads(_config(s).read_text())['link']['disabledPeers'], 'peer toggle did not persist')
    _click(s, win, 90, 154)
    _click(s, win, 1040, 275)
    _click(s, win, 1030, 580)
    time.sleep(.3)
    s.screenshot('wheel-unavailable-slot-reason', win)
    text = _surface_text(s, win)
    assert 'turned off in Connected apps' in text, text
    _click(s, win, 1011, 113)  # the picker's Close button (Escape needs the popup focused)
    time.sleep(.3)
    _click(s, win, 90, 430)
    s.screenshot('wheel-disabled-peer', win)
    _click(s, win, 405, 200)
    s.screenshot('wheel-reenabled-peer', win)
    _wait(lambda: 'arcade.box' not in json.loads(_config(s).read_text())['link']['disabledPeers'], 'peer toggle did not re-enable')
    # The master switch removes the endpoint and all offered actions.
    _click(s, win, 948, 140)
    _wait(lambda: not s.endpoint('arcade.wheel').exists(), 'master switch did not stop listener')
    manifest = json.loads((s.root / 'arcade/apps/arcade.wheel.json').read_text())
    assert not manifest['settings']['linkEnabled'] and manifest['actions'] == [], manifest
    _click(s, win, 948, 140)
    s.wait_running('arcade.wheel')

    look = json.loads((FIXTURES / 'look.json').read_text())
    selected = s.root / 'selected-wheel.png'
    subprocess.run(['convert', '-size', '2x2', 'xc:red', str(selected)], env=s.env, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    look['actions'][0]['mock']['result']['outputs'] = [{'type': 'file/image', 'path': str(selected)}]
    _mock(s, 'arcade.look', look, log)
    win = _bind(s, 'arcade.box', 'box:arcade.image.convert#webp', 'file-selection', title='Convert selected file')
    time.sleep(.4)
    count = len(_calls(log, 'box:arcade.image.convert#webp'))
    _run_slot(s)
    _wait(lambda: len(_calls(log, 'box:arcade.image.convert#webp')) > count, 'file selection target not invoked')
    assert _calls(log, 'look.preview_selection')[-1]['options']['resolveOnly'] is True
    assert _calls(log, 'box:arcade.image.convert#webp')[-1]['inputs'][0]['path'] == str(selected)
    time.sleep(.3)
    _bind(s, 'arcade.look', 'look.preview_selection', title='Preview selection')
    count = len(_calls(log, 'look.preview_selection'))
    _run_slot(s)
    _wait(lambda: len(_calls(log, 'look.preview_selection')) > count, 'flow 9 slot not invoked')
    assert not _calls(log, 'look.preview_selection')[-1]['options'].get('resolveOnly', False)
    s.screenshot('wheel-preview-selection', s.wait_window('Arcade Wheel.*Activity'))
    return 'mock optimized pipeline bound and run; searched grouped picker; shortcut clash; peer/master toggles; unavailable saved slot; resolveOnly chain and direct flow 9'


@check('wheel')
def provider_failure_contracts_and_launch_modes(s):
    r = subprocess.run([str(REPO / 'build/test_ArcadeLinkProvider')], env=dict(s.env, QT_QPA_PLATFORM='offscreen'),
                       capture_output=True, text=True, timeout=30)
    assert r.returncode == 0 and '0 failed, 0 skipped' in r.stdout, r.stdout + r.stderr
    r = s.cli('status', 'wheel', '--json', check=True)
    assert json.loads(r.stdout)['status']['mode'] == 'background', r.stdout
    s.kill('arcade.wheel', signal.SIGTERM)
    spec = APPS['arcade.wheel']
    p = subprocess.Popen([str(spec['dir'] / spec['bin']), '--settings'], env=s.env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    s.procs['arcade.wheel'] = p
    s.wait_running('arcade.wheel')
    r = s.cli('status', 'wheel', '--json', check=True)
    assert json.loads(r.stdout)['status']['mode'] == 'foreground', r.stdout
    return 'actual Qt provider: missing/disabled/unavailable/crash/cancel/timeout/oversize/Private/secret all pass; app.status launch modes verified for real Wheel'


