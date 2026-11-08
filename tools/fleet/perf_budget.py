#!/usr/bin/env python3
"""perf_budget.py: CI check of one app's performance budgets (ARCADE_APP.md RT-6, PERF-2) on Linux under Xvfb.

    perf_budget.py --app Arcade-Tools.AppImage --name tools [--budgets budgets.json --section xvfb] [--json out.json]
                   [--open-args ''] [--window 'Arcade Tools'] [--settle 20] [--close-wait 20] [--rounds 3]

In a private session (own HOME/XDG/D-Bus, as fleet_budget.py) it installs a copy of the app (`--install --silent
--no-launch`, from outside any git work tree), then measures:

  first_start_ms   launch with --open-args (default: none, the app's default page) to its window being mapped, on the
                   first launch after the install (unpacking included). Reported, not budgeted.
  cold_start_ms    the same on a later launch with nothing of the app running (median of --rounds - 1).
  background_*     idle CPU (% of one core) and wakeups (context switches per second, all threads of the whole
                   process tree, FUSE stub included) over 30 s of `--background`, after --settle seconds.
  open_*           the same over 60 s with the window open on the default page.
  closed_*         the same over 30 s after the window was closed and --close-wait seconds passed.
  show_ms          launch with --open-args while the app runs in the background with its window closed (and its
                   memory released) to the window being mapped (median of --rounds).
  disk_mb          everything the app put in the private profile: installed copy, unpacked runtime, data, caches.

Exits 1 when a budget in the "perf" block of the budgets file (per app, `<metric>` → limit) is exceeded, 2 when the
app cannot be driven. Needs Xvfb (or $DISPLAY), dbus-daemon, xdotool and gcc/libX11 (for wmclose).
"""
import argparse, json, os, shlex, shutil, signal, statistics, subprocess, sys, tempfile, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import fleet_budget as fb  # noqa: E402

TICK = os.sysconf('SC_CLK_TCK')


def counters(pid):
    """(cpu ticks, context switches) of one process, all threads."""
    try:
        stat = open(f'/proc/{pid}/stat').read()
        fields = stat[stat.rindex(')') + 2:].split()
        ticks = int(fields[11]) + int(fields[12])  # utime, stime (whole process)
        switches = 0
        for tid in os.listdir(f'/proc/{pid}/task'):
            status = fb.kv(f'/proc/{pid}/task/{tid}/status')
            switches += status.get('voluntary_ctxt_switches', 0) + status.get('nonvoluntary_ctxt_switches', 0)
        return ticks, switches
    except (OSError, ValueError, IndexError):
        return None


def over_budget(name, result, limits):
    """Every limit the result exceeds; a budgeted metric the run did not produce is a failure too."""
    return [f'{name}: {k} {result[k]} > budget {v}' if k in result else f'{name}: {k} was not measured'
            for k, v in limits.items() if not k.startswith('_') and (k not in result or result[k] > v)]


class Session:
    def __init__(self, app, name, open_args, window):
        self.name, self.open_args, self.window = name, open_args, window
        self.root = tempfile.mkdtemp(prefix='arcade-perf-')
        self.run = tempfile.mkdtemp(prefix='arp-'); os.chmod(self.run, 0o700)
        env = {k: v for k, v in os.environ.items() if not k.startswith('ARCADE_') and k not in (
            'APPIMAGE', 'APPDIR', 'LD_LIBRARY_PATH', 'WAYLAND_DISPLAY', 'DBUS_SESSION_BUS_ADDRESS')}
        self.xvfb = None
        if not env.get('DISPLAY'):
            env['DISPLAY'] = ':98'
            self.xvfb = subprocess.Popen(['Xvfb', ':98', '-screen', '0', '1920x1080x24', '-nolisten', 'tcp'],
                                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            time.sleep(1.5)
        for d in ('home', 'config', 'data', 'cache', 'state', 'download'):
            os.makedirs(f'{self.root}/{d}')
        env.update(HOME=f'{self.root}/home', XDG_CONFIG_HOME=f'{self.root}/config', XDG_DATA_HOME=f'{self.root}/data',
                   XDG_CACHE_HOME=f'{self.root}/cache', XDG_STATE_HOME=f'{self.root}/state', XDG_RUNTIME_DIR=self.run,
                   GDK_BACKEND='x11', ARCADE_NO_INSTALL_PROMPT='1')
        out = subprocess.run(['dbus-daemon', '--session', '--fork', '--print-address=1', '--print-pid=1'], env=env,
                             capture_output=True, text=True).stdout.split('\n')
        env['DBUS_SESSION_BUS_ADDRESS'] = out[0]; self.dbus = int(out[1])
        self.env, self.data, self.home = env, env['XDG_DATA_HOME'], env['HOME']
        # A file inside a git work tree counts as a dev build, which refuses to install: start from a "download".
        self.download = os.path.join(self.root, 'download', os.path.basename(app))
        shutil.copy2(app, self.download); os.chmod(self.download, 0o755)
        self.exe = self.download
        self.launched = []
        self.log = open(os.path.join(self.root, 'app.log'), 'wb')

    def install(self):
        r = subprocess.run([self.exe, '--install', '--silent', '--no-launch'], env=self.env, cwd=self.home,
                           capture_output=True, text=True, timeout=180)
        if r.returncode != 0:
            raise SystemExit(f'perf_budget: --install --silent exited {r.returncode}: {r.stderr[-2000:]}')
        installed = [os.path.join(d, f) for d, _, files in os.walk(self.root) for f in files
                     if f.endswith('.AppImage') and not d.startswith(os.path.dirname(self.download))]
        if len(installed) != 1:
            raise SystemExit(f'perf_budget: expected one installed AppImage, found {installed}')
        self.exe = installed[0]

    def start(self, *args):
        p = subprocess.Popen([self.exe, *args], env=self.env, cwd=self.home, stdout=self.log, stderr=subprocess.STDOUT,
                             start_new_session=True)
        self.launched.append(p.pid)
        return p

    def pids(self):
        """The app's live processes: what we launched and its descendants, its unpacked copy (U1), its FUSE stub."""
        own = fb.tree([p for p in self.launched if os.path.exists(f'/proc/{p}')] + fb.unpacked(self.name, self.home, self.data))
        stubs = [p for v in fb.find_stubs(self.home).values() for p in v]
        return sorted({p for p in own + stubs if os.path.exists(f'/proc/{p}')})

    def windows(self):
        r = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', self.window], env=self.env,
                           capture_output=True, text=True)
        return r.stdout.split()

    def time_to_window(self, timeout=60):
        t = time.monotonic()
        self.start(*self.open_args)
        while True:
            w = self.windows()
            if w:
                return (time.monotonic() - t) * 1000, w
            if time.monotonic() - t > timeout:
                raise SystemExit(f'perf_budget: no window matching {self.window!r} within {timeout} s\n'
                                 + open(self.log.name, errors='replace').read()[-3000:])
            time.sleep(0.01)

    def close(self, wids):
        for w in wids:
            fb.close_window(w, self.env)
        end = time.monotonic() + 15
        while self.windows() and time.monotonic() < end:
            time.sleep(0.1)

    def quit(self):
        subprocess.run([self.exe, '--quit'], env=self.env, cwd=self.home, capture_output=True, timeout=60)
        end = time.monotonic() + 20
        while time.monotonic() < end:
            if not [p for p in self.pids() if p not in [s for v in fb.find_stubs(self.home).values() for s in v]]:
                break
            time.sleep(0.2)
        self.kill(signal.SIGKILL)  # whatever --quit left behind (stubs) must not carry into the next round
        self.launched.clear()
        time.sleep(0.5)

    def idle(self, seconds):
        """CPU % of one core and wakeups per second of the whole tree over `seconds`."""
        start = {p: c for p in self.pids() if (c := counters(p))}
        time.sleep(seconds)
        ticks = switches = 0
        for p in self.pids():
            end = counters(p)
            if not end:
                continue
            before = start.get(p, (0, 0))
            ticks += end[0] - before[0]; switches += end[1] - before[1]
        return round(ticks / TICK / seconds * 100, 2), round(switches / seconds, 1)

    def disk_mb(self):
        total = 0
        for d, _, files in os.walk(self.root):
            if d.startswith(os.path.dirname(self.download)):
                continue
            for f in files:
                try:
                    st = os.lstat(os.path.join(d, f))
                    total += st.st_blocks * 512
                except OSError:
                    pass
        return round(total / 1048576, 1)

    def kill(self, sig):
        for p in self.pids():
            try:
                os.kill(p, sig)
            except OSError:
                pass

    def cleanup(self):
        self.kill(signal.SIGTERM); time.sleep(2); self.kill(signal.SIGKILL)
        try:
            os.kill(self.dbus, signal.SIGTERM)
        except OSError:
            pass
        if self.xvfb:
            self.xvfb.terminate()
        shutil.rmtree(self.root, ignore_errors=True); shutil.rmtree(self.run, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--app', required=True); ap.add_argument('--name', required=True)
    ap.add_argument('--budgets'); ap.add_argument('--section'); ap.add_argument('--json')
    ap.add_argument('--open-args', default='', help="arguments that open the window (default: none, the default page)")
    ap.add_argument('--window', help="xdotool --name regex of the window (default: 'Arcade <Name>')")
    ap.add_argument('--settle', type=int, default=20); ap.add_argument('--close-wait', type=int, default=20)
    ap.add_argument('--rounds', type=int, default=3)
    a = ap.parse_args()
    budgets = json.load(open(a.budgets)) if a.budgets else {}
    if a.section:
        budgets = budgets.get(a.section, {})
    limits = {k: v for k, v in budgets.get('perf', {}).get(a.name, {}).items() if not k.startswith('_')}
    s = Session(os.path.abspath(a.app), a.name, shlex.split(a.open_args), a.window or f'Arcade {a.name.capitalize()}')
    r = {}
    try:
        s.install()
        starts = []
        for n in range(a.rounds):
            ms, wids = s.time_to_window()
            starts.append(round(ms))
            time.sleep(3)
            s.close(wids)
            s.quit()
        r['first_start_ms'] = starts[0]
        r['cold_start_ms'] = round(statistics.median(starts[1:] or starts))
        s.start('--background'); time.sleep(a.settle)
        r['background_cpu_pct'], r['background_wakeups'] = s.idle(30)
        ms, wids = s.time_to_window(); time.sleep(5)
        r['open_cpu_pct'], r['open_wakeups'] = s.idle(60)
        shows = []
        for n in range(a.rounds):
            s.close(wids); time.sleep(a.close_wait)
            if n == 0:
                r['closed_cpu_pct'], r['closed_wakeups'] = s.idle(30)
            ms, wids = s.time_to_window(); shows.append(round(ms)); time.sleep(3)
        s.close(wids)
        r['show_ms'] = round(statistics.median(shows))
        r['show_ms_rounds'] = shows; r['cold_start_ms_rounds'] = starts[1:]
        s.quit()
        r['disk_mb'] = s.disk_mb()
    finally:
        s.cleanup()
    failures = over_budget(a.name, r, limits)
    print(f'--- {a.name} performance')
    for k, v in r.items():
        print(f'  {k:<22} {v}' + (f'   (budget {limits[k]})' if k in limits else ''))
    if a.json:
        json.dump({'perf': r, 'budgets': limits}, open(a.json, 'w'), indent=1)
    if not limits:
        print(f'\nno perf budgets for {a.name}'); return 0
    if failures:
        print('\nBUDGET FAILURES:'); [print('  ' + f) for f in failures]; return 1
    print('\nall performance budgets met'); return 0


if __name__ == '__main__':
    sys.exit(main())
