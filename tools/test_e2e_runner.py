"""Regression checks for sandbox and stale-build guards; no apps are launched."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import e2e


class RunnerGuards(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='arcade-runner-test-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
        self.source = self.root / 'main.rs'
        self.source.write_text('fn main() {}\n')
        self.binary = self.root / 'app'
        self.binary.write_text('#!/bin/sh\nexit 0\n')
        self.binary.chmod(0o755)
        os.utime(self.source, ns=(1_000_000_000, 1_000_000_000))
        os.utime(self.binary, ns=(2_000_000_000, 2_000_000_000))
        self.spec = {'dir': self.root, 'bin': 'app', 'build': ['cargo', 'build']}

    def test_profile_and_desktop_environment_are_replaced(self):
        with patch.dict(os.environ, {'HOME': '/real-home', 'WAYLAND_DISPLAY': 'real-wayland',
                                    'HYPRLAND_INSTANCE_SIGNATURE': 'real-hypr', 'DISPLAY': ':0',
                                    'DBUS_SESSION_BUS_ADDRESS': 'real-bus', 'APPIMAGE': '/real-app'}):
            env = e2e.isolated_env(self.root)
        self.assertEqual(env['HOME'], str(self.root / 'home'))
        for key in ('HOME', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_RUNTIME_DIR', 'ARCADE_HOME'):
            self.assertTrue(Path(env[key]).is_relative_to(self.root))
        for key in ('WAYLAND_DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'DISPLAY', 'DBUS_SESSION_BUS_ADDRESS', 'APPIMAGE'):
            self.assertNotIn(key, env)

    def test_session_rejects_a_non_sandbox_root(self):
        with patch.dict(os.environ, {'ARCADE_E2E_INNER': '1', 'ARCADE_E2E_ROOT': str(self.root)}):
            with self.assertRaisesRegex(AssertionError, 'isolated session'):
                e2e.Session(self.root.parent, headless=True)

    def test_missing_binary_reports_the_build_instruction(self):
        self.binary.unlink()
        with self.assertRaisesRegex(AssertionError, 'stale or missing build.*cargo build'):
            e2e.require_current_build(self.spec)

    def test_untracked_source_can_make_a_build_stale(self):
        self.source.write_text('fn main() { println!("changed"); }\n')
        with self.assertRaisesRegex(AssertionError, 'stale or missing build.*newer source'):
            e2e.require_current_build(self.spec)

    def test_documentation_does_not_make_a_build_stale(self):
        (self.root / 'README.md').write_text('New documentation\n')
        self.assertEqual(e2e.require_current_build(self.spec), self.binary)

    def test_unknown_group_does_not_start_a_session(self):
        with patch.object(e2e, 'load_check_modules'), patch.object(e2e, 'Session') as session:
            self.assertEqual(e2e.run_checks(self.root, 'misspelled'), 2)
            session.assert_not_called()


if __name__ == '__main__':
    unittest.main()
