#!/bin/bash
S=/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad
cd $S; export LAB_DISPLAY=:95
python3 bin/exp_rel.py box trim+dontneed --ext > res/box-rel-ext.log 2>&1
python3 bin/exp_rel.py look trim+dontneed --ext > res/look-rel-ext.log 2>&1
while [ ! -f res/B.done ]; do sleep 3; done
python3 bin/hot_reshow.py look > res/hot-look.log 2>&1
python3 bin/hot_reshow.py box > res/hot-box.log 2>&1
touch res/A.done
