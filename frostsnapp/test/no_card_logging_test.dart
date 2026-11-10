import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

// Nothing in the card flows' Dart prints a stage, page, word or entry
// (design §5.5): app logs can be exported, and a word in them would put the
// card in a file. A log may say that something failed, never what was on
// screen.
//
// The rule is checked on the source: no `print` at all, and no `debugPrint`,
// `log` or logger call whose arguments name any card object.

/// Every file the rule covers: lib/verifiable_card/, and the card restore's
/// views.
List<File> _cardSources() => [
  for (final e in Directory('lib/verifiable_card').listSync(recursive: true))
    if (e is File && e.path.endsWith('.dart')) e,
  for (final path in [
    'lib/restoration/enter_card_view.dart',
    'lib/restoration/phone_half_entry.dart',
  ])
    if (File(path).existsSync()) File(path),
];

/// The start of a logging call: `print(`, `debugPrint(`, `log(`, and a
/// logger's methods whatever the logger is called (`logger.i(`,
/// `_log.info(`, `cardLogger.d(`).
final _callStart = RegExp(
  r'\b(print|debugPrint|debugPrintStack|log|developer\.log|_?(?:log|logger)\.\w+|\w+(?:Log|Logger)\.\w+)\s*\(',
);

/// Every logging call in [source], with its whole argument list: up to the
/// parenthesis that closes it, skipping parentheses in strings (a `;` or
/// `)` in a message doesn't end it).
Iterable<({String kind, String args, int start})> _logCalls(
  String source,
) sync* {
  for (final m in _callStart.allMatches(source)) {
    var depth = 1;
    String? quote;
    var i = m.end;
    for (; i < source.length && depth > 0; i++) {
      final c = source[i];
      if (quote != null) {
        if (c == r'\') {
          i++;
        } else if (c == quote) {
          quote = null;
        }
      } else if (c == "'" || c == '"') {
        quote = c;
      } else if (c == '(') {
        depth++;
      } else if (c == ')') {
        depth--;
      }
    }
    yield (
      kind: m.group(1)!,
      args: source.substring(m.end, i - 1),
      start: m.start,
    );
  }
}

/// Words that name what is on a card page.
final _cardObject = RegExp(
  r'stage|page|word|tag|cell|tile|display|entry|answer|question|lookalike|box|state\b',
  caseSensitive: false,
);

void main() {
  test('the rule covers the card sources', () {
    final paths = _cardSources().map((f) => f.path).toList();
    expect(paths, contains('lib/verifiable_card/card_flow.dart'));
    expect(paths, contains('lib/verifiable_card/card_quiz.dart'));
    expect(paths, contains('lib/verifiable_card/card_words.dart'));
    expect(paths, contains('lib/restoration/enter_card_view.dart'));
    expect(paths, contains('lib/restoration/phone_half_entry.dart'));
  });

  test('the check finds a stage log and a print', () {
    const bad = [
      "debugPrint('card flow: \${stage.runtimeType}');",
      "print('box done');",
      "log('page \$page');",
      "debugPrint('\$state');",
      // A ';' or ')' in the message doesn't hide what follows.
      "debugPrint('x; \${stage}');",
      "debugPrint('done (1) ' + page.toString());",
      // Neither does the logger's name.
      "_log.info('word \$word');",
      "cardLogger.d(entry);",
    ];
    for (final line in bad) {
      final calls = _logCalls(line).toList();
      expect(calls, hasLength(1), reason: line);
      final call = calls.single;
      expect(
        call.kind == 'print' || _cardObject.hasMatch(call.args),
        isTrue,
        reason: line,
      );
    }
    final ok = _logCalls(
      "debugPrint('card flow: emitBackupRun failed: \$e');",
    ).single;
    expect(_cardObject.hasMatch(ok.args), isFalse);
    // Not a logger: a dialog, and a call that only ends in "log".
    expect(_logCalls('showDialog(context: c); catalog(x);'), isEmpty);
  });

  test('no card source prints or logs a card object', () {
    final found = <String>[];
    for (final file in _cardSources()) {
      final source = file.readAsStringSync();
      for (final call in _logCalls(source)) {
        if (call.kind == 'print' || _cardObject.hasMatch(call.args)) {
          final line = source.substring(0, call.start).split('\n').length;
          found.add('${file.path}:$line: ${call.kind}(${call.args})');
        }
      }
    }
    expect(found, isEmpty);
  });
}
