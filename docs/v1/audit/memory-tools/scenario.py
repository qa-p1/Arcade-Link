#!/usr/bin/env python3
"""scenario.py APP [--tag T] [--env K=V ...] [--trim] [--extracted] [--cycles N] [--wait SEC]
bg -> open -> close -> settle, with optional LD_PRELOAD release shim steps. Appends to res/APP-TAG.jsonl"""
import sys, os, time, json
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
args = sys.argv[1:]
app = args[0]
tag = args[args.index('--tag')+1] if '--tag' in args else 'base'
cycles = int(args[args.index('--cycles')+1]) if '--cycles' in args else 1
settle = int(args[args.index('--wait')+1]) if '--wait' in args else 60
extra = {}
for i, a in enumerate(args):
    if a == '--env': k, v = args[i+1].split('=', 1); extra[k] = v
trim = '--trim' in args
extracted = '--extracted' in args
APPCFG = {
 'tools':     dict(bg=['--background'], open=[], pat='^Arcade Tools$', closer='wm'),
 'box':       dict(bg=['--background'], open=['--show'], pat='Arcade Box', closer='wm'),
 'look':      dict(bg=['--background'], open=['@FILE'], pat='Arcade Look', closer='wm'),
 'clipboard': dict(bg=['--background'], open=['--show'], pat='Clipboard', closer='wm'),
 'wheel':     dict(bg=['--background'], open=['--preview'], pat='.', closer='hide'),
 'shelf':     dict(bg=['--background'], open=['--show'], pat='Shelf', closer='hide'),
 'find':      dict(bg=['--background'], open=['--show'], pat='Find', closer='hide'),
 'lens':      dict(bg=['--background'], open=['--capture'], pat='Lens', closer='hide'),
}
cfg = APPCFG[app]
name = f'{app}-{tag}'
L = Lab(name, extra)
trimdir = L.root + '/trim'; os.makedirs(trimdir, exist_ok=True)
L.env['TRIMDIR'] = trimdir
L.env['LD_PRELOAD'] = S + '/bin/trimshim.so'
jl = f'{RES}/lab-{name}.jsonl'; open(jl, 'w').close()
cap = app.capitalize()
exe = L.img(cap)
if extracted: exe = f'/var/tmp/arcade-audit-ext/{cap}/squashfs-root/AppRun'
fa = L.home + '/sample.txt'
open(fa, 'w').write('hello arcade\n' * 2000)
def go(mode):
    open(trimdir + '/go', 'w').write(mode); time.sleep(2.5)
def S_(label, maps=False): return L.snap(label, maps=maps, jsonl=jl)
print(f'== {name} exe={exe} env={extra}', flush=True)
t0 = time.time()
p = L.start(exe, cfg['bg'])
time.sleep(10); S_('bg+10s'); time.sleep(20); S_('bg+30s', maps=True)
for c in range(1, cycles+1):
    oa = [fa if a == '@FILE' else a for a in cfg['open']]
    tb = time.time()
    if app == 'tools' and not oa: rc, out, dt = L.cli(exe, ['--settings'], timeout=15)
    else: rc, out, dt = L.cli(exe, oa, timeout=15)
    w, tm = wait_for(lambda: (windows(cfg['pat']) or [None])[0], 15)
    lat = time.time() - tb
    pt = None
    if w:
        px, tp = wait_for(lambda: (lambda c: c if c and c not in ('000000', 'FFFFFF') else None)(pixel(w, 24, 24)), 10, 0.03)
        pt = time.time() - tb
    print(f'cycle {c}: open cmd rc={rc} {dt:.2f}s window_mapped={lat:.2f}s painted={pt}', flush=True)
    time.sleep(4); S_(f'c{c}-open+4s', maps=(c == cycles)); shot(f'{RES}/shot-{name}-c{c}.png', w if (w and app in ('tools','box','look','clipboard')) else None)
    if cfg['closer'] == 'wm' and w: wmclose(w)
    else: L.cli(exe, ['--hide'], timeout=10)
    time.sleep(3); S_(f'c{c}-closed+3s')
    time.sleep(settle if c == cycles else 8); S_(f'c{c}-closed+{settle if c == cycles else 8}s', maps=(c == cycles))
if trim:
    for mode in ('trim', 'pageout', 'trim+pageout+anon', 'trim+pageout+anon+dontneed'):
        go(mode); S_(f'after-{mode}')
print('done', flush=True)
L.quit_all()
