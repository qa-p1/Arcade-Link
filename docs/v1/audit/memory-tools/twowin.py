"""twowin.py MODE: Tools and Look windows open at once. MODE = ext (separate unpacked copies) | hl (Look's identical libs hardlinked to Tools' inodes: shared page cache)."""
import sys, os, time, subprocess, filecmp
sys.path.insert(0, os.path.dirname(__file__))
from lab import *
mode = sys.argv[1]; EXT = '/var/tmp/arcade-audit-ext'
if mode == 'hl':
    src, dst = f'{EXT}/Tools/squashfs-root', f'{EXT}/LookHL'
    if not os.path.exists(dst):
        subprocess.run(['cp', '-a', '--reflink=auto', f'{EXT}/Look/squashfs-root', dst], check=True)
        n = 0; saved = 0
        for dp, dn, fn in os.walk(dst):
            for f in fn:
                p = os.path.join(dp, f); rel = os.path.relpath(p, dst); q = os.path.join(src, rel)
                if os.path.isfile(q) and not os.path.islink(p) and os.path.getsize(p) > 65536 and os.path.getsize(p) == os.path.getsize(q) and filecmp.cmp(p, q, shallow=False):
                    os.unlink(p); os.link(q, p); n += 1; saved += os.path.getsize(q)
        print(f'hardlinked {n} identical files ({saved/1e6:.0f} MB) from Tools into LookHL', flush=True)
    lookexe = f'{dst}/AppRun'
else:
    lookexe = f'{EXT}/Look/squashfs-root/AppRun'
toolsexe = f'{EXT}/Tools/squashfs-root/AppRun'
L = Lab(f'twowin-{mode}'); f = L.home + '/s.txt'; open(f, 'w').write('x\n' * 200)
L.start(toolsexe, ['--background']); L.start(lookexe, ['--background']); time.sleep(12); L.snap('both bg')
L.cli(toolsexe, ['--settings'], timeout=20); L.cli(lookexe, [f], timeout=20); time.sleep(8)
o = L.snap('both windows open')
for pr in o['procs']:
    pass
L.quit_all(); L.stop_dbus()
