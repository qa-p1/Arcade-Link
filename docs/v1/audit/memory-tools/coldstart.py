"""coldstart.py APP MODE N: time from exec to window mapped for a process started from nothing.
MODE = image (AppImage via FUSE) | ext (extracted AppRun). Page cache is warm (a run is done first)."""
import sys, os, time, signal, subprocess
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
app, mode, n = sys.argv[1], sys.argv[2], int(sys.argv[3]); cap = app.capitalize()
CF = {'tools': ([], '^Arcade Tools$'), 'box': (['--show'], 'Arcade Box'), 'look': (['@FILE'], 'Arcade Look'), 'clipboard': (['--show'], 'Clipboard'),
      'wheel': (['--preview'], '.'), 'shelf': (['--show'], 'Shelf'), 'find': (['--show'], 'Find'), 'lens': (['--capture'], 'Lens')}
args, pat = CF[app]
L = Lab(f'{app}-cold-{mode}'); fa = L.home + '/sample.txt'; open(fa, 'w').write('hello arcade\n' * 2000)
args = [fa if a == '@FILE' else a for a in args]
exe = L.img(cap) if mode == 'image' else f'/var/tmp/arcade-audit-ext/{cap}/squashfs-root/AppRun'
res = []
for i in range(n + 1):
    before = set(windows('.', visible=False))
    t0 = time.time(); p = L.start(exe, args)
    w, tm = wait_for(lambda: next((x for x in windows(pat) if x not in before), None), 30, 0.01)
    t1 = time.time() - t0
    time.sleep(2); o = L.snap(f'{app}-{mode}-run{i}', quiet=True)
    res.append((t1, o['total']['rss']/1024, o['total']['pss']/1024))
    L.quit_all(2); time.sleep(1)
    if i == 0: res.pop()   # warm-up run
r = [x[0] for x in res]
print(f'{app} {mode}: window_mapped s: ' + ' '.join(f'{x:.2f}' for x in r) + f' | median {sorted(r)[len(r)//2]:.2f} | open RSS/PSS {res[-1][1]:.0f}/{res[-1][2]:.0f}', flush=True)
L.stop_dbus()
