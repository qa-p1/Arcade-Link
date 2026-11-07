#!/usr/bin/env python3
"""topfiles.py PID [N]: top mappings by path, summing Rss/Pss/Anonymous/Swap (KB->MB)."""
import sys, re, collections
pid = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv) > 2 else 15
agg = collections.defaultdict(collections.Counter); cur = None
for line in open(f'/proc/{pid}/smaps'):
    if re.match(r'^[0-9a-f]+-[0-9a-f]+ ', line):
        parts = line.split(None, 5); path = parts[5].strip() if len(parts) > 5 else '[anon]'
        cur = re.sub(r'^/tmp/\.mount_[A-Za-z0-9]+', '$MNT', path)
        agg[cur]['n'] += 1; perm = parts[1]
        if 'x' in perm: agg[cur]['x'] += 1
    else:
        m = re.match(r'(Rss|Pss|Anonymous|Swap|Private_Dirty):\s+(\d+)', line)
        if m: agg[cur][m.group(1)] += int(m.group(2))
rows = sorted(agg.items(), key=lambda kv: -kv[1]['Rss'])[:n]
print(f"{'RSS':>7} {'PSS':>7} {'anon':>7} {'swap':>6} {'n':>4}  path")
for k, v in rows: print(f"{v['Rss']/1024:7.1f} {v['Pss']/1024:7.1f} {v['Anonymous']/1024:7.1f} {v['Swap']/1024:6.1f} {v['n']:>4}  {k[:110]}")
