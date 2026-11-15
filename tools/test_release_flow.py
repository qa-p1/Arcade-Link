"""Tests for tools/release_flow.py (run: python3 -m unittest discover -s tools -p 'test_*.py')."""

import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("release_flow", Path(__file__).parent / "release_flow.py")
rf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rf)


def release(*kinds):
    return {"assets": [{"os": "linux", "kind": "appimage"}] + [{"os": "windows", "kind": k} for k in kinds]}


class WindowsInstallerOf(unittest.TestCase):
    def test_promotion_keeps_the_rcs_installer_kind(self):
        self.assertEqual(rf.windows_installer_of(release("inno")), "inno")
        self.assertEqual(rf.windows_installer_of(release("nsis")), "nsis")

    def test_a_portable_build_beside_the_installer_does_not_count(self):
        self.assertEqual(rf.windows_installer_of(release("portable", "inno")), "inno")

    def test_without_a_windows_installer_the_default_stays_nsis(self):
        self.assertEqual(rf.windows_installer_of(release()), "nsis")
        self.assertEqual(rf.windows_installer_of(release("msi")), "nsis")

    def test_mixed_installer_kinds_are_refused(self):
        with self.assertRaises(SystemExit):
            rf.windows_installer_of(release("nsis", "inno"))


class AssetNames(unittest.TestCase):
    def folder(self, *names):
        d = Path(self.enterContext(tempfile.TemporaryDirectory()))
        for name in names:
            (d / name).write_text(name)
        return d

    def test_spaces_become_hyphens_and_metadata_is_left_alone(self):
        d = self.folder("Arcade Box_1.0.0_amd64.AppImage", "Arcade Box_1.0.0_x64-setup.exe", "SHA256SUMS.txt")
        rf.asset_names(d)
        self.assertEqual(sorted(p.name for p in d.iterdir()),
                         ["Arcade-Box_1.0.0_amd64.AppImage", "Arcade-Box_1.0.0_x64-setup.exe", "SHA256SUMS.txt"])
        self.assertEqual((d / "Arcade-Box_1.0.0_amd64.AppImage").read_text(), "Arcade Box_1.0.0_amd64.AppImage")

    def test_names_github_keeps_are_unchanged(self):
        d = self.folder("arcade-look_1.0.0+build_x86_64.AppImage")
        rf.asset_names(d)
        self.assertTrue((d / "arcade-look_1.0.0+build_x86_64.AppImage").is_file())

    def test_other_characters_and_collisions_are_refused(self):
        with self.assertRaises(SystemExit):
            rf.asset_names(self.folder("Arcade (Box).AppImage"))
        with self.assertRaises(SystemExit):
            rf.asset_names(self.folder("A B.exe", "A-B.exe"))


if __name__ == "__main__":
    unittest.main()
