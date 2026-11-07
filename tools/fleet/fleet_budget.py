#!/usr/bin/env python3
"""fleet_budget.py: CI conformance check for the Arcade memory budget (fleet < 500 MB PSS and RSS, stubs included).

Starts ALL eight Arcade apps together in one private session (own HOME/XDG/dbus) on Xvfb, lets them settle, measures every
app's process tree from /proc (smaps_rollup PSS + VmRSS, FUSE stub counted separately), optionally opens ONE window and
measures again, closes it and measures after the release delay. Exits 1 when a budget in budgets.json is exceeded.
Read-only on the apps; Linux only. Needs: Xvfb (or an existing $DISPLAY), dbus-daemon, xdotool, the eight AppImages.

  fleet_budget.py --apps DIR [--budgets budgets.json] [--settle 75] [--open tools --close-wait 45] [--json out.json]
DIR holds Arcade-<App>.AppImage (Tools Box Look Find Lens Clipboard Shelf Wheel). Budgets are per app and per state; "fleet" is the sum
(apps + stubs). Xvfb uses llvmpipe, so GL apps (Clipboard, WebKit windows) read higher than on a GPU: the budget file has a separate "xvfb" section.
"""
import argparse, collections, json, os, re, shutil, signal, subprocess, sys, tempfile, time

APPS = ['tools', 'box', 'look', 'find', 'lens', 'clipboard', 'shelf', 'wheel']
OPEN_ARGS = {'tools': (['--settings'], r'^Arcade Tools$'), 'box': (['--show'], r'Arcade Box'), 'look': (['@FILE'], r'Arcade Look')}


def kv(path):
    d = {}
    try:
        for line in open(path):
            m = re.match(r'(\w+):\s+(\d+)', line)
            if m: d[m.group(1)] = int(m.group(2))
    except OSError:
        pass
    return d


def children_map():
    ppid = {}
    for d in os.listdir('/proc'):
        if d.isdigit():
            try:
                s = open(f'/proc/{d}/stat').read(); ppid[int(d)] = int(s[s.rindex(')') + 2:].split()[1])
            except OSError:
                pass
    return ppid


def tree(root):
    ppid = children_map(); out = [root]
    for q in out:
        out += [p for p, pp in ppid.items() if pp == q and p not in out]
    return out


def find_stubs(home):
    """The AppImage runtime forks its FUSE daemon, which is re-parented to init: it is NOT a descendant of the app.
    Find it by exe (the .AppImage file) and by the private HOME it inherited."""
    out = collections.defaultdict(list)
    for d in os.listdir('/proc'):
        if not d.isdigit(): continue
        try:
            exe = os.readlink(f'/proc/{d}/exe')
            if exe.endswith('.AppImage') and f'HOME={home}'.encode() in open(f'/proc/{d}/environ', 'rb').read().split(b'\0'):
                out[os.path.basename(exe)].append(int(d))
        except OSError:
            pass
    return out


def measure(launch, home):
    rows = {}; tot = collections.Counter(); stubs = find_stubs(home)
    for app, pid in launch.items():
        pids = [p for p in tree(pid) if os.path.exists(f'/proc/{p}')]
        stub = stubs.get(f'Arcade-{app.capitalize()}.AppImage', []); run = [p for p in pids if p not in stub]
        r = dict(rss=0.0, pss=0.0, stub_rss=0.0, stub_pss=0.0, procs=len(run))
        for p in run:
            r['rss'] += kv(f'/proc/{p}/status').get('VmRSS', 0) / 1024; r['pss'] += kv(f'/proc/{p}/smaps_rollup').get('Pss', 0) / 1024
        for p in stub:
            r['stub_rss'] += kv(f'/proc/{p}/status').get('VmRSS', 0) / 1024; r['stub_pss'] += kv(f'/proc/{p}/smaps_rollup').get('Pss', 0) / 1024
        r['total_rss'] = r['rss'] + r['stub_rss']; r['total_pss'] = r['pss'] + r['stub_pss']
        rows[app] = r
        for k in ('total_rss', 'total_pss'): tot[k] += r[k]
    rows['fleet'] = dict(total_rss=tot['total_rss'], total_pss=tot['total_pss'])
    return rows


def check(state, rows, budgets, failures):
    b = budgets.get(state, {})
    for name, lim in b.items():
        r = rows.get(name)
        if not r: continue
        for metric in ('pss', 'rss'):
            if metric in lim and r[f'total_{metric}'] > lim[metric]:
                failures.append(f'{state}/{name}: {metric.upper()} {r["total_"+metric]:.0f} MB > budget {lim[metric]} MB')


def close_window(wid, env):
    """Xvfb has no window manager, so a 'close' must be WM_DELETE_WINDOW sent by us (what a title-bar button sends).
    xdotool windowclose destroys the X window (the apps then exit) and windowquit needs a WM: use wmclose.c next to this script."""
    here = os.path.dirname(os.path.abspath(__file__)); exe = os.path.join(here, 'wmclose')
    if not os.path.exists(exe):
        subprocess.run(['gcc', '-O2', '-o', exe, os.path.join(here, 'wmclose.c'), '-lX11'], check=True)
    subprocess.run([exe, wid], env=env)


def table(state, rows):
    print(f'--- {state}')
    for name, r in rows.items():
        if name == 'fleet': continue
        print(f'  {name:<10} procs {r["procs"]}  RSS {r["total_rss"]:6.1f}  PSS {r["total_pss"]:6.1f}   (stub {r["stub_rss"]:4.1f}/{r["stub_pss"]:4.1f})')
    f = rows['fleet']; print(f'  {"FLEET":<10}            RSS {f["total_rss"]:6.1f}  PSS {f["total_pss"]:6.1f}')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--apps', required=True); ap.add_argument('--budgets'); ap.add_argument('--settle', type=int, default=75)
    ap.add_argument('--open'); ap.add_argument('--close-wait', type=int, default=45); ap.add_argument('--json')
    ap.add_argument('--section', default=None, help='budget section prefix, e.g. "xvfb" (default: none)')
    a = ap.parse_args()
    budgets = json.load(open(a.budgets)) if a.budgets else {}
    if a.section: budgets = budgets.get(a.section, {})
    root = tempfile.mkdtemp(prefix='arcade-fleet-'); run = tempfile.mkdtemp(prefix='arf-'); os.chmod(run, 0o700)
    env = {k: v for k, v in os.environ.items() if not k.startswith('ARCADE_') and k not in ('APPIMAGE', 'APPDIR', 'LD_LIBRARY_PATH', 'WAYLAND_DISPLAY', 'DBUS_SESSION_BUS_ADDRESS')}
    xv = None
    if not env.get('DISPLAY'):
        env['DISPLAY'] = ':99'; xv = subprocess.Popen(['Xvfb', ':99', '-screen', '0', '1920x1080x24', '-nolisten', 'tcp'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL); time.sleep(1.5)
    for d in ('home', 'config', 'data', 'cache', 'state'): os.makedirs(f'{root}/{d}')
    env.update(HOME=f'{root}/home', XDG_CONFIG_HOME=f'{root}/config', XDG_DATA_HOME=f'{root}/data', XDG_CACHE_HOME=f'{root}/cache', XDG_STATE_HOME=f'{root}/state',
               XDG_RUNTIME_DIR=run, GDK_BACKEND='x11', ARCADE_NO_INSTALL_PROMPT='1')
    out = subprocess.run(['dbus-daemon', '--session', '--fork', '--print-address', '--print-pid'], env=env, capture_output=True, text=True).stdout.split('\n')
    env['DBUS_SESSION_BUS_ADDRESS'] = out[0]; dbus_pid = int(out[1])
    launch = {}; failures = []; result = {}
    try:
        for app in APPS:
            exe = os.path.join(a.apps, f'Arcade-{app.capitalize()}.AppImage')
            launch[app] = subprocess.Popen([exe, '--background'], env=env, cwd=env['HOME'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True).pid
            time.sleep(2)
        time.sleep(a.settle); rows = measure(launch, env['HOME']); table('background', rows); check('background', rows, budgets, failures); result['background'] = rows
        if a.open:
            args, pat = OPEN_ARGS[a.open]; exe = os.path.join(a.apps, f'Arcade-{a.open.capitalize()}.AppImage')
            sample = f'{root}/home/sample.txt'; open(sample, 'w').write('x\n' * 100)
            subprocess.run([exe, *[sample if x == '@FILE' else x for x in args]], env=env, timeout=30, capture_output=True)
            time.sleep(6); rows = measure(launch, env['HOME']); table(f'{a.open} window open', rows); check('open', rows, budgets, failures); result['open'] = rows
            wid = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', pat], env=env, capture_output=True, text=True).stdout.split()
            for w in wid: close_window(w, env)
            time.sleep(a.close_wait); rows = measure(launch, env['HOME']); table(f'{a.open} closed +{a.close_wait}s', rows); check('closed', rows, budgets, failures); result['closed'] = rows
    finally:
        extra = [p for v in find_stubs(env['HOME']).values() for p in v]
        for pid in list(launch.values()) + extra:
            for p in tree(pid):
                try: os.kill(p, signal.SIGTERM)
                except OSError: pass
        time.sleep(2)
        for pid in list(launch.values()):
            for p in tree(pid):
                try: os.kill(p, signal.SIGKILL)
                except OSError: pass
        try: os.kill(dbus_pid, signal.SIGTERM)
        except OSError: pass
        if xv: xv.terminate()
        shutil.rmtree(root, ignore_errors=True); shutil.rmtree(run, ignore_errors=True)
    if a.json: json.dump(result, open(a.json, 'w'), indent=1)
    if failures:
        print('\nBUDGET FAILURES:'); [print('  ' + f) for f in failures]; return 1
    print('\nall budgets met'); return 0


if __name__ == '__main__':
    sys.exit(main())
