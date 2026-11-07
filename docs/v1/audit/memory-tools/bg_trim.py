"""bg_trim.py APP [ext|image]: fresh background, then shim trim+dontneed at +20 s, measure immediately and after 30 s idle."""
import sys, os, time
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
app = sys.argv[1]; mode = sys.argv[2] if len(sys.argv) > 2 else 'ext'; cap = app.capitalize()
extra = dict(kv.split('=',1) for kv in sys.argv[3:])
L = Lab(f'{app}-bgtrim-{mode}' + ''.join('-'+k[:4] for k in extra), extra); td = L.root + '/trim'; os.makedirs(td, exist_ok=True); open(td + '/go', 'w').write('init')
L.env['TRIMDIR'] = td; L.env['LD_PRELOAD'] = S + '/bin/trimshim.so'
exe = f'/var/tmp/arcade-audit-ext/{cap}/squashfs-root/AppRun' if mode == 'ext' else L.img(cap)
L.start(exe, ['--background']); time.sleep(20); L.snap(f'{app} {mode} bg+20s')
open(td + '/go', 'w').write('trim+dontneed'); time.sleep(3); L.snap(f'{app} {mode} after trim')
time.sleep(30); L.snap(f'{app} {mode} idle +30s')
L.quit_all(); L.stop_dbus()
