import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/restoration/phone_half_entry.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/verifiable_card/card_flow.dart';
import 'package:google_fonts/google_fonts.dart';

import 'support/card_flow_pages.dart';

// Accessibility services are third-party code, as keyboards are, so no
// card word may reach the semantics tree (design §5.2, critique 11): not on
// the pages of words 33–64, the quizzes, the later check or the restore's
// entry. The quiz buttons are "Option 1", "Option 2", ...

/// Words that appear on the screens under test, and must not be heard.
const _sentinels = ['ZEBRA', 'ZERO', 'ZONE', 'ZOO', 'SIZE', 'SIDE', 'SISTER'];

class _Driver implements CardFlowDriver {
  final controller = StreamController<CardFlowState>();

  @override
  Stream<CardFlowState> start() => controller.stream;
  @override
  Stream<bool> deviceConnected() => Stream.value(true);
  @override
  int get sealSize => fakeSealSize;
  @override
  Future<void> deviceHalfWritten() async {}

  @override
  Future<void> writeUnverified() async {}
  @override
  Future<void> phoneNext() async {}
  @override
  Future<void> phoneBack() async {}
  @override
  Future<void> phoneHalfWritten() async {}
  @override
  Future<bool> selfCheckAnswer(String word) async => true;
  @override
  Future<void> finalBegin() async {}
  @override
  Future<void> answer(CardAnswer answer) async {}
  @override
  Future<CardFlowState?> answerInterruption(bool causedByUser) async => null;
  @override
  Future<void> recordChanged() async {}
  @override
  Future<void> markBackupComplete() async {}
  @override
  void leave() => controller.close();
}

/// Every string the semantics tree holds.
List<String> _heard(WidgetTester tester) {
  final out = <String>[];
  void walk(SemanticsNode node) {
    final data = node.getSemanticsData();
    out.addAll(
      [
        data.label,
        data.value,
        data.hint,
        data.tooltip,
        data.increasedValue,
        data.decreasedValue,
      ].where((s) => s.isNotEmpty),
    );
    node.visitChildren((child) {
      walk(child);
      return true;
    });
  }

  var node = tester.getSemantics(find.byType(Scaffold).first);
  while (node.parent != null) {
    node = node.parent!;
  }
  walk(node);
  return out;
}

void _expectNoWord(WidgetTester tester, {bool someHeard = true}) {
  final heard = _heard(tester).join(' | ').toUpperCase();
  if (someHeard) expect(heard, isNotEmpty, reason: 'the tree is there');
  for (final word in _sentinels) {
    expect(heard.contains(word), isFalse, reason: '$word in: $heard');
    expect(
      find.bySemanticsLabel(RegExp(word, caseSensitive: false)),
      findsNothing,
    );
  }
}

Future<void> _flow(WidgetTester tester, CardFlowStage stage) async {
  final driver = _Driver();
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(body: VerifiableCardFlow(driver: driver)),
    ),
  );
  driver.controller.add(cardFlowState(stage));
  for (var i = 0; i < 10; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

/// A word list of four: the Z words.
final _lookup = WordLookup(
  isWord: (word) =>
      const ['ZEBRA', 'ZERO', 'ZONE', 'ZOO'].contains(word.toUpperCase()),
);

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  final stages = <String, CardFlowStage>{
    'P3 a page of words': const CardFlowStage.phoneWords(
      page: 0,
      firstWord: 33,
      words: ['ZEBRA', 'ZERO', 'ZONE'],
    ),
    'P4 all the words': CardFlowStage.phoneAllWords(
      words: [for (var i = 0; i < 32; i++) _sentinels[i % _sentinels.length]],
    ),
    'P6 the self-check': const CardFlowStage.selfCheck(
      k: 0,
      total: 32,
      wordNumber: 33,
      options: ['SIZE', 'SIDE', 'SISTER'],
      wrong: false,
      recheck: false,
    ),
    'P11 the phone round': const CardFlowStage.question(
      k: 0,
      wordNumber: 7,
      options: ['SIZE', 'SIDE', 'SISTER'],
      secondLook: false,
    ),
    'P15 a later check': const CardFlowStage.selfCheck(
      k: 33,
      total: 37,
      wordNumber: 7,
      options: ['ZOO', 'ZONE', 'ZERO'],
      wrong: true,
      recheck: true,
    ),
  };
  for (final MapEntry(key: name, value: stage) in stages.entries) {
    testWidgets('$name: no word is heard', (tester) async {
      final semantics = tester.ensureSemantics();
      await _flow(tester, stage);
      _expectNoWord(tester);
      semantics.dispose();
    });
  }

  testWidgets('the check hears a word that is not hidden', (tester) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      const MaterialApp(home: Scaffold(body: Text('ZONE'))),
    );
    expect(_heard(tester).join(' '), contains('ZONE'));
    semantics.dispose();
  });

  testWidgets('the quiz buttons are heard as options', (tester) async {
    final semantics = tester.ensureSemantics();
    await _flow(tester, stages['P11 the phone round']!);
    expect(find.bySemanticsLabel('Option 1'), findsOne);
    expect(find.bySemanticsLabel('Option 3'), findsOne);
    expect(find.bySemanticsLabel('None of these'), findsOne);
    semantics.dispose();
  });

  testWidgets('E2 the entry, with its words typed: no word is heard', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: PhoneHalfEntry(
              lookup: _lookup,
              initial: [for (var i = 0; i < 32; i++) _sentinels[i % 4]],
              onSend: (_) {},
            ),
          ),
        ),
      ),
    );
    await tester.enterText(find.byKey(const ValueKey('word-34')), 'zoo');
    await tester.pump();
    expect(find.text('zoo'), findsOne);
    _expectNoWord(tester, someHeard: false);
    semantics.dispose();
  });
}
