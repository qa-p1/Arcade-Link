#!/usr/bin/env python3
"""appmaps.py [--home H] [--app NAME]: per-app PSS/RSS by mapping category, summed over the app tree (read-only /proc)."""
import os, sys, re, collections, json
sys.path.insert(0, os.path.dirname(__file__))
src = open(os.path.join(os.path.dirname(__file__), 'fleet.py')).read().split("rows=collections.defaultdict(list)")[0]
sys.argv_saved = sys.argv; exec(src)   # gives procs, roots, stubs, owner (and T)
rows = collections.defaultdict(list)
for p, a in owner.items(): rows[a].append(p)
CATS = [
 (r'^\[heap\]', 'heap(brk)'), (r'^\[stack', 'stack'), (r'^\[(vdso|vvar|vsyscall)', 'kernel-maps'),
 (r'^/memfd:|^/SYSV|^/dev/shm|^/dev/dri|^/dev/nvidia', 'shm/memfd/gpu'),
 (r'libwebkit2gtk|libjavascriptcore', 'webkit+jsc'),
 (r'libLLVM', 'libLLVM'), (r'libgallium|swrast|dri\.so|libvulkan|libEGL_mesa|libGLX_mesa|libgbm|radeonsi|libvulkan_', 'mesa'),
 (r'libQt6|/qt6/|libicu.*Qt', 'qt-libs'),
 (r'libflutter|flutter_assets|libapp\.so', 'flutter'),
 (r'libarcade_core|libclipboard', 'rust-core-lib'),
 (r'icudata', 'icu-data'), (r'\.(ttf|otf|ttc)$', 'fonts'),
 (r'\.(sqlite|db|db-wal|db-shm)', 'sqlite'),
 (r'/usr/bin/arcade|/clipboard$|arcade-(tools|look|find|lens|shelf|wheel|desktop)$|/AppRun', 'app-binary'),
 (r'\$MNT.*\.so', 'bundled-libs'), (r'^/usr/lib.*\.so', 'system-libs'), (r'^/usr/(share|lib/locale)', 'share-data'),
]
def cat(path):
    if not path: return 'anon'
    for pat, c in CATS:
        if re.search(pat, path): return c
    if path.startswith('['): return 'other[]'
    return 'file:other'
want = sys.argv[sys.argv.index('--app')+1] if '--app' in sys.argv else None
out = {}
for a, ps in sorted(rows.items()):
    if want and a != want: continue
    agg = collections.defaultdict(collections.Counter)
    for p in ps:
        cur = None
        try:
            for line in open(f'/proc/{p}/smaps'):
                if re.match(r'^[0-9a-f]+-[0-9a-f]+ ', line):
                    parts = line.split(None, 5); path = parts[5].strip() if len(parts) > 5 else ''
                    path = re.sub(r'^/tmp/\.mount_[A-Za-z0-9]+', '$MNT', path)
                    cur = cat(path)
                else:
                    m = re.match(r'(Rss|Pss|Anonymous|Swap):\s+(\d+)', line)
                    if m: agg[cur][m.group(1)] += int(m.group(2))
        except Exception: pass
    out[a] = {k: dict(rss=v['Rss']/1024, pss=v['Pss']/1024, anon=v['Anonymous']/1024, swap=v['Swap']/1024) for k, v in agg.items()}
if '--json' in sys.argv: print(json.dumps(out)); sys.exit()
for a, d in out.items():
    tr = sum(v['rss'] for v in d.values()); tp = sum(v['pss'] for v in d.values())
    print(f"== {a}: RSS {tr:.1f} PSS {tp:.1f}")
    for k, v in sorted(d.items(), key=lambda kv: -kv[1]['pss'])[:9]:
        print(f"   {k:<14} rss {v['rss']:6.1f} pss {v['pss']:6.1f} anon {v['anon']:6.1f} swap {v['swap']:5.1f}")
