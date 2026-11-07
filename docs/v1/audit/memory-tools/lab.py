"""lab.py: isolated Arcade app lab on Xvfb :95 (private HOME/XDG/dbus per lab). Read/measure only."""
import os, sys, subprocess, time, json, re, signal, shutil
S = '/tmp/claude-1000/-home-harneet-Work-Coding/69c33f8a-aad7-46f7-9ffa-5d4305bade94/scratchpad'
sys.path.insert(0, S + '/bin')
import homemem, treemem as T
DISPLAY = os.environ.get('LAB_DISPLAY', ':95')
APPS = os.path.expanduser('~/Applications/Arcade')
RES = '/home/harneet/Work/Coding/ARCADE_V1/AUDIT/memory'
LABS = S + '/lab'

def xvfb():
    pf = f'{LABS}/xvfb-{DISPLAY.strip(":")}.pid'
    if os.path.exists(pf):
        try:
            os.kill(int(open(pf).read()), 0); return
        except Exception: pass
    env = {k: v for k, v in os.environ.items() if k not in ('WAYLAND_DISPLAY', 'DISPLAY')}
    p = subprocess.Popen(['Xvfb', DISPLAY, '-screen', '0', '1920x1080x24', '-nolisten', 'tcp'], env=env, start_new_session=True,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    open(pf, 'w').write(str(p.pid)); time.sleep(1)

class Lab:
    def __init__(self, name, extra_env=None):
        self.name = name; self.root = f'{LABS}/{name}'; self.home = self.root + '/home'
        for d in ('home', 'config', 'data', 'cache', 'state', 'run'):
            os.makedirs(f'{self.root}/{d}', exist_ok=True)
        import hashlib
        self.run = '/tmp/arl-' + hashlib.md5(name.encode()).hexdigest()[:8]
        os.makedirs(self.run, exist_ok=True); os.chmod(self.run, 0o700)
        xvfb()
        self.env = {k: v for k, v in os.environ.items() if not (k.startswith('ARCADE_') or k in (
            'WAYLAND_DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'DBUS_SESSION_BUS_ADDRESS', 'APPIMAGE', 'APPDIR', 'LD_LIBRARY_PATH',
            'SWAYSOCK', 'QT_QPA_PLATFORM', 'ARGV0', 'OWD', 'XDG_SESSION_TYPE', 'XDG_CURRENT_DESKTOP'))}
        self.env.update(HOME=self.home, XDG_CONFIG_HOME=self.root + '/config', XDG_DATA_HOME=self.root + '/data',
                        XDG_CACHE_HOME=self.root + '/cache', XDG_STATE_HOME=self.root + '/state', XDG_RUNTIME_DIR=self.run,
                        DISPLAY=DISPLAY, GDK_BACKEND='x11', ARCADE_NO_INSTALL_PROMPT='1', CARGO_HOME='/home/harneet/.cargo')
        self.env.update(extra_env or {})
        self.dbus_pid = None
        self._dbus()
        self.log = open(self.root + '/app.log', 'ab')
        self.procs = []

    def _dbus(self):
        af = self.root + '/dbus.addr'
        if os.path.exists(af):
            pf = self.root + '/dbus.pid'
            try:
                os.kill(int(open(pf).read()), 0); self.env['DBUS_SESSION_BUS_ADDRESS'] = open(af).read().strip(); return
            except Exception: pass
        out = subprocess.run(['dbus-daemon', '--session', '--fork', '--print-address', '--print-pid'], env=self.env, capture_output=True, text=True).stdout.split('\n')
        open(af, 'w').write(out[0]); open(self.root + '/dbus.pid', 'w').write(out[1])
        self.env['DBUS_SESSION_BUS_ADDRESS'] = out[0]

    def start(self, exe, args=(), env=None, cwd=None):
        e = dict(self.env); e.update(env or {})
        p = subprocess.Popen([exe, *args], cwd=cwd or self.home, env=e, stdout=self.log, stderr=self.log, start_new_session=True)
        self.procs.append(p); return p

    def cli(self, exe, args=(), timeout=30, env=None):
        e = dict(self.env); e.update(env or {})
        t0 = time.time()
        try:
            r = subprocess.run([exe, *args], cwd=self.home, env=e, capture_output=True, timeout=timeout, text=True)
            return r.returncode, r.stdout, time.time() - t0
        except subprocess.TimeoutExpired:
            return -1, '', time.time() - t0

    def img(self, app): return f'{APPS}/Arcade-{app}.AppImage'

    def measure(self, maps=False):
        return homemem.measure(self.home, maps)

    def snap(self, label, maps=False, jsonl=None, quiet=False):
        o = self.measure(maps); o['label'] = label; o['lab'] = self.name
        if jsonl:
            with open(jsonl, 'a') as f: f.write(json.dumps(o) + '\n')
        if not quiet: print(fmt(o), flush=True)
        return o

    def quit_all(self, grace=3):
        keep, runtime = homemem.app_pids(self.home)
        for p in keep + runtime:
            try: os.kill(p, signal.SIGTERM)
            except Exception: pass
        time.sleep(grace)
        keep, runtime = homemem.app_pids(self.home)
        for p in keep + runtime:
            try: os.kill(p, signal.SIGKILL)
            except Exception: pass

    def stop_dbus(self):
        try: os.kill(int(open(self.root + '/dbus.pid').read()), signal.SIGTERM)
        except Exception: pass
        for f in ('dbus.addr', 'dbus.pid'):
            try: os.remove(f'{self.root}/{f}')
            except Exception: pass

def xdo(*a):
    return subprocess.run(['xdotool', *a], env={**os.environ, 'DISPLAY': DISPLAY}, capture_output=True, text=True).stdout

def windows(pattern='.', visible=True):
    a = ['search'] + (['--onlyvisible'] if visible else []) + ['--name', pattern]
    return xdo(*a).split()

def wname(w): return xdo('getwindowname', w).strip()
def wgeom(w):
    m = re.search(r'Geometry: (\d+)x(\d+)', xdo('getwindowgeometry', w)); return (int(m.group(1)), int(m.group(2))) if m else (0, 0)
def wmclose(w): subprocess.run([S + '/bin/wmclose', str(w)], env={**os.environ, 'DISPLAY': DISPLAY})

def pixel(w, x, y):
    out = subprocess.run(['import', '-window', str(w), '-crop', '1x1+%d+%d' % (x, y), 'txt:-'], env={**os.environ, 'DISPLAY': DISPLAY}, capture_output=True, text=True).stdout
    m = re.search(r'#([0-9A-Fa-f]{6})', out); return m.group(1) if m else None

def shot(path, w=None):
    subprocess.run(['import', '-window', str(w) if w else 'root', path], env={**os.environ, 'DISPLAY': DISPLAY}, capture_output=True)

def wait_for(fn, timeout=20, step=0.02):
    t0 = time.time()
    while time.time() - t0 < timeout:
        r = fn()
        if r: return r, time.time() - t0
        time.sleep(step)
    return None, time.time() - t0

def fmt(o):
    t = o['total']
    return '%-26s RSS %4.0f PSS %4.0f anon %4.0f swap %3.0f thr %3d stub %3.0f/%3.0f | %s' % (o.get('label', ''), t.get('rss', 0)/1024, t.get('pss', 0)/1024,
        t.get('anon', 0)/1024, t.get('swap', 0)/1024, t.get('threads', 0), o.get('runtime_rss', 0)/1024, o.get('runtime_pss', 0)/1024,
        ' '.join('%s:%d/%d' % (p['name'][:10], round(p['rss']/1024), round(p['pss']/1024)) for p in sorted(o['procs'], key=lambda p: -p['rss'])[:5]))
