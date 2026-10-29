#!/usr/bin/env python3
"""Instantiate Inter's egui weights; requires fonttools==4.61.1.

egui 0.33 does not expose variable-font axes. `RichText.strong()` changes
colour only, so medium/semibold must be actual font instances.
Regenerate with: python3 design/fonts/make-egui.py
The original Inter font and OFL licence remain in this directory.
"""
import hashlib
import json
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

ROOT = Path(__file__).resolve().parent


def main():
    source = ROOT / "InterVariable.ttf"
    hashes = {source.name: hashlib.sha256(source.read_bytes()).hexdigest()}
    for weight, name in [(500, "InterMedium.ttf"), (600, "InterSemibold.ttf")]:
        font = TTFont(source, recalcTimestamp=False)
        axes = {axis.axisTag: axis.defaultValue for axis in font["fvar"].axes}
        axes["wght"] = weight
        instance = instantiateVariableFont(font, axes, inplace=True)
        instance.recalcTimestamp = False
        destination = ROOT / name
        instance.save(destination)
        hashes[name] = hashlib.sha256(destination.read_bytes()).hexdigest()
    (ROOT / "egui-weights.json").write_text(json.dumps(hashes, indent=2) + "\n")


if __name__ == "__main__":
    main()
