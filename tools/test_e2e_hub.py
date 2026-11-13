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
            ):
                apps = {f.apps[0] for f in e2e.CHECKS[group] if f.__name__.endswith(suffix)}
                self.assertEqual(apps, set(e2e.FLEET), (group, suffix))
            # Tools never adopts itself: its own AppImage installs it, which
            # the install group's check above covers.
            for suffix in ('_bundle_install', '_update_rollback_incompatible_hold', '_manual_appimage_adoption'):
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
        # No backup bug is open (fixed in Arcade Tools 0.2.0), so no check may excuse a failure.
        self.assertEqual(bugs, {})
        self.assertEqual(flagged, set())
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
        self.assertIn('no successful main', result['reason'])

    def test_shelf_packaging_run_is_selected_after_unrelated_tag_run(self):
        runs = [{'databaseId': 12}, {'databaseId': 11}]
        responses = [json.dumps(runs), json.dumps([{'artifacts': []}]),
                     json.dumps([{'artifacts': [
                         {'name': 'package-windows-x64', 'expired': False},
                         {'name': 'package-linux-x64', 'expired': False},
                     ]}])]
        with patch.object(e2e_artifacts, 'command', side_effect=responses):
            _, chosen = e2e_artifacts.select(e2e.FLEET['arcade.shelf'])
        self.assertIsNotNone(chosen)
        self.assertEqual(chosen[0]['databaseId'], 11)
        self.assertEqual(chosen[1]['name'], 'package-linux-x64')

    def test_dependabot_jobs_do_not_hide_the_packaging_run(self):
        # Box main, 2026-11-11: five Dependabot update jobs were the newest successful runs.
        runs = [{'databaseId': 20 + n, 'event': 'dynamic'} for n in range(5)] + [{'databaseId': 19, 'event': 'push'}]
        responses = [json.dumps(runs), json.dumps([{'artifacts': [{'name': 'arcade-box-appimage', 'expired': False}]}])]
        with patch.object(e2e_artifacts, 'command', side_effect=responses) as command:
            _, chosen = e2e_artifacts.select(e2e.FLEET['arcade.box'])
        self.assertIsNotNone(chosen)
        self.assertEqual(chosen[0]['databaseId'], 19)
        self.assertIn('actions/runs/19/artifacts', ' '.join(command.call_args_list[1].args[0]))

    def test_link_off_edits_current_profiles_without_recreating_legacy_files(self):
        with patch.object(e2e, 'CHECKS', {}):
            e2e.load_check_modules(real_apps=True)
            disable_link = e2e.CHECKS['tools-hub'][0].__globals__['_disable_native_link']
        from types import SimpleNamespace
        cases = [
            ('arcade.look', 'config/arcade/look/settings.json',
             'config/arcade-look/config.json', 'linkEnabled'),
            ('arcade.wheel', 'config/arcade/wheel/settings.json',
             'config/Arcade Wheel/Arcade Wheel/config.json', 'link'),
            ('arcade.clipboard', 'clipdata/settings.json',
             'data/dev.arcade.clipboard/shared_preferences.json', 'link_enabled'),
        ]
        with tempfile.TemporaryDirectory() as directory:
            session = SimpleNamespace(root=Path(directory))
            for app, current, legacy, key in cases:
                with self.subTest(app=app):
                    path = session.root / current
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(json.dumps({'theme': 'light', 'futureSetting': 17}))
                    disable_link(session, app)
                    saved = json.loads(path.read_text())
                    self.assertEqual(saved[key], {'enabled': False} if key == 'link' else False)
                    self.assertEqual(saved['theme'], 'light')
                    self.assertEqual(saved['futureSetting'], 17)
                    self.assertFalse((session.root / legacy).exists())


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
