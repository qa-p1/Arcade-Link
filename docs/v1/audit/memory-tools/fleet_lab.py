#!/usr/bin/env python3
"""fleet_lab.py TAG [--ext] [--shim] [--open APP] [--env K=V ...]
All 8 Arcade apps running together in ONE isolated session (shared HOME/dbus so Link sees them), Xvfb :95.
Measures per-app tree RSS/PSS (+ FUSE stub), fleet total, optionally after a trim-shim step and with one window open.
This is also the prototype of the CI conformance check (see memory.md §6)."""
import sys, os, time, json, collections, signal
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
tag = sys.argv[1]; ext = '--ext' in sys.argv; shim = '--shim' in sys.argv; opentrim = '--opentrim' in sys.argv
settle = int(sys.argv[sys.argv.index('--settle') + 1]) if '--settle' in sys.argv else 70
openapp = sys.argv[sys.argv.index('--open') + 1] if '--open' in sys.argv else None
extra = {}
for i, a in enumerate(sys.argv):
    if a == '--env': k, v = sys.argv[i+1].split('=', 1); extra[k] = v
EXT = '/var/tmp/arcade-audit-ext'
APPS = [('tools', 'Tools'), ('box', 'Box'), ('look', 'Look'), ('find', 'Find'), ('lens', 'Lens'), ('clipboard', 'Clipboard'), ('shelf', 'Shelf'), ('wheel', 'Wheel')]
L = Lab(f'fleet-{tag}', extra)
td = L.root + '/trim'; os.makedirs(td, exist_ok=True); open(td + '/go', 'w').write('init')
if shim: L.env['TRIMDIR'] = td; L.env['LD_PRELOAD'] = S + '/bin/trimshim.so'
cfg = L.root + '/config/arcade/find'; os.makedirs(cfg, exist_ok=True)
sj = json.load(open(os.path.expanduser('~/.config/arcade/find/settings.json')))
sj['roots'] = ['/home/harneet/Work']; sj['startAtLogin'] = False
json.dump(sj, open(cfg + '/settings.json', 'w'))
jl = f'{RES}/fleet-{tag}.jsonl'; open(jl, 'w').close()
launch = {}
for a, cap in APPS:
    exe = f'{EXT}/{cap}/squashfs-root/AppRun' if ext else L.img(cap)
    p = L.start(exe, ['--background']); launch[a] = p.pid; time.sleep(2)

def descendants(root):
    ppid = {}
    for d in os.listdir('/proc'):
        if d.isdigit():
            try:
                s = open(f'/proc/{d}/stat').read(); ppid[int(d)] = int(s[s.rindex(')')+2:].split()[1])
            except Exception: pass
    out = [root]
    for q in out:
        out += [p for p, pp in ppid.items() if pp == q and p not in out]
    return out

def stubs_of():
    out = collections.defaultdict(list)
    for d in os.listdir('/proc'):
        if not d.isdigit(): continue
        try:
            exe = os.readlink(f'/proc/{d}/exe')
            if not exe.endswith('.AppImage'): continue
            if f'HOME={L.home}'.encode() not in open(f'/proc/{d}/environ', 'rb').read().split(b'\0'): continue
            out[os.path.basename(exe)].append(int(d))
        except Exception: pass
    return out

def measure(label):
    rows = {}; tot = collections.Counter(); allstubs = stubs_of()
    for a, _ in APPS:
        pids = descendants(launch[a])
        stub = [] if ext else allstubs.get(f'Arcade-{dict(APPS)[a]}.AppImage', [])
        app = [p for p in pids if p not in stub and os.path.exists(f'/proc/{p}')]
        c = collections.Counter(names=[])
        for p in app:
            st = T.status(p); ro = T.rollup(p)
            c['rss'] += st.get('VmRSS', 0); c['pss'] += ro.get('Pss', 0); c['anon'] += st.get('RssAnon', 0) + st.get('RssShmem', 0); c['swap'] += st.get('VmSwap', 0); c['thr'] += st.get('Threads', 0)
        names = sorted({T.name(p)[:14] for p in app})
        srss = sum(T.status(p).get('VmRSS', 0) for p in stub); spss = sum(T.rollup(p).get('Pss', 0) for p in stub)
        rows[a] = dict(rss=c['rss'] / 1024, pss=c['pss'] / 1024, anon=c['anon'] / 1024, swap=c['swap'] / 1024, thr=c['thr'], procs=len(app), stub_rss=srss / 1024, stub_pss=spss / 1024, names=names)
        for k in ('rss', 'pss', 'stub_rss', 'stub_pss', 'anon', 'thr'): tot[k] += rows[a][k]
    print(f'--- {label}: ' + ('unpacked' if ext else 'AppImage') + (' +shim' if shim else ''), flush=True)
    for a, r in rows.items():
        print(f"  {a:<10} procs {r['procs']} RSS {r['rss']:6.1f} PSS {r['pss']:6.1f} anon {r['anon']:6.1f} thr {r['thr']:3d} stub {r['stub_rss']:5.1f}/{r['stub_pss']:5.1f}  {','.join(r['names'])}", flush=True)
    print(f"  FLEET apps  RSS {tot['rss']/1:6.1f} PSS {tot['pss']:6.1f} | stubs RSS {tot['stub_rss']:5.1f} PSS {tot['stub_pss']:5.1f} | TOTAL RSS {tot['rss']+tot['stub_rss']:6.1f} PSS {tot['pss']+tot['stub_pss']:6.1f}  threads {tot['thr']}", flush=True)
    with open(jl, 'a') as f: f.write(json.dumps(dict(label=label, rows=rows, tot=dict(tot))) + '\n')
    return rows

time.sleep(settle); measure(f'bg+{settle}s')
if shim:
    open(td + '/go', 'w').write('trim+dontneed'); time.sleep(3); measure('bg-after-trim+dontneed')
if openapp:
    cap = openapp.capitalize()
    exe = f'{EXT}/{cap}/squashfs-root/AppRun' if ext else L.img(cap)
    pat = {'tools': '^Arcade Tools$', 'box': 'Arcade Box', 'look': 'Arcade Look'}[openapp]
    args = {'tools': ['--settings'], 'box': ['--show'], 'look': [L.home + '/s.txt']}[openapp]
    open(L.home + '/s.txt', 'w').write('x\n' * 100)
    t = time.time(); L.cli(exe, args, timeout=20)
    w, _ = wait_for(lambda: (windows(pat) or [None])[0], 20)
    print(f'open {openapp}: window mapped {time.time()-t:.2f}s', flush=True)
    time.sleep(6); measure(f'{openapp}-window-open')
    if opentrim:
        open(td + '/go', 'w').write('trim+dontneed'); time.sleep(3); measure(f'{openapp}-window-open-trimmed')
    if w: wmclose(w)
    time.sleep(30); measure(f'{openapp}-closed+30s')
    if shim:
        for p in [x for x in os.listdir('/proc') if x.isdigit()]:
            try:
                if T.name(int(p)).startswith('WebKitNetwork') and f'HOME={L.home}'.encode() in open(f'/proc/{p}/environ', 'rb').read().split(b'\0'): os.kill(int(p), signal.SIGTERM)
            except Exception: pass
        open(td + '/go', 'w').write('trim+dontneed'); time.sleep(4); measure(f'{openapp}-closed-released')
L.quit_all(); L.stop_dbus()
