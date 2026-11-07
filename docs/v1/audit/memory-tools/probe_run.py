"""probe_run.py MODE [RECREATE_S]: hello_probe Flutter app on Xvfb (software GL). Modes: realized | destroyview | unrealize."""
import sys, os, time, subprocess
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
mode = sys.argv[1]; rec = sys.argv[2] if len(sys.argv) > 2 else None
L = Lab(f'probe-{mode}{rec or ""}')
env = {'PROBE_MODE': mode}
if rec: env['PROBE_RECREATE'] = rec
exe = S + '/flt/probe/build/linux/x64/release/bundle/hello_probe'
p = L.start(exe, [], env=env)
for t, lab in ((12, 'start+12s'), (20, 'start+20s'), (30, 'start+30s'), (45, 'start+45s')):
    time.sleep(t - (time.time() - (getattr(L, '_t0', None) or time.time()))) if False else None
t0 = time.time()
for lab, at in (('start+10s', 10), ('start+20s', 20), ('start+30s', 30), ('start+45s', 45)):
    time.sleep(max(0, at - (time.time() - t0))); o = L.snap(f'{mode}{rec or ""} {lab}')
print(open(L.root + '/app.log').read()[-400:])
L.quit_all(); L.stop_dbus()
