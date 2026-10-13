"""Production Tools/fleet probes, called within Link's isolated Xvfb session."""
from __future__ import annotations

import argparse
import concurrent.futures
import ctypes
import json
import math
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time

import e2e
from real_apps import Peer, call, fleet, index, tools_preferences, wait


def percentile(values, p=.95):
    return sorted(values)[max(0, math.ceil(len(values) * p) - 1)]


def close_window(s, window):
    """WM_DELETE_WINDOW works without a window manager; XKillClient does not."""
    class Data(ctypes.Union):
        _fields_ = [('b', ctypes.c_char * 20), ('s', ctypes.c_short * 10), ('l', ctypes.c_long * 5)]
    class ClientMessage(ctypes.Structure):
        _fields_ = [('type', ctypes.c_int), ('serial', ctypes.c_ulong), ('send_event', ctypes.c_int),
                    ('display', ctypes.c_void_p), ('window', ctypes.c_ulong), ('message_type', ctypes.c_ulong),
                    ('format', ctypes.c_int), ('data', Data)]
    class Event(ctypes.Union):
        _fields_ = [('client', ClientMessage), ('padding', ctypes.c_long * 24)]
    x11 = ctypes.CDLL('libX11.so.6')
    x11.XOpenDisplay.restype = ctypes.c_void_p
    x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x11.XInternAtom.restype = ctypes.c_ulong
    x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(Event)]
    x11.XFlush.argtypes = [ctypes.c_void_p]
    x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = x11.XOpenDisplay(s.env['DISPLAY'].encode())
    assert display, 'cannot connect to isolated Xvfb'
    try:
        event = Event()
        event.client.type = 33
        event.client.display = display
        event.client.window = int(window)
        event.client.message_type = x11.XInternAtom(display, b'WM_PROTOCOLS', 0)
        event.client.format = 32
        event.client.data.l[0] = x11.XInternAtom(display, b'WM_DELETE_WINDOW', 0)
        x11.XSendEvent(display, int(window), 0, 0, ctypes.byref(event))
        x11.XFlush(display)
    finally:
        x11.XCloseDisplay(display)


def overlay_spec():
    return {**e2e.TOOLS, 'bin': os.environ.get('ARCADE_OVERLAY_BIN', 'target/debug/arcade-tools-overlay'),
            'build': ['cargo', 'build', '-p', 'arcade-tools-overlay']}


def overlay_frames(s, runs=20, hotkey=True):
    overlay = e2e.require_current_build(overlay_spec())
    logpath = s.root / 'overlay-frames.log'
    logpath.write_text('', encoding='utf-8')
    owner = None
    if hotkey:
        # Tools deliberately detaches the overlay with null stderr. A disposable
        # companion launcher captures the actual first-frame callback. This adds
        # a shell exec to the measured key→frame path; it generates no frames.
        tools = e2e.require_current_build(e2e.TOOLS)
        directory = s.root / 'overlay-tools'
        directory.mkdir(exist_ok=True)
        shutil.copy2(tools, directory / 'arcade-tools')
        wrapper = directory / 'arcade-tools-overlay'
        wrapper.write_text('#!/bin/sh\nexec ' + shlex.quote(str(overlay)) + ' "$@" 2>>' + shlex.quote(str(logpath)) + '\n', encoding='utf-8')
        wrapper.chmod(0o755)
        tools_preferences(s)
        log = (s.root / 'arcade.tools.log').open('ab')
        owner = subprocess.Popen([str(directory / 'arcade-tools'), '--background'],
                                 env=dict(s.env, ARCADE_OVERLAY_TIMING='1'), stdout=log, stderr=log, start_new_session=True)
        log.close()
        s.procs['arcade.tools'] = owner
        s.wait_running('arcade.tools')
        time.sleep(.5)  # Allow the native hotkey registration worker to finish.
    samples, callback_samples, backends = [], [], []
    try:
        for _ in range(runs):
            previous = logpath.stat().st_size
            began = time.perf_counter()
            process = None
            if hotkey:
                key = subprocess.run(['xdotool', 'key', '--clearmodifiers', 'super+slash'], env=s.env, capture_output=True, timeout=3)
                assert key.returncode == 0, key.stderr
            else:
                with logpath.open('ab') as log:
                    process = subprocess.Popen([str(overlay), '--show'], env=dict(s.env, ARCADE_OVERLAY_TIMING='1'), stdout=log, stderr=log, start_new_session=True)
                s.procs['overlay'] = process
            def frame():
                text = logpath.read_text(encoding='utf-8')[previous:]
                return re.search(r'first_frame backend=(\S+) spawn_to_first_frame_ms=([\d.]+)', text)
            result = wait(frame, lambda: f'real overlay did not paint: {logpath.read_text(encoding="utf-8")[-3000:]}', timeout=10, interval=.001)
            samples.append(round((time.perf_counter() - began) * 1000, 3))
            callback_samples.append(float(result[2]))
            backends.append(result[1])
            window = s.wait_window('^Arcade Tools Shortcuts$')
            close_window(s, window)
            wait(lambda: not s.xdotool('search', '--onlyvisible', '--name', '^Arcade Tools Shortcuts$'), 'overlay did not close')
            if process:
                process.wait(timeout=3)
                s.procs.pop('overlay', None)
            # No overlay process stays running between uses. Wait on the
            # instance command, not a fixed delay on the owner's desktop.
            runtime = Path(s.env['ARCADE_HOME']) / 'run'
            wait(lambda: not any(runtime.glob('*overlay*.endpoint')), 'overlay endpoint persisted', timeout=3)
    finally:
        if owner:
            s.kill('arcade.tools')
        s.kill('overlay')
    return {'method': 'xdotool Super+/ → real first-frame callback observed in private log (includes XTest command and capture-launcher overhead)' if hotkey else 'spawn → real first-frame callback observed in private log',
            'backend': backends[0] if len(set(backends)) == 1 else backends,
            'samplesMs': samples, 'p95Ms': percentile(samples), 'callbackSamplesMs': callback_samples,
            'callbackP95Ms': percentile(callback_samples), 'budgetMs': 80,
            'budgetMet': percentile(samples) <= 80, 'physicalDisplayMeasured': False}


def tree_pids(root):
    result, frontier = {root}, [root]
    while frontier:
        pid = frontier.pop()
        try:
            children = set()
            for task in Path(f'/proc/{pid}/task').iterdir():
                children.update(int(p) for p in (task / 'children').read_text().split())
            for child in children - result:
                frontier.append(child)
                result.add(child)
        except OSError:
            pass
    return result


def counters(pid):
    rss, ticks = 0, 0
    pids = tree_pids(pid)
    for child in pids:
        try:
            fields = Path(f'/proc/{child}/stat').read_text().rsplit(')', 1)[1].split()
            ticks += int(fields[11]) + int(fields[12])
            rss += next(int(line.split()[1]) for line in Path(f'/proc/{child}/status').read_text().splitlines() if line.startswith('VmRSS:'))
        except (OSError, StopIteration):
            pass
    return {'rssKiB': rss, 'cpuTicks': ticks, 'pids': sorted(pids)}


def idle(s, seconds=30):
    tools_preferences(s)
    p = s.start('arcade.tools')
    call(s, 'arcade.tools', 'app.activate')
    window = s.wait_window('^Arcade Tools$')
    close_window(s, window)
    wait(lambda: not s.xdotool('search', '--onlyvisible', '--name', '^Arcade Tools$'), 'Tools webview did not close')
    time.sleep(3)
    before = counters(p.pid)
    time.sleep(seconds)
    assert p.poll() is None, 'Tools exited instead of remaining resident'
    after = counters(p.pid)
    assert after['cpuTicks'] >= before['cpuTicks'], 'process-tree CPU counter decreased; sample invalid'
    s.kill('arcade.tools')
    return {'method': '/proc process tree after foreground webview closed; 3 s settle; no metric probes during idle window; update checks disabled',
            'seconds': seconds, 'before': before, 'after': after,
            'rssMiB': round(after['rssKiB'] / 1024, 3), 'cpuTicks': after['cpuTicks'] - before['cpuTicks'],
            'budgetMet': after['rssKiB'] <= 40 * 1024 and after['cpuTicks'] == before['cpuTicks']}


def handback(s, runs):
    fleet(s)
    tools_preferences(s)
    samples = {app: [] for app in e2e.APP_IDS}
    for _ in range(runs):
        s.start('arcade.tools')
        for app in e2e.APP_IDS:
            with Peer(s, app) as peer:
                wait(lambda: peer.call('app.status').get('status', {}).get('tray') == 'hosted', f'{app} not hosted')
        peers = {app: Peer(s, app) for app in e2e.APP_IDS}
        try:
            gate = __import__('threading').Event()
            def own(app):
                gate.wait()
                wait(lambda: peers[app].call('app.status').get('status', {}).get('tray') == 'own', f'{app}: hand-back timed out', timeout=1, interval=.001)
                return round((time.perf_counter() - began) * 1000, 3)
            with concurrent.futures.ThreadPoolExecutor(max_workers=len(peers)) as pool:
                jobs = {app: pool.submit(own, app) for app in peers}
                began = time.perf_counter()
                s.procs['arcade.tools'].kill()
                gate.set()
                for app, job in jobs.items():
                    samples[app].append(job.result(timeout=2))
            s.kill('arcade.tools')
        finally:
            for peer in peers.values():
                peer.close()
    largest = max(v for values in samples.values() for v in values)
    return {'method': 'real Tools SIGKILL → real app.status tray=own over preconnected sockets; 1 ms bounded test sampling',
            'samplesMs': samples, 'maxMs': largest, 'limitMs': 1000, 'targetMs': 200, 'budgetMet': largest <= 1000,
            'physicalIconPaintMeasured': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runs', type=int, default=20)
    parser.add_argument('--idle-seconds', type=float, default=30)
    parser.add_argument('--json', type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get('ARCADE_E2E_INNER') != '1':
        parser.error('run through python3 tools/e2e.py run -- python3 tools/real_bench.py ...')
    s = e2e.Session(Path(os.environ['ARCADE_E2E_ROOT']), headless=True, real_apps=True)
    s.env.update({key: os.environ[key] for key in ('DISPLAY', 'DBUS_SESSION_BUS_ADDRESS')})
    result = {'date': time.strftime('%Y-%m-%d'), 'runs': args.runs, 'platform': 'Linux X11 under Xvfb; no real desktop', 'builds': {}}
    for app, spec in {**e2e.APPS, 'arcade.tools': e2e.TOOLS}.items():
        head = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=spec['dir'], capture_output=True, text=True)
        result['builds'][app] = {'repo': str(spec['dir']), 'head': head.stdout.strip(), 'binary': str(spec['dir'] / spec['bin'])}
    probes = {
        'trayHandback': lambda: handback(s, min(args.runs, 10)),
        'toolsIdle': lambda: idle(s, args.idle_seconds),
        'shortcutIndex': lambda: {'method': 'actual Tools build_index, real manifest/doc/catalog inputs; worker elapsed excludes adapter startup', 'samplesMs': [index(s)['elapsedMs'] for _ in range(args.runs)], 'budgetMs': 150},
        'overlay': lambda: overlay_frames(s, args.runs),
        'startup': lambda: json.loads(subprocess.run([str(e2e.LINK / 'target/debug/examples/startup-probe')], env=s.env, capture_output=True, text=True, encoding='utf-8', check=True, timeout=15).stdout),
    }
    for name, probe in probes.items():
        print(f'measuring real {name}...', flush=True)
        try:
            result[name] = probe()
        except Exception as error:
            result[name] = {'measured': False, 'reason': str(error)}
        finally:
            s.close()
        print(json.dumps({name: result[name]}), flush=True)
        args.json.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    return 0 if all('reason' not in result[name] for name in probes) else 1


if __name__ == '__main__':
    sys.exit(main())
