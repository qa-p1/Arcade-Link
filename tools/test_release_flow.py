"""Tests for tools/release_flow.py (run: python3 -m unittest discover -s tools -p 'test_*.py')."""

import importlib.util
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


if __name__ == "__main__":
    unittest.main()
