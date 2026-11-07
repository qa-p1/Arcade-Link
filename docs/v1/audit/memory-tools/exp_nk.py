import sys, os, time, signal
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
app = sys.argv[1]; cap = app.capitalize()
cfg = {'tools': (['--background'], ['--settings'], '^Arcade Tools$'), 'box': (['--background'], ['--show'], 'Arcade Box')}[app]
L = Lab(f'{app}-nk'); jl = f'{RES}/lab-{app}-nk.jsonl'; open(jl, 'w').close()
exe = L.img(cap)
L.start(exe, cfg[0]); time.sleep(10); L.snap('bg+10s', jsonl=jl)
def opn(label):
    t = time.time(); L.cli(exe, cfg[1], timeout=15)
    w, _ = wait_for(lambda: (windows(cfg[2]) or [None])[0], 15)
    print(label, 'window mapped %.2fs' % (time.time() - t), flush=True); time.sleep(4); L.snap(label + '-open', jsonl=jl); return w
w = opn('o1'); wmclose(w); time.sleep(20); L.snap('closed+20s', jsonl=jl)
keep, rt = homemem.app_pids(L.home)
nk = [p for p in keep if T.name(p).startswith('WebKitNetwork')]
print('network pids', nk, flush=True)
for p in nk: os.kill(p, signal.SIGTERM)
time.sleep(3); L.snap('after-kill-netproc', jsonl=jl)
w = opn('o2-after-kill'); wmclose(w); time.sleep(20); L.snap('closed2+20s', jsonl=jl)
keep, rt = homemem.app_pids(L.home); print('procs', [(p, T.name(p)) for p in keep])
L.quit_all()
