#!/bin/bash
# runs the hidden-realized Flutter probe on the REAL Wayland session (never mapped => invisible), samples, kills only its own pid
S=/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad
mode=$1
env PROBE_MODE=$mode GDK_BACKEND=wayland timeout 60 $S/flt/probe/build/linux/x64/release/bundle/hello_probe >$S/res/probe-real-$mode.log 2>&1 &
sleep 1; pid=$(pgrep -n -f "bundle/hello_probe")
for t in 10 20 35; do sleep $((t - ${prev:-0})); prev=$t; python3 - "$pid" "$t" <<'P'
import sys, os
sys.path.insert(0, '/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad/bin')
import treemem as T
pid = int(sys.argv[1]); st = T.status(pid); ro = T.rollup(pid)
print(f"real-GPU probe mode={sys.argv[2]}s: RSS {st.get('VmRSS',0)/1024:.0f} PSS {ro.get('Pss',0)/1024:.0f} anon {st.get('RssAnon',0)/1024:.0f} thr {st.get('Threads',0)}")
P
done
kill $pid 2>/dev/null; wait $pid 2>/dev/null
hyprctl clients -j | python3 -c "import json,sys; print('clients now:', [c['class'] for c in json.load(sys.stdin)])"
