"""box_motion.py xvfb|real: Box's own motion probe (ARCADE_MOTION_PROBE, 700 ms of requestAnimationFrame intervals after show) with
WebKit compositing ON vs OFF. Prints MOTION_PROFILE lines. real = isolated instance on the real Wayland session (no Hyprland env)."""
import sys, os, time, re, subprocess
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
where = sys.argv[1]
V = (('comp', {}), ('nocomp', {'WEBKIT_DISABLE_COMPOSITING_MODE': '1'}), ('comp2', {}), ('nocomp2', {'WEBKIT_DISABLE_COMPOSITING_MODE': '1'}))
for variant, extra in (V if where == 'xvfb' else V[:2]):
    L = Lab(f'boxmotion-{where}-{variant}', {'ARCADE_MOTION_PROBE': '1', **extra})
    if where == 'real':
        L.env.update(WAYLAND_DISPLAY='/run/user/1000/wayland-1', GDK_BACKEND='wayland', XDG_SESSION_TYPE='wayland'); L.env.pop('DISPLAY', None)
    exe = L.img('Box'); L.start(exe, ['--background']); time.sleep(8)
    for i in range(5 if where == 'xvfb' else 4):
        L.cli(exe, ['--show'], timeout=15); time.sleep(2.5); L.cli(exe, ['--hide'], timeout=15); time.sleep(2)
    time.sleep(1)
    log = open(L.root + '/app.log', errors='replace').read()
    rows = [re.search(r'MOTION_PROFILE (\{.*\})', l).group(1) for l in log.split('\n') if 'MOTION_PROFILE' in l]
    print(f'== {where} {variant}: {len(rows)} samples')
    for r in rows: print('  ', r)
    L.cli(exe, ['--quit'], timeout=15); time.sleep(2); L.quit_all(); L.stop_dbus()
