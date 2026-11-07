import sys, os, time, subprocess, statistics
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
L = Lab('tools-opentrim'); jl = f'{RES}/lab-tools-opentrim.jsonl'; open(jl, 'w').close()
td = L.root + '/trim'; os.makedirs(td, exist_ok=True); L.env['TRIMDIR'] = td; L.env['LD_PRELOAD'] = S + '/bin/trimshim.so'
exe = L.img('Tools'); L.start(exe, ['--background']); time.sleep(10)
L.cli(exe, ['--settings'], timeout=15)
w, _ = wait_for(lambda: (windows('^Arcade Tools$') or [None])[0], 15); time.sleep(5)
def shotsig(w):
    out = subprocess.run(['import', '-window', str(w), '-crop', '700x500+200+120', '-resize', '50x35!', 'txt:-'], env={**os.environ, 'DISPLAY': DISPLAY}, capture_output=True, text=True).stdout
    return hash(out)
def click_tab(y):
    xdo('mousemove', '--window', str(w), '60', str(y)); a = shotsig(w); t = time.time(); xdo('click', '1')
    while time.time() - t < 3:
        if shotsig(w) != a: return time.time() - t
    return 3.0
def bench(label):
    xs = []
    for rep in range(2):
        for y in (121, 229, 337, 85): xs.append(click_tab(y)); time.sleep(0.4)
    print(f'{label}: click->paint ms median {statistics.median(xs)*1000:.0f} max {max(xs)*1000:.0f} all {[round(x*1000) for x in xs]}', flush=True)
L.snap('open-idle', jsonl=jl); bench('before-trim')
L.snap('open-after-bench', jsonl=jl)
open(td + '/go', 'w').write('trim+dontneed'); time.sleep(3); L.snap('open-trimmed-all-procs', jsonl=jl); bench('after-trim')
L.snap('open-after-bench2', jsonl=jl)
open(td + '/go', 'w').write('trim+pageout+anon+dontneed'); time.sleep(3); L.snap('open-trimmed+anon', jsonl=jl); bench('after-trim+anon')
L.snap('open-final', jsonl=jl)
L.quit_all(); L.stop_dbus()
