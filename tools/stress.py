"""Stress test for the Arcade apps, run inside the isolated session:

    python3 tools/e2e.py run -- python3 tools/stress.py

Hammers Box and Lens over the Link concurrently, runs the replaced Box
engines (built-in PDF conversion, images to PDF, searchable PDF, system
speech) repeatedly and in parallel, kills Box mid-burst, and reports
failures, latency and memory before/after. Prints one JSON summary."""

import concurrent.futures as cf
import importlib.util
import json
import os
import signal
import shutil
import subprocess
import time
from pathlib import Path

LINK = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("e2e", LINK / "tools/e2e.py")
e2e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e2e)

root = Path(os.environ["ARCADE_E2E_ROOT"])
s = e2e.Session.__new__(e2e.Session)
s.root, s.env, s.procs = root, dict(os.environ), {}
s.env["WEBKIT_DISABLE_COMPOSITING_MODE"] = "1"
summary = {}


def group_rss_mib(app):
    p = s.procs.get(app)
    if not p or p.poll() is not None:
        return None
    out = subprocess.run(["ps", "-o", "rss=", "-g", str(p.pid)], capture_output=True, text=True).stdout
    return round(sum(int(x) for x in out.split()) / 1024, 1)


def invoke(app, action, *args, timeout=180):
    t = time.monotonic()
    code, result = s.invoke(app, action, *args, timeout=timeout)
    return code, result, time.monotonic() - t


def burst(name, calls, workers):
    lat, failures = [], []
    with cf.ThreadPoolExecutor(workers) as pool:
        for code, result, dt, check in pool.map(lambda c: (*invoke(*c[0]), c[1]), calls):
            lat.append(dt)
            ok = code == 0 and (check is None or check(result))
            if not ok:
                failures.append(str(result)[:200])
    lat.sort()
    summary[name] = {
        "calls": len(calls), "failures": len(failures), "p50_s": round(lat[len(lat) // 2], 3),
        "p95_s": round(lat[int(len(lat) * 0.95) - 1], 3), "max_s": round(lat[-1], 3),
        "sample_failures": failures[:3],
    }


def outputs_text(result):
    if not isinstance(result, dict):
        return ""
    return "\n".join(o.get("text", "") for o in result.get("outputs", []) if isinstance(o, dict))


def files_exist(result):
    return isinstance(result, dict) and all(
        Path(o["path"]).is_file() and Path(o["path"]).stat().st_size > 0
        for o in result.get("outputs", []) if isinstance(o, dict) and o.get("path"))


# ---- fixtures
text_png = root / "stress-text.png"
subprocess.run(["magick", "-size", "900x200", "xc:white", "-fill", "black", "-font", "DejaVu-Sans",
                "-pointsize", "56", "-gravity", "center", "-annotate", "0", "Stress test 4821",
                str(text_png)], check=True)
scan_pdf = root / "stress-scan.pdf"
subprocess.run(["magick", str(text_png), "-density", "150", str(scan_pdf)], check=True)
odt = root / "stress-manual.odt"
shutil.copy("/usr/share/gutenprint/doc/gutenprint-users-manual.odt", odt)

# ---- Box resident: text tools under concurrency
t0 = time.monotonic()
s.start("arcade.box")
summary["box_start_to_serving_s"] = round(time.monotonic() - t0, 2)


def engines_ready():
    rows = json.loads(s.cli("describe", "box", "--json", check=True).stdout)
    return all(any(a["id"] == f"box:arcade.{t}" and a.get("available", True) for a in rows)
               for t in ("pdf.ocr", "audio.text-to-speech"))


while not engines_ready():
    assert time.monotonic() - t0 < 90, "Box never finished checking its engines"
    time.sleep(0.25)
summary["box_engine_check_s"] = round(time.monotonic() - t0, 2)
time.sleep(2)
summary["box_rss_start_mib"] = group_rss_mib("arcade.box")
text_calls = []
for i in range(300):
    action, opt = [("box:arcade.text.case", "mode=upper"), ("box:arcade.developer.hash", "algorithm=sha256"),
                   ("box:arcade.text.statistics", None)][i % 3]
    args = ["--text", f"hello stress {i}"] + (["--option", opt] if opt else [])
    check = (lambda r, i=i: f"HELLO STRESS {i}" in outputs_text(r)) if action.endswith("case") else None
    text_calls.append(((("box", action, *args)), check))
burst("box_text_tools_300x16", text_calls, 16)
# Leak check: the same burst twice more, memory after each.
summary["box_rss_after_text_bursts_mib"] = [group_rss_mib("arcade.box")]
for round_ in (2, 3):
    burst(f"box_text_tools_300x16_round{round_}", text_calls, 16)
    summary["box_rss_after_text_bursts_mib"].append(group_rss_mib("arcade.box"))

# ---- Box replaced engines, in parallel
heavy = []
for i in range(4):
    heavy.append((("box", "box:arcade.pdf.convert", "--file", str(odt)), files_exist))
    heavy.append((("box", "box:arcade.pdf.ocr", "--file", str(scan_pdf)), files_exist))
    heavy.append((("box", "box:arcade.pdf.images-to-pdf", "--file", str(text_png), "--file", str(text_png)), files_exist))
    heavy.append((("box", "box:arcade.audio.text-to-speech", "--text", f"Stress run number {i}"), files_exist))
burst("box_replaced_engines_16x4", heavy, 4)
summary["box_rss_after_load_mib"] = group_rss_mib("arcade.box")
burst("box_replaced_engines_round2", heavy, 4)
summary["box_rss_after_load_round2_mib"] = group_rss_mib("arcade.box")
time.sleep(10)
summary["box_rss_idle_10s_after_mib"] = group_rss_mib("arcade.box")

# ---- Lens resident: OCR under concurrency (Tesseract)
s.start("arcade.lens")
time.sleep(1)
summary["lens_rss_start_mib"] = group_rss_mib("arcade.lens")
lens_calls = [((("lens", "lens.recognize", "--file", str(text_png), "--option", "ocrOnly=true")),
               lambda r: "4821" in outputs_text(r)) for _ in range(60)]
summary["lens_rss_after_ocr_bursts_mib"] = []
for round_ in (1, 2, 3):
    burst(f"lens_ocr_60x6_round{round_}", lens_calls, 6)
    summary["lens_rss_after_ocr_bursts_mib"].append(group_rss_mib("arcade.lens"))

# ---- Box OCR through Lens
box_lens = [((("box", "box:arcade.image.ocr", "--file", str(text_png), "--option", "provider=lens")),
             lambda r: "4821" in outputs_text(r) or "4821" in json.dumps(r)) for _ in range(10)]
burst("box_ocr_via_lens_10x3", box_lens, 3)

# ---- Kill Box mid-burst, then restart: errors must be clean, then recover
def killer():
    time.sleep(0.5)
    s.kill("arcade.box", signal.SIGKILL)

# Long enough that the kill lands mid-burst: calls after it must fail
# cleanly and quickly (no hang), never succeed with a wrong result.
churn = [((("box", "box:arcade.text.case", "--text", f"churn {i}", "--option", "mode=upper")),
          lambda r, i=i: f"CHURN {i}" in outputs_text(r)) for i in range(1500)]
with cf.ThreadPoolExecutor(1) as k:
    k.submit(killer)
    burst("box_killed_mid_burst_1500x8", churn, 8)
leftover = s.endpoint("arcade.box").exists()
s.start("arcade.box")
time.sleep(2)
burst("box_after_restart_50x8", [((("box", "box:arcade.text.case", "--text", f"again {i}", "--option", "mode=upper")),
                                  lambda r, i=i: f"AGAIN {i}" in outputs_text(r)) for i in range(50)], 8)
summary["stale_endpoint_after_kill"] = leftover

# ---- Clean quits
for app in ("box", "lens"):
    t = time.monotonic()
    q = s.cli("quit", app, timeout=30)
    p = s.procs[f"arcade.{app}"]
    try:
        p.wait(timeout=15)
        summary[f"{app}_quit"] = {"exit": p.returncode, "seconds": round(time.monotonic() - t, 2)}
    except subprocess.TimeoutExpired:
        summary[f"{app}_quit"] = {"exit": None, "note": q.stdout + q.stderr}
for app in list(s.procs):
    s.kill(app, signal.SIGTERM)
print(json.dumps(summary, indent=1), flush=True)
