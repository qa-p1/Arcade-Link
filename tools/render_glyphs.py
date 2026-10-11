#!/usr/bin/env python3
"""Render the family's existing 16-unit SVGs with librsvg; no profile writes."""
import argparse
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SIZES = (16, 22, 24, 32, 48)


def render(output):
    renderer = shutil.which("rsvg-convert")
    if not renderer:
        raise RuntimeError("rsvg-convert is required to regenerate the committed renders")
    for svg in sorted((ROOT / "assets/glyphs").glob("*.svg")):
        for size in SIZES:
            path = output / str(size) / (svg.stem + ".png")
            path.parent.mkdir(parents=True, exist_ok=True)
            subprocess.run([renderer, "-w", str(size), "-h", str(size), "-o", str(path), str(svg)], check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "assets/glyphs/png")
    args = parser.parse_args()
    render(args.output)
