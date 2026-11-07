import sys, os, time, collections
sys.path.insert(0, os.path.dirname(__file__))
os.environ['LAB_DISPLAY'] = ':97'
from lab import *
def descendants(root):
    pp = {}
    for d in os.listdir('/proc'):
        if d.isdigit():
            try: s = open(f'/proc/{d}/stat').read(); pp[int(d)] = int(s[s.rindex(')')+2:].split()[1])
            except Exception: pass
    out = [root]
    for q in out: out += [p for p, x in pp.items() if x == q and p not in out]
    return out
page = "<html><body style='font:14px sans-serif'>" + ''.join(f'<div class=r><b>Row {i}</b> some text content to lay out for the page</div>' for i in range(300)) + '</body></html>'
for variant, extra in (('comp', {}), ('nocomp', {'WEBKIT_DISABLE_COMPOSITING_MODE': '1'})):
    L = Lab(f'wkfloor-{variant}', extra)
    p = L.start('/usr/bin/python3', [S + '/bin/wk_probe.py', page]); time.sleep(12)
    tot = collections.Counter(); rows = []
    for q in descendants(p.pid):
        st = T.status(q); ro = T.rollup(q); rows.append((T.name(q)[:16], st.get('VmRSS', 0)/1024, ro.get('Pss', 0)/1024, st.get('RssAnon', 0)/1024))
        tot['rss'] += st.get('VmRSS', 0)/1024; tot['pss'] += ro.get('Pss', 0)/1024
    print(f'system WebKit 2.54 minimal 940x780 page, {variant}: RSS {tot["rss"]:.0f} PSS {tot["pss"]:.0f} | ' + ' '.join(f'{n}:{r:.0f}/{ps:.0f}(anon {a:.0f})' for n, r, ps, a in rows), flush=True)
    L.quit_all(); L.stop_dbus()
