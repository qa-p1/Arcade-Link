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

    def test_fresh_runner_does_not_hide_a_stale_flutter_component(self):
        (self.root / 'main.dart').write_text('void main() {}\n')
        library = self.root / 'libapp.so'
        library.write_bytes(b'old library')
        os.utime(library, ns=(1_000_000_000, 1_000_000_000))
        # As for Clipboard, Dart sources belong to the component, not the runner.
        spec = {**self.spec, 'source_suffixes': {'.rs'}, 'components': [{'bin': 'libapp.so', 'source_suffixes': {'.dart'}}]}
        with self.assertRaisesRegex(AssertionError, 'stale or missing build.*stale component'):
            e2e.require_current_build(spec)

    def test_documentation_does_not_make_a_build_stale(self):
        (self.root / 'README.md').write_text('New documentation\n')
        self.assertEqual(e2e.require_current_build(self.spec), self.binary)

    def test_unknown_group_does_not_start_a_session(self):
        with patch.object(e2e, 'load_check_modules'), patch.object(e2e, 'Session') as session:
            self.assertEqual(e2e.run_checks(self.root, 'misspelled'), 2)
            session.assert_not_called()


class RootCleanup(unittest.TestCase):
    """The isolated root is removed without failing a finished run."""

    def test_mounts_inside_the_root_are_found_deepest_first(self):
        root = Path('/tmp/arcade-e2e-x')
        info = '\n'.join([
            '22 1 0:21 / / rw - ext4 /dev/sda rw',
            '90 22 0:50 / /tmp/arcade-e2e-x/runtime/doc rw - fuse.portal portal rw',
            '91 90 0:51 / /tmp/arcade-e2e-x/runtime/doc/by\\040app rw - fuse fuse rw',
            '92 22 0:52 / /tmp/arcade-e2e-xy/runtime/doc rw - fuse.portal portal rw',
        ])
        self.assertEqual(e2e.mounts_under(root, info),
                         ['/tmp/arcade-e2e-x/runtime/doc/by app', '/tmp/arcade-e2e-x/runtime/doc'])

    def test_mounts_are_detached_before_removal_and_errors_never_raise(self):
        calls = []
        with tempfile.TemporaryDirectory(prefix='arcade-e2e-cleanup-') as d:
            root = Path(d)
            with patch.object(e2e, 'mounts_under', return_value=[str(root / 'runtime/doc')]), \
                 patch.object(e2e.shutil, 'which', return_value='/usr/bin/fusermount3'), \
                 patch.object(e2e.subprocess, 'run', side_effect=lambda args, **_: calls.append(('umount', args))), \
                 patch.object(e2e.shutil, 'rmtree', side_effect=ConnectionAbortedError(103, 'Software caused connection abort')):
                e2e.remove_root(root)
        self.assertEqual(calls, [('umount', ['/usr/bin/fusermount3', '-u', '-z', str(root / 'runtime/doc')])])


if __name__ == '__main__':
    unittest.main()
