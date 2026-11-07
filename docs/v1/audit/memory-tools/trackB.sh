#!/bin/bash
S=/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad
cd $S; export LAB_DISPLAY=:96
python3 bin/fleet_lab.py image2 --open tools > res/fleet-image2.log 2>&1
python3 bin/twowin.py ext > res/twowin-ext.log 2>&1
python3 bin/twowin.py hl > res/twowin-hl.log 2>&1
touch res/B.done
