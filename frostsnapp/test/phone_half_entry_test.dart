import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/restoration/phone_half_entry.dart';
import 'package:google_fonts/google_fonts.dart';

import 'support/rust_lib.dart';

// The in-app entry for words 33–64 (design §5.3 E2, E3), against the app's
// own word list through the bridge: one field per word, each checked, blank
// for a word that can't be read, and a keyboard asked for no suggestions.

Future<void> _pump(
  WidgetTester tester, {
  required ValueChanged<List<String?>> onSend,
  List<String?>? initial,
}) async {
  tester.view.physicalSize = const Size(360, 900) * 2;
  tester.view.devicePixelRatio = 2;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SingleChildScrollView(
          padding: const EdgeInsets.all(16),
          child: PhoneHalfEntry(onSend: onSend, initial: initial),
        ),
      ),
    ),
  );
}

Finder _field(int number) => find.byKey(ValueKey('word-$number'));

Future<void> _enter(WidgetTester tester, int number, String text) async {
  await tester.ensureVisible(_field(number));
  await tester.enterText(_field(number), text);
  await tester.pump();
}

Future<void> _send(WidgetTester tester) async {
  await tester.ensureVisible(find.byKey(const ValueKey('send-half')));
  await tester.tap(find.byKey(const ValueKey('send-half')));
  await tester.pump();
}

bool _sendEnabled(WidgetTester tester) => tester
    .widget<FilledButton>(find.byKey(const ValueKey('send-half')))
    .enabled;

void main() {
  setUpAll(() async {
    GoogleFonts.config.allowRuntimeFetching = false;
    await initRustLibForTests();
  });

  testWidgets('one field for each of words 33–64, and none for word 1', (
    tester,
  ) async {
    await _pump(tester, onSend: (_) {});
    expect(find.byType(TextFormField), findsNWidgets(32));
    expect(_field(33), findsOne);
    expect(_field(64), findsOne);
    expect(_field(32), findsNothing);
    expect(_field(1), findsNothing);
  });

  testWidgets('the keyboard is asked for no suggestions or learning', (
    tester,
  ) async {
    await _pump(tester, onSend: (_) {});
    for (final field in tester.widgetList<TextField>(find.byType(TextField))) {
      expect(field.autocorrect, isFalse);
      expect(field.enableSuggestions, isFalse);
      expect(field.enableIMEPersonalizedLearning, isFalse);
      expect(field.keyboardType, TextInputType.visiblePassword);
    }
  });

  testWidgets('a word not on the list is refused, and nothing is sent', (
    tester,
  ) async {
    List<String?>? sent;
    await _pump(tester, onSend: (words) => sent = words);
    for (var n = 33; n <= 64; n++) {
      await _enter(tester, n, 'zoo');
    }
    await _enter(tester, 40, 'zooo');
    expect(find.text('Not a backup word'), findsOne);
    await _send(tester);
    expect(sent, isNull);

    await _enter(tester, 40, 'Zone ');
    expect(find.text('Not a backup word'), findsNothing);
    await _send(tester);
    expect(sent, hasLength(32));
    expect(sent![7], 'ZONE');
    expect(sent!.where((w) => w == 'ZOO'), hasLength(31));
  });

  testWidgets('only letters can be typed', (tester) async {
    await _pump(tester, onSend: (_) {});
    await _enter(tester, 33, 'zo1o-');
    expect(find.text('zoo'), findsOne);
  });

  testWidgets('a blank field is a word that can\'t be read, at most 3', (
    tester,
  ) async {
    List<String?>? sent;
    await _pump(tester, onSend: (words) => sent = words);
    expect(_sendEnabled(tester), isFalse);
    for (var n = 33; n <= 60; n++) {
      await _enter(tester, n, 'abandon');
    }
    // 28 entered, 4 blank: one too many to read.
    expect(_sendEnabled(tester), isFalse);
    expect(find.textContaining('Fill in at least 29'), findsOne);
    await _enter(tester, 61, 'ability');
    expect(_sendEnabled(tester), isTrue);
    await _send(tester);
    expect(sent![28], 'ABILITY');
    expect(sent!.sublist(29), [null, null, null]);
  });

  testWidgets('starts from the words given, such as the ones sent last', (
    tester,
  ) async {
    List<String?>? sent;
    final initial = [for (var i = 0; i < 32; i++) i == 5 ? null : 'ZOO'];
    await _pump(tester, onSend: (words) => sent = words, initial: initial);
    await _send(tester);
    expect(sent, initial);
  });
}
