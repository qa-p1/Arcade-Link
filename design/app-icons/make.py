#!/usr/bin/env python3
"""Render every app's launcher icon set from design/glyphs + design/tokens.json.

    python3 design/app-icons/make.py            # all eight apps
    python3 design/app-icons/make.py box tools  # some apps

Needs librsvg's `rsvg-convert`; everything else is the Python standard library.
Writes design/app-icons/<app>/:

- icon.svg (from design/generate.py): the 1024 master, radius 22.5 %;
- png/<n>.png for 16-1024, and <app>.ico (16-256);
- macos/: icon.icns plus AppIcon PNGs, drawn inside Apple's 10 % safe area;
- ios/: full-bleed opaque squares (iOS applies its own mask);
- android/mipmap-*/: adaptive foreground (glyph) and background (gradient)
  plus the legacy ic_launcher.png;
- tauri/: the file names `tauri icon` produces (32x32.png, 128x128@2x.png,
  icon.icns, icon.ico, Square*Logo.png, StoreLogo.png, icon.png).

Apps copy their folder with their scripts/sync-design step.
"""
from __future__ import annotations

import json
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
DESIGN = HERE.parent
sys.path.insert(0, str(DESIGN))
import generate  # noqa: E402  (shares the master-icon geometry)

PNG_SIZES = (16, 24, 32, 48, 64, 128, 256, 512, 1024)
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
ICNS_TYPES = (("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128), ("ic08", 256), ("ic09", 512), ("ic10", 1024),
              ("ic11", 32), ("ic12", 64), ("ic13", 256), ("ic14", 512))
MACOS_SIZES = (16, 32, 64, 128, 256, 512, 1024)
IOS_SIZES = (20, 29, 40, 58, 60, 76, 80, 87, 120, 152, 167, 180, 1024)
ANDROID = {"mdpi": 1, "hdpi": 1.5, "xhdpi": 2, "xxhdpi": 3, "xxxhdpi": 4}
TAURI = {"32x32.png": 32, "64x64.png": 64, "128x128.png": 128, "128x128@2x.png": 256, "icon.png": 512,
         "Square30x30Logo.png": 30, "Square44x44Logo.png": 44, "Square71x71Logo.png": 71, "Square89x89Logo.png": 89,
         "Square107x107Logo.png": 107, "Square142x142Logo.png": 142, "Square150x150Logo.png": 150,
         "Square284x284Logo.png": 284, "Square310x310Logo.png": 310, "StoreLogo.png": 50}


def renderer() -> str:
    found = shutil.which("rsvg-convert")
    if not found:
        raise SystemExit("rsvg-convert (librsvg) is required: apt install librsvg2-bin / brew install librsvg")
    return found


def render(svg: str, size: int, out: Path, work: Path) -> bytes:
    source = work / "in.svg"
    source.write_text(svg, encoding="utf-8")
    out.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([renderer(), "-w", str(size), "-h", str(size), "-o", str(out), str(source)], check=True)
    return out.read_bytes()


def inner(glyph: str) -> str:
    import re
    return re.search(r"<svg[^>]*>(.*)</svg>", glyph, re.S).group(1)


def gradient(accent: str) -> str:
    top = generate.mix(accent, "#ffffff", 0.82)
    return f'<linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{accent}"/></linearGradient>'


def glyph_group(glyph: str, box: float, origin: float, colour: str = "#ffffff") -> str:
    scale = box / 16
    stroke = generate.fmt(1.5 * 16 / 24)
    return (f'<g transform="translate({generate.fmt(origin)} {generate.fmt(origin)}) scale({generate.fmt(scale)})" fill="none" stroke="{colour}" '
            f'stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round">{inner(glyph)}</g>')


def macos_svg(glyph: str, accent: str) -> str:
    # Apple's grid: the body is inset 10 % on every side, radius 22.5 % of it.
    body, inset = 1024 * 0.8, 1024 * 0.1
    g = 1024 * 0.56 * 0.8
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><defs>{gradient(accent)}</defs>'
            f'<rect x="{generate.fmt(inset)}" y="{generate.fmt(inset)}" width="{generate.fmt(body)}" height="{generate.fmt(body)}" rx="{generate.fmt(body * 0.225)}" fill="url(#g)"/>'
            f'{glyph_group(glyph, g, (1024 - g) / 2)}</svg>')


def full_bleed_svg(glyph: str, accent: str) -> str:
    g = 1024 * 0.56
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><defs>{gradient(accent)}</defs>'
            f'<rect width="1024" height="1024" fill="url(#g)"/>{glyph_group(glyph, g, (1024 - g) / 2)}</svg>')


def android_foreground_svg(glyph: str) -> str:
    # 108 dp canvas, 66 dp safe zone: the glyph keeps 56 % of the 72 dp visible icon.
    g = 108 * 72 / 108 * 0.56 / 108 * 1024
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">{glyph_group(glyph, g, (1024 - g) / 2)}</svg>'


def android_background_svg(accent: str) -> str:
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><defs>{gradient(accent)}</defs><rect width="1024" height="1024" fill="url(#g)"/></svg>'


def write_ico(path: Path, images: dict[int, bytes]) -> None:
    sizes = sorted(images)
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries, blobs = b"", b""
    for size in sizes:
        data = images[size]
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset + len(blobs))
        blobs += data
    path.write_bytes(header + entries + blobs)


def write_icns(path: Path, images: dict[int, bytes]) -> None:
    body = b""
    for kind, size in ICNS_TYPES:
        data = images[size]
        body += kind.encode("ascii") + struct.pack(">I", len(data) + 8) + data
    path.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def make(app: str, tokens: dict, work: Path) -> None:
    app_id = f"arcade.{app}"
    accent = tokens["accent"][app_id]["light"]
    glyph = (DESIGN / "glyphs" / f"{app_id}.svg").read_text(encoding="utf-8")
    out = HERE / app
    master = generate.app_icon_svg(glyph, accent)
    (out / "icon.svg").write_text(master, encoding="utf-8")
    pngs = {size: render(master, size, out / "png" / f"{size}.png", work) for size in PNG_SIZES}
    write_ico(out / f"{app}.ico", {s: pngs[s] for s in ICO_SIZES})
    mac = macos_svg(glyph, accent)
    mac_pngs = {size: render(mac, size, out / "macos" / f"icon_{size}.png", work) for size in MACOS_SIZES}
    write_icns(out / "macos" / "icon.icns", mac_pngs)
    bleed = full_bleed_svg(glyph, accent)
    for size in IOS_SIZES:
        render(bleed, size, out / "ios" / f"AppIcon-{size}.png", work)
    fg, bg = android_foreground_svg(glyph), android_background_svg(accent)
    for density, factor in ANDROID.items():
        folder = out / "android" / f"mipmap-{density}"
        render(fg, round(108 * factor), folder / "ic_launcher_foreground.png", work)
        render(bg, round(108 * factor), folder / "ic_launcher_background.png", work)
        render(master, round(48 * factor), folder / "ic_launcher.png", work)
    tauri = out / "tauri"
    for name, size in TAURI.items():
        render(master, size, tauri / name, work)
    shutil.copyfile(out / f"{app}.ico", tauri / "icon.ico")
    shutil.copyfile(out / "macos" / "icon.icns", tauri / "icon.icns")
    print(f"{app}: {sum(1 for p in out.rglob('*') if p.is_file())} files")


def main(argv: list[str]) -> int:
    tokens = json.loads((DESIGN / "tokens.json").read_text(encoding="utf-8"))
    apps = argv or [app.removeprefix("arcade.") for app in tokens["accent"]]
    with tempfile.TemporaryDirectory() as tmp:
        for app in apps:
            make(app, tokens, Path(tmp))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
