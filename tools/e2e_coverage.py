#!/usr/bin/env python3
"""Group × app coverage table from tools/e2e.py JSON reports.

    python3 tools/e2e_coverage.py target/e2e-reports [--fleet fleet.json]

Each cell counts per-app results: P pass, F fail, X expected failure (known
product bug), S skipped. A skip is never shown as a pass. Fleet-wide checks
(several apps) count once in every app they name. With --fleet, the skip
reasons recorded by tools/e2e_artifacts.py are listed under the table.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import e2e

GROUPS = ('install', 'trayhost', 'tools-hub', 'shortcuts')
CODES = {'pass': 'P', 'fail': 'F', 'unexpected_pass': 'F', 'expected_failure': 'X', 'skipped': 'S'}


def table(reports: list[dict]) -> tuple[str, dict]:
    cells = {group: {app: {code: 0 for code in 'PFXS'} for app in e2e.FLEET} for group in GROUPS}
    for report in reports:
        for row in report['checks']:
            if row['group'] not in cells:
                continue
            for app in row.get('apps') or []:
                if app in e2e.FLEET:
                    cells[row['group']][app][CODES.get(row['status'], 'F')] += 1
    apps = list(e2e.FLEET)
    lines = ['| group | ' + ' | '.join(a.removeprefix('arcade.') for a in apps) + ' |',
             '|---|' + '---|' * len(apps)]
    for group in GROUPS:
        def cell(counts):
            text = ' '.join(f'{n}{code}' for code, n in counts.items() if n)
            return text or '-'
        lines.append(f'| {group} | ' + ' | '.join(cell(cells[group][app]) for app in apps) + ' |')
    return '\n'.join(lines), cells


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('reports', type=Path)
    parser.add_argument('--fleet', type=Path)
    args = parser.parse_args()
    reports = [json.loads(p.read_text(encoding='utf-8')) for p in sorted(args.reports.glob('*.json'))]
    text, _ = table(reports)
    print(text)
    print('\nP pass, F fail, X expected failure (known product bug), S skipped (prerequisite unavailable)')
    if args.fleet and args.fleet.is_file():
        fleet = json.loads(args.fleet.read_text(encoding='utf-8'))
        print('\nArtifacts:')
        for app in e2e.FLEET:
            item = fleet.get(app, {})
            state = f'SKIP: {item["reason"]}' if 'reason' in item else f'{item.get("artifact")} @ {item.get("headSha", "")[:7]}'
            print(f'- {app}: {state}')
    rows = [r for report in reports for r in report['checks']]
    skipped = [r for r in rows if r['status'] == 'skipped']
    missing = [r for r in rows if r.get('missingArtifact')]
    print(f'\nSkipped: {len(skipped)} of {len(rows)} checks' + (f' ({len(missing)} for a missing artifact or build)' if missing else ''))
    for row in skipped:
        print(f'- {row["group"]}: {row["check"]}: {row["detail"][:200]}')
    known = sorted({(r['check'], r['knownBug']) for report in reports for r in report['checks'] if r['status'] == 'expected_failure'})
    if known:
        print('\nExpected failures (known product bugs):')
        for name, note in known:
            print(f'- {name}: {note}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
