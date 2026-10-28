"""Ten rounds of the same load against Box and Lens; RSS after each round.
Each call must also return the right output: Box's upper-cased text, and
Lens's OCR text with the words in the image. Exit codes alone are not enough.
python3 tools/e2e.py run -- python3 tools/leak.py"""
import concurrent.futures as cf, importlib.util, json, os, subprocess, sys, time
from pathlib import Path
LINK = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("e2e", LINK / "tools/e2e.py"); e2e = importlib.util.module_from_spec(spec); spec.loader.exec_module(e2e)
s = e2e.Session.__new__(e2e.Session); s.root, s.env, s.procs = Path(os.environ["ARCADE_E2E_ROOT"]), dict(os.environ), {}
s.env["WEBKIT_DISABLE_COMPOSITING_MODE"] = "1"
def rss(app):
    out = subprocess.run(["ps", "-o", "rss=", "-g", str(s.procs[app].pid)], capture_output=True, text=True).stdout
    return round(sum(int(x) for x in out.split()) / 1024, 1)
def text(result):
    return "\n".join(o.get("text", "") for o in result.get("outputs", []) if isinstance(o, dict)) if isinstance(result, dict) else ""
def box_case(i):
    code, result = s.invoke("box", "box:arcade.text.case", "--text", f"x {i}", "--option", "mode=upper")
    return "exit" if code != 0 else None if f"X {i}" in text(result) else "wrong"
def lens_ocr(_):
    code, result = s.invoke("lens", "lens.recognize", "--file", str(png), "--option", "ocrOnly=true")
    words = text(result)
    return "exit" if code != 0 else None if "Leak" in words and "4821" in words else "wrong"
png = s.root / "t.png"
subprocess.run(["magick", "-size", "900x200", "xc:white", "-fill", "black", "-pointsize", "56", "-gravity", "center", "-annotate", "0", "Leak 4821", str(png)], check=True)
s.start("arcade.box"); s.start("arcade.lens"); time.sleep(8)
out = {"box": [rss("arcade.box")], "lens": [rss("arcade.lens")], "fail": 0, "wrong": 0}
for r in range(10):
    for workers, call, n in ((16, box_case, 300), (6, lens_ocr, 60)):
        with cf.ThreadPoolExecutor(workers) as p:
            for outcome in p.map(call, range(n)):
                out["fail"] += outcome == "exit"
                out["wrong"] += outcome == "wrong"
    out["box"].append(rss("arcade.box")); out["lens"].append(rss("arcade.lens"))
for a in list(s.procs): s.kill(a)
print(json.dumps(out))
sys.exit(1 if out["fail"] or out["wrong"] else 0)
