"""hot_reshow.py APP: re-show latency while the webview is still alive (hide -> 3 s -> show) vs after release (hide -> 40 s -> show)."""
import sys, os, time
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
app = sys.argv[1]; cap = app.capitalize()
CF = {'box': (['--background'], ['--show'], ['--hide'], 'Arcade Box'), 'look': (['--background'], ['@F'], ['--hide'], 'Arcade Look')}
bg, op, hd, pat = CF[app]
L = Lab(f'{app}-hot'); exe = L.img(cap); f = L.home + '/s.txt'; open(f, 'w').write('x\n' * 200)
op = [f if a == '@F' else a for a in op]
L.start(exe, bg); time.sleep(10)
def show(label):
    before = set(windows(pat))
    t = time.time(); L.cli(exe, op, timeout=15)
    w, _ = wait_for(lambda: next((x for x in windows(pat) if x not in before or True), None), 10, 0.01)
    print(f'{label}: window visible {time.time()-t:.3f}s', flush=True); return w
show('first show (cold webview)'); time.sleep(3)
for i in range(3):
    L.cli(exe, hd, timeout=15); time.sleep(0.5 if i == 0 else 3)
    n = len(windows(pat)); t0 = time.time(); L.cli(exe, op, timeout=15)
    w, _ = wait_for(lambda: windows(pat) and True, 10, 0.01)
    print(f'hot re-show #{i+1} (hidden for {"0.5" if i == 0 else "3"} s; visible windows before={n}): {time.time()-t0:.3f}s', flush=True); time.sleep(2)
L.cli(exe, hd, timeout=15); time.sleep(45)
t0 = time.time(); L.cli(exe, op, timeout=15); wait_for(lambda: windows(pat) and True, 10, 0.01)
print(f'after-release re-show (hidden 45 s): {time.time()-t0:.3f}s', flush=True)
L.quit_all(); L.stop_dbus()
