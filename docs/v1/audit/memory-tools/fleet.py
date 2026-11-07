#!/usr/bin/env python3
"""fleet.py [--home HOME] [--json]: memory of every Arcade app tree running for HOME (default: the real $USER home).
Read-only. Selects processes by environ HOME and by exe (.mount_Arcade* / AppImage), groups into app trees by walking ppid
(children such as WebKitWebProcess are attributed to their parent app)."""
import os, sys, re, json, collections
sys.path.insert(0, os.path.dirname(__file__))
import treemem as T

home = sys.argv[sys.argv.index('--home')+1] if '--home' in sys.argv else '/home/harneet'
APP = {'arcade-tools':'tools','arcade-desktop':'box','arcade-look':'look','arcade-find':'find','arcade-lens':'lens','clipboard':'clipboard',
       'arcade-shelf':'shelf','arcade-wheel':'wheel','arcade-tools-overlay':'tools','AppRun.wrapped':None}
def environ_home(p):
    try:
        for kv in open(f'/proc/{p}/environ','rb').read().split(b'\0'):
            if kv.startswith(b'HOME='): return kv[5:].decode()
    except Exception: return None
def ppid(p):
    try:
        s=open(f'/proc/{p}/stat').read(); return int(s[s.rindex(')')+2:].split()[1])
    except Exception: return 0
procs={}
for d in os.listdir('/proc'):
    if not d.isdigit(): continue
    p=int(d)
    if environ_home(p)!=home: continue
    try: exe=os.readlink(f'/proc/{p}/exe')
    except Exception: continue
    procs[p]=exe
# identify app roots
roots={}; stubs={}
for p,exe in procs.items():
    base=os.path.basename(exe)
    if '.mount_Arcade' in exe and base in APP:
        a=APP[base]
        if a is None:
            a={'arcade-wheel':'wheel'}.get(os.path.basename(exe),'wheel')   # Wheel's AppRun.wrapped execs arcade-wheel
        roots[p]=a
    elif re.search(r'Arcade-(\w+)\.AppImage$', exe) or re.search(r'Arcade-(\w+)\.App', T.name(p)):
        m=re.search(r'Arcade-(\w+)', exe) ; stubs[p]=m.group(1).lower() if m else '?'
# attribute children
owner={}
for p in procs:
    q=p; seen=0
    while q>1 and seen<20:
        if q in roots: owner[p]=roots[q]; break
        q=ppid(q); seen+=1
rows=collections.defaultdict(list)
for p,a in owner.items(): rows[a].append(p)
tot=collections.Counter(); out={}
for a,ps in sorted(rows.items()):
    c=collections.Counter(); names=[]
    for p in ps:
        st=T.status(p); ro=T.rollup(p)
        c['rss']+=st.get('VmRSS',0); c['pss']+=ro.get('Pss',0); c['anon']+=st.get('RssAnon',0)+st.get('RssShmem',0); c['swap']+=st.get('VmSwap',0); c['thr']+=st.get('Threads',0)
        c['pss_anon']+=ro.get('Pss_Anon',0)
        names.append(T.name(p)[:14])
    stub=sum(T.status(p).get('VmRSS',0) for p,n in stubs.items() if n==a)
    stubp=sum(T.rollup(p).get('Pss',0) for p,n in stubs.items() if n==a)
    out[a]=dict(c, procs=len(ps), names=names, stub_rss=stub, stub_pss=stubp)
    for k in ('rss','pss','anon','swap','thr'): tot[k]+=c[k]
    tot['stub_rss']+=stub; tot['stub_pss']+=stubp
if '--json' in sys.argv:
    print(json.dumps(dict(apps=out,total=dict(tot)))); sys.exit()
mb=lambda k:f'{k/1024:7.1f}'
print(f"{'app':<10}{'procs':>5} {'RSS':>7} {'PSS':>7} {'anon+shm':>8} {'swap':>7} {'thr':>4} {'stubRSS':>8} {'stubPSS':>8}  processes")
for a,v in out.items():
    print(f"{a:<10}{v['procs']:>5} {mb(v['rss'])} {mb(v['pss'])} {mb(v['anon']):>8} {mb(v['swap'])} {v['thr']:>4} {mb(v['stub_rss']):>8} {mb(v['stub_pss']):>8}  {','.join(sorted(set(v['names'])))}")
print(f"{'FLEET':<10}{'':>5} {mb(tot['rss'])} {mb(tot['pss'])} {mb(tot['anon']):>8} {mb(tot['swap'])} {tot['thr']:>4} {mb(tot['stub_rss']):>8} {mb(tot['stub_pss']):>8}")
