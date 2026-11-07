"""real_cycle.py APP: on the owner's real desktop: measure tree, `--show`, measure, `--hide`, measure +3s/+settle.
Uses only the app's own CLI. Read-only /proc measurement."""
import sys, os, time, json, subprocess
S = '/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad'
RES = '/home/harneet/Work/Coding/ARCADE_V1/AUDIT/memory'
app = sys.argv[1]; settle = int(sys.argv[2]) if len(sys.argv) > 2 else 40
cap = app.capitalize(); img = os.path.expanduser(f'~/Applications/Arcade/Arcade-{cap}.AppImage')
ENV = {k: v for k, v in os.environ.items() if k not in ('APPIMAGE', 'APPDIR', 'LD_LIBRARY_PATH', 'ARGV0', 'OWD')}
SHOW = {'box': ['--show'], 'clipboard': ['--show'], 'wheel': ['--preview'], 'shelf': ['--show'], 'find': ['--show'], 'lens': ['--capture'], 'look': [S + '/lookme.txt']}[app]
HIDE = ['--hide']
open(S + '/lookme.txt', 'w').write('Arcade memory audit sample\n' * 50)
def fleet():
    o = json.loads(subprocess.run(['python3', S + '/bin/fleet.py', '--json'], capture_output=True, text=True).stdout)['apps'].get(app)
    return o
def mb(k): return round(k / 1024, 1)
def row(label):
    o = fleet(); j = {'label': label, 'app': app, 'rss': mb(o['rss']), 'pss': mb(o['pss']), 'anon': mb(o['anon']), 'swap': mb(o['swap']), 'thr': o['thr'], 'procs': o['procs'], 'stub_rss': mb(o['stub_rss']), 'stub_pss': mb(o['stub_pss'])}
    print(f"{label:<22} RSS {j['rss']:6.1f} PSS {j['pss']:6.1f} anon {j['anon']:6.1f} swap {j['swap']:5.1f} thr {j['thr']:3d} procs {j['procs']} {o['names']}", flush=True)
    with open(f'{RES}/real-cycle-{app}.jsonl', 'a') as f: f.write(json.dumps(j) + '\n')
    return j
def layers():
    d = json.loads(subprocess.run(['hyprctl', 'layers', '-j'], capture_output=True, text=True).stdout)
    out = {}
    for mon, v in d.items():
        for lvl, arr in v.get('levels', {}).items():
            for l in arr: out['L' + l['address']] = ('layer:' + l['namespace'], lvl, l['pid'], True)
    return out
def clients():
    out = layers()
    out.update({c['address']: (c['class'], c['title'][:30], c['pid'], c['mapped']) for c in json.loads(subprocess.run(['hyprctl', 'clients', '-j'], capture_output=True, text=True).stdout)})
    return out
before = clients()
row('before')
t0 = time.time()
subprocess.run([img, *SHOW], env=ENV, capture_output=True, timeout=30)
t_cli = time.time() - t0
new = {}
while time.time() - t0 < 15:
    cur = clients(); new = {a: v for a, v in cur.items() if a not in before and v[3]}
    if new: break
    time.sleep(0.02)
print(f'show cli returned {t_cli:.2f}s; first new mapped window after {time.time()-t0:.2f}s: {new}', flush=True)
time.sleep(4); row('open+4s')
subprocess.run([img, *HIDE], env=ENV, capture_output=True, timeout=30)
time.sleep(3); row('hidden+3s')
time.sleep(settle); row(f'hidden+{settle}s')
after = clients()
print('windows after == before:', {a: v[:2] for a, v in after.items()} == {a: v[:2] for a, v in before.items()}, {a: v[:2] for a, v in after.items() if a not in before}, flush=True)
