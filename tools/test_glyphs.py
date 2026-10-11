from pathlib import Path
import struct
import unittest
import xml.etree.ElementTree as ET
import e2e


class GlyphTests(unittest.TestCase):
    def test_every_family_id_has_a_vector_and_each_tray_render(self):
        root = e2e.LINK / "assets/glyphs"
        for app in e2e.APP_IDS + ["arcade.tools"]:
            self.assertEqual(ET.parse(root / (app + ".svg")).getroot().get("viewBox"), "0 0 16 16")
            for size in (16, 22, 24, 32, 48):
                png = (root / "png" / str(size) / (app + ".png")).read_bytes()
                self.assertEqual(png[:8], b"\x89PNG\r\n\x1a\n")
                self.assertEqual(struct.unpack(">II", png[16:24]), (size, size))
