"""bgenv.py APP VARIANT... : fresh background memory of APP under env variants. VARIANT = name:K=V,K=V  (base for none)
 prints one line per variant: settle 25 s then snapshot; also thread names histogram."""
import sys, os, time, collections
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
app = sys.argv[1]; cap = app.capitalize()
BG = {'tools': ['--background'], 'box': ['--background'], 'look': ['--background'], 'clipboard': ['--background'], 'wheel': ['--background'],
      'shelf': ['--background'], 'find': ['--background'], 'lens': ['--background']}
for v in sys.argv[2:]:
    name, _, envs = v.partition(':')
    extra = dict(kv.split('=', 1) for kv in envs.split(',') if kv)
    L = Lab(f'{app}-bg-{name}', extra); jl = f'{RES}/lab-{app}-bg-{name}.jsonl'; open(jl, 'w').close()
    exe = L.img(cap)
    if 'EXTRACTED' in extra: exe = f'/var/tmp/arcade-audit-ext/{cap}/squashfs-root/AppRun'; extra.pop('EXTRACTED'); L.env.pop('EXTRACTED', None)
    t0 = time.time(); L.start(exe, BG[app]); 
    time.sleep(25)
    o = L.snap(f'{name}', jsonl=jl)
    keep, rt = homemem.app_pids(L.home)
    h = collections.Counter()
    for p in keep:
        for t in os.listdir(f'/proc/{p}/task'):
            try: h[open(f'/proc/{p}/task/{t}/comm').read().strip()] += 1
            except Exception: pass
    print('   threads:', dict(h.most_common(6)), flush=True)
    L.quit_all(); L.stop_dbus(); time.sleep(1)
