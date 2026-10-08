"""Ten rounds of the same load against Box and Lens; RSS after each round.
python3 tools/e2e.py run -- python3 tools/leak.py"""
import concurrent.futures as cf, importlib.util, json, os, subprocess, time
from pathlib import Path
LINK = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("e2e", LINK / "tools/e2e.py"); e2e = importlib.util.module_from_spec(spec); spec.loader.exec_module(e2e)
s = e2e.Session.__new__(e2e.Session); s.root, s.env, s.procs = Path(os.environ["ARCADE_E2E_ROOT"]), dict(os.environ), {}
s.env["WEBKIT_DISABLE_COMPOSITING_MODE"] = "1"
def rss(app):
    out = subprocess.run(["ps", "-o", "rss=", "-g", str(s.procs[app].pid)], capture_output=True, text=True).stdout
    return round(sum(int(x) for x in out.split()) / 1024, 1)
png = s.root / "t.png"
subprocess.run(["magick", "-size", "900x200", "xc:white", "-fill", "black", "-pointsize", "56", "-gravity", "center", "-annotate", "0", "Leak 4821", str(png)], check=True)
s.start("arcade.box"); s.start("arcade.lens"); time.sleep(8)
out = {"box": [rss("arcade.box")], "lens": [rss("arcade.lens")], "fail": 0}
for r in range(10):
    with cf.ThreadPoolExecutor(16) as p:
        out["fail"] += sum(c != 0 for c, _ in p.map(lambda i: s.invoke("box", "box:arcade.text.case", "--text", f"x {i}", "--option", "mode=upper"), range(300)))
    with cf.ThreadPoolExecutor(6) as p:
        out["fail"] += sum(c != 0 for c, _ in p.map(lambda i: s.invoke("lens", "lens.recognize", "--file", str(png), "--option", "ocrOnly=true"), range(60)))
    out["box"].append(rss("arcade.box")); out["lens"].append(rss("arcade.lens"))
for a in list(s.procs): s.kill(a)
print(json.dumps(out))
