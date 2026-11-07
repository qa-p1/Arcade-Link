#!/bin/bash
S=/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad
mkdir -p $S/lab/probe-trim; echo init > $S/lab/probe-trim/go
env PROBE_MODE=realized GDK_BACKEND=wayland TRIMDIR=$S/lab/probe-trim LD_PRELOAD=$S/bin/trimshim.so $S/flt/probe/build/linux/x64/release/bundle/hello_probe >$S/res/probe-real-trim.log 2>&1 &
sleep 1; pid=$(pgrep -n -f "bundle/hello_probe"); sleep 12
m() { python3 - "$pid" "$1" <<'P'
import sys
sys.path.insert(0, '/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad/bin')
import treemem as T
pid = int(sys.argv[1]); st = T.status(pid); ro = T.rollup(pid)
print(f"real-GPU hidden Flutter probe {sys.argv[2]}: RSS {st.get('VmRSS',0)/1024:.0f} PSS {ro.get('Pss',0)/1024:.0f} anon {st.get('RssAnon',0)/1024:.0f} swap {st.get('VmSwap',0)/1024:.0f} thr {st.get('Threads',0)}")
P
}
m before; echo "trim+dontneed" > $S/lab/probe-trim/go; sleep 3; m "after trim+dontneed"
echo "trim+pageout+anon+dontneed" > $S/lab/probe-trim/go; sleep 3; m "after +anon pageout"
sleep 5; m "8s later (re-touched)"
kill $pid; wait $pid 2>/dev/null; hyprctl clients -j | python3 -c "import json,sys; print('clients now:', [c['class'] for c in json.load(sys.stdin)])"
