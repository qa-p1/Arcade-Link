#!/usr/bin/env bash
# Analyzes design/dart/*.dart with Arcade Clipboard's strict lints, runs a
# smoke test of arcadeTheme(), the icon painter and every component in
# arcade_widgets.dart (light and dark, no overflow), and runs the shared
# format cases. Needs Flutter (the version Clipboard pins).
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
import 'package:arcade_design_check/arcade_widgets.dart';
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
  for (final brightness in Brightness.values) {
    testWidgets('every component renders without overflow ($brightness)', (tester) async {
      tester.view.physicalSize = const Size(1100, 1600);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final search = TextEditingController(text: 'query');
      var toggled = false;
      var filter = 'all';
      await tester.pumpWidget(MaterialApp(
        theme: arcadeTheme(brightness, 'arcade.clipboard'),
        home: Scaffold(
          body: Row(children: <Widget>[
            ArcadeSidebar(
              appName: 'Arcade Clipboard',
              mark: const ArcadeIcon('clipboard'),
              version: '0.3.0',
              footer: const ArcadeBadge('Synced', tone: ArcadeTone.ok, dot: true),
              items: <Widget>[
                ArcadeNavItem(icon: 'clipboard', label: 'History', count: 12, selected: true, onTap: () {}),
                ArcadeNavItem(icon: 'monitor', label: 'Devices', selected: false, onTap: () {}),
              ],
            ),
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(ArcadeTokens.space32),
                child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: <Widget>[
                  ArcadePageHead(title: 'History', subtitle: '12 clips', actions: <Widget>[
                    ArcadeButton(label: 'Add clip', icon: 'plus', onPressed: () {}),
                    ArcadeButton(label: 'Clear history…', variant: ArcadeButtonVariant.danger, onPressed: () {}),
                    ArcadeButton(label: 'Invite', variant: ArcadeButtonVariant.primary, busy: true, onPressed: () {}),
                    ArcadeButton(label: 'Delete', variant: ArcadeButtonVariant.destructive, small: true, onPressed: () {}),
                    ArcadeButton(label: 'Close', icon: 'x', iconOnly: true, variant: ArcadeButtonVariant.ghost, onPressed: () {}),
                  ]),
                  ArcadeSearchField(controller: search, hint: 'Search clips'),
                  const SizedBox(height: 12),
                  StatefulBuilder(builder: (context, setState) => ArcadeSegmented<String>(
                        options: const <(String, String)>[('all', 'All'), ('text', 'Text'), ('images', 'Images')],
                        counts: const <String, int>{'all': 12},
                        value: filter,
                        onChanged: (v) => setState(() => filter = v),
                      )),
                  const SizedBox(height: 12),
                  ArcadeListSection(title: 'General', meta: '3 settings', children: <Widget>[
                    ArcadeSwitchRow(title: 'Start at login', subtitle: 'Opens in the background', value: toggled, onChanged: (v) => toggled = v),
                    ArcadeRow(
                      title: 'A very long clip title that must ellipsize instead of overflowing the row it sits in',
                      subtitle: 'Laptop · 2 min ago',
                      leading: const ArcadeGlyphTile(child: ArcadeIcon('type')),
                      trailing: const ArcadeKeyCombo(<String>['Ctrl', 'Alt', 'V'], alternatives: <List<String>>[<String>['⌃', '⌥', 'V']]),
                      selected: true,
                      onTap: () {},
                    ),
                  ]),
                  const SizedBox(height: 12),
                  const ArcadeNotice('Pairing request from Phone', tone: ArcadeTone.warn, title: 'Allow this device?'),
                  const SizedBox(height: 12),
                  const ArcadeEmptyState(title: 'No other devices yet', message: 'Invite a device to sync your clips.', icon: 'monitor'),
                  const ArcadeOverlayRow(title: 'Clip', subtitle: 'Text', selected: true),
                  const ArcadeHintBar(<(List<String>, String)>[(<String>['↵'], 'Paste'), (<String>['Ctrl', '↵'], 'Paste as plain text'), (<String>['Esc'], 'Close')]),
                  const ArcadeSkeletonRow(),
                ]),
              ),
            ),
          ]),
        ),
      ));
      await tester.pump(const Duration(milliseconds: 300));
      expect(tester.takeException(), isNull);
      await tester.tap(find.text('Start at login'));
      expect(toggled, isTrue);
      await tester.tap(find.text('Images'));
      await tester.pump();
      expect(filter, 'images');
      await tester.tap(find.byTooltip('Clear search'));
      expect(search.text, isEmpty);
    });
  }
  testWidgets('rows put a wide trailing control under the title on a phone-width window', (tester) async {
    tester.view.physicalSize = const Size(360, 640);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pumpWidget(MaterialApp(
      theme: arcadeTheme(Brightness.light, 'arcade.clipboard'),
      home: Scaffold(
        body: Padding(
          padding: const EdgeInsets.all(ArcadeTokens.space16),
          child: ArcadeListSection(title: 'General', children: <Widget>[
            ArcadeRow(
              title: 'Appearance',
              subtitle: 'Follows your system unless you pick one',
              trailing: ArcadeSegmented<String>(
                options: const <(String, String)>[('system', 'System'), ('light', 'Light'), ('dark', 'Dark')],
                value: 'system',
                onChanged: (_) {},
              ),
            ),
            ArcadeRow(
              title: 'Arcade Shelf',
              trailing: Row(mainAxisSize: MainAxisSize.min, children: <Widget>[
                const ArcadeBadge('Not installed'),
                const SizedBox(width: ArcadeTokens.space8),
                ArcadeButton(label: 'Install', small: true, onPressed: () {}),
              ]),
            ),
          ]),
        ),
      ),
    ));
    expect(tester.takeException(), isNull);
    final title = tester.getRect(find.text('Appearance'));
    final control = tester.getRect(find.text('System'));
    expect(control.top, greaterThan(title.bottom), reason: 'the segmented control sits under the title');
  });
  testWidgets('confirm dialog: Keep is the default, the destructive verb confirms', (tester) async {
    late BuildContext context;
    await tester.pumpWidget(MaterialApp(
      theme: arcadeTheme(Brightness.light, 'arcade.clipboard'),
      home: Scaffold(body: Builder(builder: (c) {
        context = c;
        return const SizedBox();
      })),
    ));
    final result = confirmArcadeDestroy(context, title: 'Clear history?', message: 'Unpinned clips are removed.', confirmLabel: 'Clear history');
    await tester.pumpAndSettle();
    await tester.tap(find.text('Clear history'));
    await tester.pumpAndSettle();
    expect(await result, isTrue);
  });
}
DART
(cd "$work" && flutter pub get >/dev/null && flutter analyze --fatal-infos && flutter test)
(cd "$root" && dart run tools/format_cases.dart)
