#!/usr/bin/env bash
# Analyzes design/dart/*.dart with Arcade Clipboard's strict lints, runs a
# smoke test of arcadeTheme() and the icon painter, and runs the shared format
# cases. Needs Flutter (the version Clipboard pins).
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/lib" "$work/test"
cp "$root"/design/dart/*.dart "$work/lib/"
cat > "$work/pubspec.yaml" <<'YAML'
name: arcade_design_check
publish_to: none
environment:
  sdk: '>=3.4.0 <4.0.0'
dependencies:
  flutter:
    sdk: flutter
dev_dependencies:
  flutter_test:
    sdk: flutter
  flutter_lints: ^6.0.0
YAML
cat > "$work/analysis_options.yaml" <<'YAML'
include: package:flutter_lints/flutter.yaml
analyzer:
  language:
    strict-casts: true
    strict-inference: true
    strict-raw-types: true
linter:
  rules:
    avoid_print: true
    always_declare_return_types: true
    directives_ordering: true
    prefer_const_constructors: true
YAML
cat > "$work/test/kit_test.dart" <<'DART'
import 'package:arcade_design_check/arcade_icons.dart';
import 'package:arcade_design_check/arcade_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('themes build for every app in both brightnesses', () {
    for (final id in arcadeAccents.keys) {
      for (final b in Brightness.values) {
        final accent = arcadeAccents[id]!;
        expect(arcadeTheme(b, id).colorScheme.primary, b == Brightness.light ? accent.light : accent.dark);
      }
    }
  });
  testWidgets('icons paint', (tester) async {
    await tester.pumpWidget(MaterialApp(theme: arcadeTheme(Brightness.dark, 'arcade.clipboard'), home: const Center(child: ArcadeIcon('clipboard', size: 24))));
    expect(find.byType(ArcadeIcon), findsOneWidget);
    for (final name in arcadeIconOps.keys) {
      final bounds = arcadeIconPath(name, 24).getBounds();
      expect(bounds.width + bounds.height, greaterThan(4), reason: name);
    }
  });
}
DART
(cd "$work" && flutter pub get >/dev/null && flutter analyze --fatal-infos && flutter test)
(cd "$root" && dart run tools/format_cases.dart)
