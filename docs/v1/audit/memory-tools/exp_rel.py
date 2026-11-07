"""exp_rel.py APP MODE [--env K=V]: bg -> open -> close(+settle) -> kill net proc + shim MODE -> measure -> reopen latency -> close -> measure"""
import sys, os, time, signal
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
app, mode = sys.argv[1], sys.argv[2]
extra = {}
for i, a in enumerate(sys.argv):
    if a == '--env': k, v = sys.argv[i+1].split('=', 1); extra[k] = v
CF = {'tools': (['--background'], ['--settings'], '^Arcade Tools$'), 'box': (['--background'], ['--show'], 'Arcade Box'),
      'look': (['--background'], ['@FILE'], 'Arcade Look'), 'clipboard': (['--background'], ['--show'], 'Clipboard')}
cfg = CF[app]; cap = app.capitalize()
ext = '--ext' in sys.argv
tag = ('ext_' if ext else '') + mode.replace('+', '_') + ''.join('_' + k + v for k, v in extra.items())
L = Lab(f'{app}-rel-{tag}', extra); jl = f'{RES}/lab-{app}-rel-{tag}.jsonl'; open(jl, 'w').close()
td = L.root + '/trim'; os.makedirs(td, exist_ok=True); L.env['TRIMDIR'] = td; L.env['LD_PRELOAD'] = S + '/bin/trimshim.so'
exe = (f'/var/tmp/arcade-audit-ext/{cap}/squashfs-root/AppRun' if ext else L.img(cap)); fa = L.home + '/sample.txt'; open(fa, 'w').write('hello arcade\n' * 2000)
oa = [fa if a == '@FILE' else a for a in cfg[1]]
L.start(exe, cfg[0]); time.sleep(10); L.snap('bg+10s', jsonl=jl)
def opn(label):
    t = time.time(); L.cli(exe, oa, timeout=15)
    w, _ = wait_for(lambda: (windows(cfg[2]) or [None])[0], 15)
    lat = time.time() - t
    px, tp = wait_for(lambda: (lambda c: c if c and c not in ('000000',) else None)(pixel(w, 24, 24)), 10, 0.03) if w else (None, 0)
    print(f'{label}: window mapped {lat:.2f}s painted {time.time()-t:.2f}s', flush=True); time.sleep(4); L.snap(label + '-open', jsonl=jl); return w
def close(w, settle):
    wmclose(w); time.sleep(settle)
w = opn('o1'); close(w, 35); L.snap('closed+35s', jsonl=jl)
keep, rt = homemem.app_pids(L.home)
for p in keep:
    if T.name(p).startswith('WebKitNetwork'): os.kill(p, signal.SIGTERM)
open(td + '/go', 'w').write(mode); time.sleep(3); L.snap('released:' + mode, jsonl=jl)
w = opn('o2'); close(w, 35); L.snap('closed2+35s', jsonl=jl)
keep, rt = homemem.app_pids(L.home)
for p in keep:
    if T.name(p).startswith('WebKitNetwork'): os.kill(p, signal.SIGTERM)
open(td + '/go', 'w').write(mode); time.sleep(3); L.snap('released2:' + mode, jsonl=jl)
w = opn('o3'); close(w, 5)
L.quit_all()
