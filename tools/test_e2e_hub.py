"""Fleet coverage and honest skip/failure reporting; no real apps are launched."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import e2e
import e2e_artifacts


class HubHarnessTests(unittest.TestCase):
    def test_all_eight_apps_have_install_docs_adoption_and_own_tray_checks(self):
        with patch.object(e2e, 'CHECKS', {}):
            e2e.load_check_modules(real_apps=True)
            for group, suffix in (
                ('install', '_appimage_install_update_repair_uninstall_reexec'),
                ('install', '_development_install_suppression'),
                ('shortcuts', '_shortcut_document'),
                ('trayhost', '_standalone_status_notifier_item'),
                ('tools-hub', '_manual_appimage_adoption'),
            ):
                apps = {f.apps[0] for f in e2e.CHECKS[group] if f.__name__.endswith(suffix)}
                self.assertEqual(apps, set(e2e.FLEET), (group, suffix))
            for suffix in ('_bundle_install', '_update_rollback_incompatible_hold'):
                apps = {f.apps[0] for f in e2e.CHECKS['tools-hub'] if f.__name__.endswith(suffix)}
                self.assertEqual(apps, set(e2e.APP_IDS))

    def test_skips_are_not_passes_and_are_strict_unless_allowed(self):
        def unavailable(_session):
            raise e2e.SkipCheck('arcade.find: no successful CI AppImage available')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.object(e2e, 'CHECKS', {'install': [unavailable]}), patch.object(e2e, 'load_check_modules'), patch.object(e2e, 'Session'), patch.dict(os.environ, ARCADE_E2E_REPORT_DIR=directory):
                self.assertEqual(e2e.run_checks(root, 'install'), 1)
                self.assertEqual(e2e.run_checks(root, 'install', allow_skips=True), 0)
            report = json.loads((root / 'install.json').read_text())
            self.assertEqual((report['passed'], report['failed'], report['skipped']), (0, 0, 1))
            self.assertFalse(report['checks'][0]['passed'])
            self.assertEqual(report['checks'][0]['status'], 'skipped')
            self.assertIn('no successful CI AppImage', report['checks'][0]['detail'])

    def test_a_failure_cannot_turn_into_a_skip_by_its_message(self):
        def broken(_session):
            raise AssertionError('stale or missing build: checksum mismatch')
        with patch.object(e2e, 'CHECKS', {'install': [broken]}), patch.object(e2e, 'load_check_modules'), patch.object(e2e, 'Session'):
            self.assertEqual(e2e.run_checks(Path('.'), 'install', allow_skips=True), 1)

    def _run_one(self, function, directory):
        with patch.object(e2e, 'CHECKS', {'tools-hub': [function]}), patch.object(e2e, 'load_check_modules'), patch.object(e2e, 'Session'), patch.dict(os.environ, ARCADE_E2E_REPORT_DIR=directory):
            code = e2e.run_checks(Path(directory), 'tools-hub')
        return code, json.loads((Path(directory) / 'tools-hub.json').read_text())

    def test_known_product_bug_excuses_only_its_documented_symptom(self):
        def rejected(_session):
            raise AssertionError({'notIncluded': [{'reason': 'Tools rejected the export.'}]})
        def other(_session):
            raise AssertionError('settings differ after restore')
        def fixed(_session):
            return 'round-tripped'
        for function in (rejected, other, fixed):
            function.known_bug, function.known_bug_match = 'documented defect', 'Tools rejected the export.'
        with tempfile.TemporaryDirectory() as directory:
            code, report = self._run_one(rejected, directory)
            self.assertEqual((code, report['checks'][0]['status'], report['expectedFailures']), (0, 'expected_failure', 1))
            self.assertEqual(report['checks'][0]['knownBug'], 'documented defect')
            code, report = self._run_one(other, directory)
            self.assertEqual((code, report['checks'][0]['status']), (1, 'fail'))
            code, report = self._run_one(fixed, directory)
            self.assertEqual((code, report['checks'][0]['status'], report['passed']), (1, 'unexpected_pass', 0))

    def test_backup_comparison_reads_non_json_exports(self):
        with patch.object(e2e, 'CHECKS', {}):
            e2e.load_check_modules(real_apps=True)
            snapshot = e2e.CHECKS['tools-hub'][0].__globals__['export_snapshot']
            bugs = e2e.CHECKS['tools-hub'][0].__globals__['BACKUP_BUGS']
            flagged = {f.__name__ for f in e2e.CHECKS['tools-hub'] if getattr(f, 'known_bug', None)}
        self.assertEqual(flagged, {a.replace('.', '_') + '_portable_settings_restore' for a in bugs}
                         | {'portable_backup_restores_real_app_settings_in_a_second_arcade_home'})
        def shelf(created, payload=b'owned'):
            manifest = json.dumps({'format': 'arcade-shelf-portable', 'createdAt': created, 'items': []}).encode()
            return b'ARCADE-SHELF-PORTABLE\n' + len(manifest).to_bytes(8, 'big') + manifest + payload
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ('a', 'b', 'c', 'd', 'e')]
            for path, data in zip(paths, (shelf('t1'), shelf('t2'), shelf('t1', b'other'), b'{"x": 1}', b'\x00raw')):
                path.write_bytes(data)
            self.assertEqual(snapshot(paths[0]), snapshot(paths[1]))
            self.assertNotEqual(snapshot(paths[0]), snapshot(paths[2]))
            self.assertEqual(snapshot(paths[3]), {'x': 1})
            self.assertEqual(snapshot(paths[4])['size'], 4)

    def test_coverage_table_never_counts_a_skip_as_a_pass(self):
        import e2e_coverage
        reports = [{'checks': [
            {'group': 'install', 'apps': ['arcade.find'], 'status': 'pass'},
            {'group': 'install', 'apps': ['arcade.shelf'], 'status': 'skipped'},
            {'group': 'tools-hub', 'apps': ['arcade.lens', 'arcade.tools'], 'status': 'expected_failure'},
        ]}]
        _, cells = e2e_coverage.table(reports)
        self.assertEqual(cells['install']['arcade.find']['P'], 1)
        self.assertEqual((cells['install']['arcade.shelf']['P'], cells['install']['arcade.shelf']['S']), (0, 1))
        self.assertEqual(cells['tools-hub']['arcade.tools']['X'], 1)

    def test_artifact_fetch_records_the_app_with_no_successful_run(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(e2e_artifacts, 'command', return_value='[]'):
            result = e2e_artifacts.fetch('arcade.tools', e2e.FLEET['arcade.tools'], Path(directory))
        self.assertNotIn('path', result)
        self.assertIn('no successful arcade/tools-hub', result['reason'])


    def test_bounded_download_skips_the_app_and_reuses_a_verified_package(self):
        import hashlib, subprocess
        with tempfile.TemporaryDirectory() as directory:
            packages = Path(directory) / 'packages'
            packages.mkdir()
            (packages / 'A.AppImage').write_bytes(b'appimage')
            (packages / 'A.AppImage.provenance.json').write_text(json.dumps(
                {'databaseId': 7, 'artifact': 'linux', 'sha256': hashlib.sha256(b'appimage').hexdigest()}))
            self.assertEqual(e2e_artifacts.reusable(packages, {'databaseId': 7}, {'name': 'linux'}), packages / 'A.AppImage')
            self.assertIsNone(e2e_artifacts.reusable(packages, {'databaseId': 8}, {'name': 'linux'}))
            (packages / 'A.AppImage').write_bytes(b'tampered')
            self.assertIsNone(e2e_artifacts.reusable(packages, {'databaseId': 7}, {'name': 'linux'}))
            calls = []
            def stalled(argv, timeout):
                calls.append(timeout)
                raise subprocess.TimeoutExpired(argv, timeout)
            with patch.object(e2e_artifacts, 'command', side_effect=stalled):
                with self.assertRaisesRegex(e2e_artifacts.DownloadFailed, 'artifact download failed'):
                    e2e_artifacts.download(['gh'], packages)
            self.assertEqual(calls, [300, 300])
            self.assertFalse(packages.exists())


if __name__ == '__main__':
    unittest.main()
