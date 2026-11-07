#!/bin/bash
S=/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad
cd $S
while [ ! -f res/B.done ]; do sleep 3; done
export LAB_DISPLAY=:96
for app in tools box look; do
  python3 bin/fleet_lab.py neutral-$app --ext --shim --opentrim --settle 45 --open $app > res/fleet-neutral-$app.log 2>&1
  python3 bin/fleet_lab.py nocomp-$app --ext --shim --opentrim --settle 45 --open $app --env WEBKIT_DISABLE_COMPOSITING_MODE=1 > res/fleet-nocomp-$app.log 2>&1
done
touch res/C.done
