#!/usr/bin/env python3
"""homemem.py HOMEDIR [--maps] [--label X] [--jsonl FILE]
Finds all processes whose environ has HOME=HOMEDIR, drops infra (dbus, xdg portals, Xvfb, gvfs, at-spi, etc.)
and the AppImage runtime stub, and reports the app process tree memory."""
import os, sys, json, time, collections
sys.path.insert(0, os.path.dirname(__file__))
import treemem as T

INFRA = ('dbus-','xdg-','Xvfb','gvfs','at-spi','dconf','gnome-keyring','pipewire','wireplumber','xdotool','bash','sh','sleep','timeout','python','fusermount','xdg-desktop-por','xdg-document-p','xdg-permission','gsd-','ibus')
def home_pids(home):
    out = []
    for d in os.listdir('/proc'):
        if not d.isdigit(): continue
        try:
            env = open(f'/proc/{d}/environ','rb').read().split(b'\0')
        except Exception: continue
        if f'HOME={home}'.encode() in env: out.append(int(d))
    return out

def app_pids(home):
    allp = home_pids(home)
    keep, runtime = [], []
    for p in allp:
        n = T.name(p)
        if any(n.startswith(i) for i in INFRA): continue
        try: exe = os.readlink(f'/proc/{p}/exe')
        except Exception: exe = ''
        if n.startswith('Arcade-') and exe.endswith('.AppImage'):
            runtime.append(p); continue
        keep.append(p)
    return keep, runtime

def measure(home, maps=False):
    keep, runtime = app_pids(home)
    procs = []; tot = collections.Counter(); classes = collections.defaultdict(collections.Counter)
    for p in keep:
        st = T.status(p); ro = T.rollup(p)
        rec = dict(pid=p, name=T.name(p), rss=st.get('VmRSS',0), anon=st.get('RssAnon',0), file=st.get('RssFile',0), shmem=st.get('RssShmem',0),
                   swap=st.get('VmSwap',0), threads=st.get('Threads',0), pss=ro.get('Pss',0), pss_anon=ro.get('Pss_Anon',0),
                   pss_file=ro.get('Pss_File',0), pss_shmem=ro.get('Pss_Shmem',0), vmsize=st.get('VmSize',0))
        if maps:
            sm = T.smaps(p)
            rec['maps'] = {k: dict(rss=v['Rss'], pss=v['Pss'], anon=v['Anonymous'], swap=v['Swap'], n=v['n']) for k,v in sm.items()}
            for k, v in sm.items():
                for kk in ('Rss','Pss','Anonymous','Swap','n'): classes[k][kk] += v[kk]
        procs.append(rec)
        for k in ('rss','anon','file','shmem','swap','threads','pss','pss_anon','pss_file','pss_shmem'): tot[k] += rec[k]
    rt = sum(T.status(p).get('VmRSS',0) for p in runtime)
    rtp = sum(T.rollup(p).get('Pss',0) for p in runtime)
    out = dict(t=time.time(), nproc=len(procs), total=dict(tot), procs=procs, runtime_rss=rt, runtime_pss=rtp)
    if maps: out['classes'] = {k: dict(v) for k,v in classes.items()}
    return out

def pretty(o):
    mb = lambda k: f'{k/1024:8.1f}'
    print(f"{'pid':>7} {'name':<16} {'RSS':>8} {'PSS':>8} {'anon':>8} {'file':>8} {'shmem':>8} {'swap':>8} {'thr':>4}")
    for r in sorted(o['procs'], key=lambda r:-r['rss']):
        print(f"{r['pid']:>7} {r['name']:<16} {mb(r['rss'])} {mb(r['pss'])} {mb(r['anon'])} {mb(r['file'])} {mb(r['shmem'])} {mb(r['swap'])} {r['threads']:>4}")
    t = o['total']
    print(f"{'TOTAL':>7} {o['nproc']:<16} {mb(t['rss'])} {mb(t['pss'])} {mb(t['anon'])} {mb(t['file'])} {mb(t['shmem'])} {mb(t['swap'])} {t['threads']:>4}   (+AppImage runtime stub rss={o['runtime_rss']/1024:.1f} pss={o['runtime_pss']/1024:.1f})")
    if 'classes' in o:
        print('\nby mapping class (MB): rss  pss  anon  swap  nmaps')
        for k, v in sorted(o['classes'].items(), key=lambda kv: -kv[1]['Pss']):
            if v['Rss'] < 512 and v['Pss'] < 512: continue
            print(f"  {k:<22} {mb(v['Rss'])} {mb(v['Pss'])} {mb(v['Anonymous'])} {mb(v['Swap'])} {v['n']:>6}")

if __name__ == '__main__':
    home = sys.argv[1]; maps = '--maps' in sys.argv
    label = sys.argv[sys.argv.index('--label')+1] if '--label' in sys.argv else ''
    o = measure(home, maps); o['label'] = label
    if '--jsonl' in sys.argv:
        with open(sys.argv[sys.argv.index('--jsonl')+1], 'a') as f: f.write(json.dumps(o)+'\n')
    if '--quiet' not in sys.argv: pretty(o)
