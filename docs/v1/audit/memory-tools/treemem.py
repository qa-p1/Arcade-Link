#!/usr/bin/env python3
"""treemem.py PID [--json] [--maps] : memory of a process tree (PID + all descendants)
Reads only /proc/<pid>/{status,smaps_rollup,smaps,task,stat}. Read-only."""
import os, sys, re, json, collections

def kids_of(root):
    ppid = {}
    for d in os.listdir('/proc'):
        if d.isdigit():
            try:
                with open(f'/proc/{d}/stat') as f:
                    s = f.read()
                rp = s.rindex(')')
                parts = s[rp+2:].split()
                ppid[int(d)] = int(parts[1])
            except Exception:
                pass
    out = [root]; i = 0
    while i < len(out):
        for p, pp in ppid.items():
            if pp == out[i] and p not in out:
                out.append(p)
        i += 1
    return out

def kv(path):
    d = {}
    try:
        with open(path) as f:
            for line in f:
                m = re.match(r'(\w+):\s+(\d+)', line)
                if m: d[m.group(1)] = int(m.group(2))
    except Exception:
        pass
    return d

def name(p):
    try:
        with open(f'/proc/{p}/comm') as f: return f.read().strip()
    except Exception: return '?'

def status(p):
    s = kv(f'/proc/{p}/status')
    s['Threads'] = s.get('Threads', 0)
    return s

def rollup(p):
    return kv(f'/proc/{p}/smaps_rollup')

CLASSES = [
 (r'^\[heap\]$', 'heap(brk)'),
 (r'^\[stack', 'stack(main)'),
 (r'^\[anon:.*(v8|JIT|jit)', 'anon-jit'),
 (r'^/memfd:', 'memfd'),
 (r'^/dev/dri|/dev/nvidia|/dev/kgsl', 'gpu-dri'),
 (r'^/dev/shm|^/SYSV|shmem', 'shm'),
 (r'\.sqlite|\.db(-wal|-shm)?$|-wal$|-shm$', 'sqlite-mmap'),
 (r'\.(ttf|otf|ttc|pfb)$', 'fonts'),
 (r'icon-theme\.cache|\.cache$|mesa_shader_cache', 'caches'),
 (r'/libwebkit|/libjavascriptcoregtk', 'webkit-libs'),
 (r'/libflutter|libapp\.so|libarcade_core|flutter_assets', 'flutter'),
 (r'/libQt6|/qt6|\.qml|/libQt', 'qt-libs'),
 (r'/libLLVM|llvmpipe|swrast|libgallium|libvulkan_lvp|dri\.so|/mesa', 'mesa/llvm'),
 (r'\.so(\.|$)', 'other-libs'),
 (r'^$', 'anon(mmap/arena)'),
]
def classify(path):
    if path.startswith('[heap]'): return 'heap(brk)'
    if path.startswith('[stack'): return 'stack(main)'
    for pat, c in CLASSES:
        if re.search(pat, path): return c
    if path.startswith('['): return path.split(' ')[0]
    if path.startswith('/'): return 'file:other'
    return 'anon(mmap/arena)'

def smaps(p):
    """group smaps by class; returns dict class -> {rss,pss,anon,swap,n}"""
    res = collections.defaultdict(lambda: collections.Counter())
    cur = None
    try:
        f = open(f'/proc/{p}/smaps')
    except Exception:
        return res
    with f:
        for line in f:
            if re.match(r'^[0-9a-f]+-[0-9a-f]+ ', line):
                parts = line.split(None, 5)
                path = parts[5].strip() if len(parts) > 5 else ''
                cur = classify(path); res[cur]['n'] += 1
                res[cur]['_last'] = 0
            else:
                m = re.match(r'(Rss|Pss|Anonymous|Swap|Shared_Clean|Shared_Dirty|Private_Clean|Private_Dirty):\s+(\d+)', line)
                if m and cur:
                    res[cur][m.group(1)] += int(m.group(2))
    return res

def main():
    root = int(sys.argv[1]); as_json = '--json' in sys.argv; maps = '--maps' in sys.argv
    pids = kids_of(root)
    procs = []; tot = collections.Counter(); classes = collections.defaultdict(collections.Counter)
    for p in pids:
        st = status(p); ro = rollup(p)
        rec = dict(pid=p, name=name(p), rss=st.get('VmRSS',0), anon=st.get('RssAnon',0), file=st.get('RssFile',0),
                   shmem=st.get('RssShmem',0), swap=st.get('VmSwap',0), threads=st.get('Threads',0),
                   pss=ro.get('Pss',0), pss_anon=ro.get('Pss_Anon',0), pss_file=ro.get('Pss_File',0), pss_shmem=ro.get('Pss_Shmem',0),
                   vmsize=st.get('VmSize',0), vmdata=st.get('VmData', 0))
        if maps:
            sm = smaps(p)
            rec['maps'] = {k: dict(rss=v['Rss'], pss=v['Pss'], anon=v['Anonymous'], swap=v['Swap'], n=v['n']) for k, v in sm.items()}
            for k, v in sm.items():
                for kk in ('Rss','Pss','Anonymous','Swap','n'): classes[k][kk] += v[kk]
        procs.append(rec)
        for k in ('rss','anon','file','shmem','swap','threads','pss','pss_anon','pss_file','pss_shmem'): tot[k] += rec[k]
    out = dict(root=root, nproc=len(procs), total=dict(tot), procs=procs)
    if maps: out['classes'] = {k: dict(v) for k, v in classes.items()}
    if as_json:
        print(json.dumps(out))
        return
    mb = lambda k: f'{k/1024:8.1f}'
    print(f"{'pid':>7} {'name':<16} {'RSS':>8} {'PSS':>8} {'anon':>8} {'file':>8} {'shmem':>8} {'swap':>8} {'thr':>4}")
    for r in procs:
        print(f"{r['pid']:>7} {r['name']:<16} {mb(r['rss'])} {mb(r['pss'])} {mb(r['anon'])} {mb(r['file'])} {mb(r['shmem'])} {mb(r['swap'])} {r['threads']:>4}")
    t = tot
    print(f"{'TOTAL':>7} {len(procs)+0:<16} {mb(t['rss'])} {mb(t['pss'])} {mb(t['anon'])} {mb(t['file'])} {mb(t['shmem'])} {mb(t['swap'])} {t['threads']:>4}")
    if maps:
        print('\nby mapping class (all procs; MB):  rss  pss  anon  swap  nmaps')
        for k, v in sorted(classes.items(), key=lambda kv: -kv[1]['Pss']):
            if v['Rss'] < 256 and v['Pss'] < 256 and v['Swap'] < 256: continue
            print(f"  {k:<22} {mb(v['Rss'])} {mb(v['Pss'])} {mb(v['Anonymous'])} {mb(v['Swap'])} {v['n']:>6}")
if __name__ == "__main__":
    main()
