// Runs design/format-cases.json against design/dart/arcade_format.dart:
//   dart run tools/format_cases.dart
import 'dart:convert';
import 'dart:io';

import '../design/dart/arcade_format.dart';

void main() {
  final cases = jsonDecode(File('design/format-cases.json').readAsStringSync()) as Map<String, dynamic>;
  final failures = <String>[];
  void expect(String what, String got, Object? want) {
    if (got != want) failures.add('$what: got "$got", want "$want"');
  }

  for (final c in cases['size'] as List) {
    expect('size ${c[0]}', formatSize(c[0] as int), c[1]);
  }
  for (final c in cases['count'] as List) {
    expect('count ${c[0]}', formatCount(c[0] as int), c[1]);
  }
  for (final c in cases['duration'] as List) {
    final style = c[1] == 'job' ? DurationStyle.job : DurationStyle.media;
    expect('duration ${c[0]}', formatDuration((c[0] as num).toDouble(), style), c[2]);
  }
  for (final c in cases['relative'] as List) {
    expect('relative ${c[0]}', formatRelative(c[0] as int, nowMs: c[1] as int, offsetMinutes: c[2] as int), c[3]);
  }
  if (failures.isNotEmpty) {
    stderr.writeln(failures.join('\n'));
    exit(1);
  }
  stdout.writeln('format cases: ${(cases['size'] as List).length + (cases['count'] as List).length + (cases['duration'] as List).length + (cases['relative'] as List).length} passed');
}
