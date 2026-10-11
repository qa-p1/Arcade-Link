import tempfile
import unittest
from pathlib import Path
import vendor_qt


class VendorTests(unittest.TestCase):
    def test_pin_includes_the_complete_module_and_detects_tampering(self):
        with tempfile.TemporaryDirectory(prefix="arcade-vendor-") as root:
            target = Path(root)
            pin = vendor_qt.bundle(target)
            self.assertIn("qt/ArcadeTrayHost.cpp", pin["files"])
            self.assertIn("qt/ArcadeInstallPlatform.cpp", pin["files"])
            self.assertIn("qt/install-strings.inc", pin["files"])
            self.assertIn("spec/vectors/shortcuts.json", pin["files"])
            self.assertEqual(vendor_qt.bundle(target, check=True), pin)
            (target / "qt/ArcadeLink.cpp").write_text("changed", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "differs from the pin"):
                vendor_qt.bundle(target, check=True)

    def test_foreign_files_are_not_overwritten(self):
        with tempfile.TemporaryDirectory(prefix="arcade-vendor-") as root:
            target = Path(root)
            (target / "qt").mkdir()
            path = target / "qt/ArcadeLink.cpp"
            path.write_text("foreign", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "not owned"):
                vendor_qt.bundle(target)
            self.assertEqual(path.read_text(encoding="utf-8"), "foreign")
