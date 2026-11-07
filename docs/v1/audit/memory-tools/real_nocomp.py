"""real_nocomp.py APP: isolated instance (private HOME/dbus/runtime, NO hyprland env) on the REAL Wayland session, compositing on vs off.
Opens the app's window briefly, measures, screenshots with grim, closes with the app's own --quit."""
import sys, os, time, json, subprocess
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
app = sys.argv[1]; cap = app.capitalize()
for variant, extra in (('comp', {}), ('nocomp', {'WEBKIT_DISABLE_COMPOSITING_MODE': '1'})):
    L = Lab(f'{app}-realwl-{variant}', extra)
    L.env.update(WAYLAND_DISPLAY='/run/user/1000/wayland-1', GDK_BACKEND='wayland', QT_QPA_PLATFORM='wayland', XDG_SESSION_TYPE='wayland')
    L.env.pop('DISPLAY', None)
    exe = L.img(cap); open(L.home + '/s.txt', 'w').write('Arcade audit sample\n' * 200)
    args = {'tools': ['--settings'], 'look': [L.home + '/s.txt']}[app]
    L.start(exe, ['--background']); time.sleep(8)
    before = {c['address'] for c in json.loads(subprocess.run(['hyprctl', 'clients', '-j'], capture_output=True, text=True).stdout)}
    t = time.time(); L.cli(exe, args, timeout=20)
    new = None
    while time.time() - t < 15 and not new:
        cl = json.loads(subprocess.run(['hyprctl', 'clients', '-j'], capture_output=True, text=True).stdout)
        new = next((c for c in cl if c['address'] not in before and c['mapped']), None); time.sleep(0.02)
    lat = time.time() - t; time.sleep(5)
    o = L.snap(f'{app} real-wayland {variant} open')
    if new:
        g = f"{new['at'][0]},{new['at'][1]} {new['size'][0]}x{new['size'][1]}"
        subprocess.run(['grim', '-g', g, f'{RES}/shot-real-{app}-{variant}.png'], timeout=10)
    print(f'  window mapped after {lat:.2f}s  client={new and (new["class"], new["size"])}', flush=True)
    L.cli(exe, ['--quit'], timeout=15); time.sleep(3)
    L.quit_all(); L.stop_dbus()
after = json.loads(subprocess.run(['hyprctl', 'clients', '-j'], capture_output=True, text=True).stdout)
print('clients now:', [c['class'] for c in after])
