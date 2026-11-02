import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/restoration/enter_card_view.dart';
import 'package:frostsnap/restoration/phone_half_entry.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/verifiable_card/card_flow.dart';
import 'package:frostsnap/verifiable_card/alarm_view.dart';
import 'package:frostsnap/verifiable_card/card_flow_preview.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';
import 'package:frostsnap/verifiable_card/card_quiz.dart';
import 'package:frostsnap/verifiable_card/card_word.dart';
import 'package:frostsnap/verifiable_card/card_words.dart';
import 'package:frostsnap/verifiable_card/hold_button.dart';

import 'support/card_flow_pages.dart';
import 'support/card_fonts.dart';

// Drives the debug preview's fake device through whole flows: a split card
// from the intro to "Backup checked", a disconnect on the pages of words and
// its question (owner B9), a later check, a device round that fails, no Stop
// after N (owner B12), every scenario, and a restore. Every page renders, and
// every input sends what it should.

const _tick = Duration(milliseconds: 10);

Future<PreviewCardFlowDriver> _pumpFlow(
  WidgetTester tester,
  CardFlowPreviewScenario scenario, {
  Duration tick = _tick,
  int proofsPerTick = 2,
}) async {
  tester.view.physicalSize = const Size(412, 915) * 2;
  tester.view.devicePixelRatio = 2;
  tester.view.padding = const FakeViewPadding(
    top: statusBarHeight * 2,
    bottom: navigationBarHeight * 2,
  );
  addTearDown(tester.view.reset);
  final driver = PreviewCardFlowDriver(
    scenario,
    tick: tick,
    proofsPerTick: proofsPerTick,
  );
  await tester.pumpWidget(
    MaterialApp(
      theme: appTheme(),
      home: Scaffold(
        body: SafeArea(top: false, child: VerifiableCardFlow(driver: driver)),
      ),
    ),
  );
  await tester.pump();
  return driver;
}

Future<void> _pumpUntilTrue(
  WidgetTester tester,
  bool Function() done, {
  Duration limit = const Duration(seconds: 30),
  String what = 'the condition',
}) async {
  const step = Duration(milliseconds: 50);
  for (var t = Duration.zero; t < limit; t += step) {
    if (done()) return;
    await tester.pump(step);
  }
  fail('$what never came');
}

Future<void> _pumpUntil(WidgetTester tester, Finder finder) => _pumpUntilTrue(
  tester,
  () => finder.evaluate().isNotEmpty,
  what: finder.describeMatch(Plurality.one),
);

Future<void> _hold(WidgetTester tester, Finder finder) async {
  final hold = await tester.startGesture(tester.getCenter(finder));
  for (var i = 0; i < 50; i++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
  await hold.up();
  await tester.pump();
}

/// Closes the page, so the fake device's timers run out with it.
Future<void> _close(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
  await tester.pump(const Duration(seconds: 1));
}

Finder _option(String word) =>
    find.byWidgetPredicate((w) => w is CardWord && w.word == word);

/// Taps [finder]'s last match: while pages cross-fade, the one coming in.
Future<void> _tap(WidgetTester tester, Finder finder) async {
  await tester.ensureVisible(finder.last);
  await tester.tap(finder.last);
  await tester.pump();
}

/// Taps Next on the page of words on screen once it is on (it is off while
/// the last tap is in flight).
Future<void> _tapNext(WidgetTester tester) async {
  final next = find.widgetWithText(FilledButton, 'Next');
  await _pumpUntilTrue(
    tester,
    () =>
        next.evaluate().isNotEmpty &&
        tester.widget<FilledButton>(next.last).onPressed != null,
    what: 'Next',
  );
  await _tap(tester, next);
}

/// Holds "I've written words 1–32" once the device's quiz has passed (owner
/// B1): no word of 33–64 shows before it.
Future<void> _confirmDeviceHalf(
  WidgetTester tester,
  PreviewCardFlowDriver driver,
) async {
  await _pumpUntil(tester, find.text('Have you written words 1–32?'));
  expect(find.byType(CardWordRow), findsNothing);
  expect(driver.calls, isNot(contains('device half written')));
  await _hold(tester, find.byType(HoldToConfirmButton));
  expect(driver.calls, contains('device half written'));
}

/// Taps Back on the page of words on screen once it is on.
Future<void> _tapBack(WidgetTester tester) async {
  final back = find.widgetWithText(OutlinedButton, 'Back');
  await _pumpUntilTrue(
    tester,
    () =>
        back.evaluate().isNotEmpty &&
        tester.widget<OutlinedButton>(back.last).onPressed != null,
    what: 'Back',
  );
  await _tap(tester, back);
}

/// Writes words 33–64 off the pages, holds the confirm, and passes the quiz
/// with one wrong pick. Returns the words written.
Future<List<String>> _phoneHalf(
  WidgetTester tester,
  PreviewCardFlowDriver driver,
) async {
  final written = <String>[];
  await _pumpUntilTrue(
    tester,
    () => driver.state.stage is CardFlowStage_PhoneWords,
    what: 'the first page',
  );
  for (var tries = 0; ; tries++) {
    if (tries > 2000) {
      fail('the pages never reached the confirm: ${driver.state.stage}');
    }
    final stage = driver.state.stage;
    if (stage is CardFlowStage_PhoneConfirm) break;
    if (stage is CardFlowStage_PhoneWords &&
        written.length == stage.firstWord - 33) {
      written.addAll(stage.words);
    }
    if (stage is CardFlowStage_PhoneAllWords) expect(stage.words, written);
    // Next is off while the last tap is in flight, and the page that is
    // fading out has one too: tap the new page's once it is on.
    final next = find.widgetWithText(FilledButton, 'Next');
    if ((stage is CardFlowStage_PhoneWords ||
            stage is CardFlowStage_PhoneAllWords) &&
        next.evaluate().isNotEmpty &&
        tester.widget<FilledButton>(next.last).onPressed != null) {
      await tester.ensureVisible(next.last);
      await tester.tap(next.last, warnIfMissed: false);
    }
    await tester.pump(const Duration(milliseconds: 50));
  }
  await _pumpUntil(tester, find.byType(HoldToConfirmButton));
  await _hold(tester, find.byType(HoldToConfirmButton));

  var missed = false;
  for (var tries = 0; ; tries++) {
    if (tries > 2000) fail('the quiz never ended');
    await tester.pump(const Duration(milliseconds: 50));
    final stage = driver.state.stage;
    if (stage is! CardFlowStage_SelfCheck) break;
    if (find.text('Select word ${stage.wordNumber}').evaluate().isEmpty) {
      continue;
    }
    final word = written[stage.wordNumber - 33];
    final pick = missed ? word : stage.options.firstWhere((w) => w != word);
    missed = true;
    await _tap(tester, _option(pick));
    await tester.pump(quizWrongDelay + quizRightDelay);
  }
  return written;
}

/// The device round on the fake device, then the phone round's five
/// questions, the second answered "None of these" first.
Future<void> _challenge(
  WidgetTester tester,
  PreviewCardFlowDriver driver,
) async {
  await _tap(tester, find.text('Continue'));
  await _pumpUntil(tester, find.text('Answer on your device'));
  for (final (k, w) in previewRound.indexed) {
    await _pumpUntilTrue(
      tester,
      () =>
          driver.state.stage is CardFlowStage_Question &&
          find.text('Select word ${w.wordNumber}').evaluate().isNotEmpty,
      what: 'question $k',
    );
    if (k == 1) {
      await _tap(tester, find.text(noneOfTheseLabel));
      await _pumpUntil(
        tester,
        find.text('Look again at word ${w.wordNumber} on your card.'),
      );
    }
    await _tap(tester, _option(w.word));
  }
}

void main() {
  setUpAll(loadAppFonts);

  testWidgets('a whole split card, from the intro to Backup checked', (
    tester,
  ) async {
    final driver = await _pumpFlow(tester, CardFlowPreviewScenario.fullRun);
    expect(find.text('Preparing your card'), findsOne);
    await _pumpUntil(tester, find.text('Write words 1–32 from your device'));
    expect(driver.backupMarks, 0);
    await _confirmDeviceHalf(tester, driver);

    final written = await _phoneHalf(tester, driver);
    expect(written, previewPhoneHalf);
    await _pumpUntil(tester, find.text('Success! Words 33–64 checked'));
    // Both halves recorded: the backup run's step, before any challenge.
    expect(driver.backupMarks, 1);
    // Every proof in, then Continue. This is the fake driver's own gating
    // (its finalBegin refuses before then), not the app's: the page's side is
    // card_flow_test.dart ("Continue stays off until every proof is
    // verified") and core's is final_begin_needs_proving (app review n3).
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_ReadyForFinalCheck,
      what: 'every proof',
    );
    expect(driver.state.proofsVerified, driver.sealSize);
    await tester.pump(const Duration(milliseconds: 500));

    await _challenge(tester, driver);
    await _pumpUntil(tester, find.text('Backup checked'));
    expect(driver.backupMarks, 1, reason: 'never again at the verdict');
    expect(driver.calls.where((c) => c == 'answer'), hasLength(6));
  });

  testWidgets('a disconnect on the words asks "Was that you?", then the pages '
      'keep working', (tester) async {
    // Slow proofs, so they're still coming at the unplug.
    final driver = await _pumpFlow(
      tester,
      CardFlowPreviewScenario.fullRunWithUnplug,
      proofsPerTick: 1,
    );
    await _confirmDeviceHalf(tester, driver);
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_PhoneWords,
      what: 'the first page',
    );
    // The preview unplugs the device on the third page, after N: the
    // question covers the page.
    await _tapNext(tester);
    await _tapNext(tester);
    await _pumpUntil(tester, find.text(interruptedTitle));
    await tester.pump(const Duration(milliseconds: 400));
    expect(driver.state.deviceConnected, isFalse);
    expect(find.text(interruptedDisconnectedLine), findsOne);
    expect(find.byType(CardWordRow), findsNothing);
    expect(find.widgetWithText(TextButton, 'Stop'), findsNothing);
    await _tap(tester, find.text(interruptedYes));
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_PhoneWords,
      what: 'the page again',
    );
    expect(driver.calls, contains('it was me'));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.textContaining('Your device is unplugged.'), findsOne);
    expect(find.byType(CardWordRow), findsWidgets);
    // The proofs wait for the device.
    final proofs = driver.state.proofsVerified;
    expect(proofs, lessThan(driver.sealSize));
    expect(
      find.text('Your device checks your card once it is plugged back in.'),
      findsOne,
    );
    await tester.pump(const Duration(milliseconds: 200));
    expect(driver.state.proofsVerified, proofs);
    // The pages before it still work with the device out, Back and Next as
    // fast as they come.
    await _tapBack(tester);
    await _pumpUntilTrue(
      tester,
      () =>
          driver.state.stage is CardFlowStage_PhoneWords &&
          (driver.state.stage as CardFlowStage_PhoneWords).page == 1,
      what: 'page 2 again',
    );
    expect(driver.state.deviceConnected, isFalse);
    await _tapBack(tester);
    await _phoneHalf(tester, driver);
    await _pumpUntil(tester, find.text('Success! Words 33–64 checked'));
  });

  testWidgets('a later check quizzes 37 words and records nothing', (
    tester,
  ) async {
    final driver = await _pumpFlow(tester, CardFlowPreviewScenario.recheck);
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_SelfCheck,
      what: 'the quiz',
    );
    final all = [...previewPhoneHalf, for (final w in previewRound) w.word];
    while (true) {
      final stage = driver.state.stage;
      if (stage is! CardFlowStage_SelfCheck) break;
      final CardFlowStage_SelfCheck(:k, :total) = stage;
      expect(total, 37);
      await _pumpUntil(tester, find.bySemanticsLabel('$k of 37 words checked'));
      await _tap(tester, _option(all[k]));
      await tester.pump(quizRightDelay * 2);
    }
    await _pumpUntil(tester, find.text('Both checks done.'));
    expect(driver.backupMarks, 1, reason: 'already recorded; idempotent');
  });

  testWidgets('a device round that fails is its alarm, with no phone round', (
    tester,
  ) async {
    final driver = await _pumpFlow(
      tester,
      CardFlowPreviewScenario.deviceRoundFails,
    );
    await _pumpUntil(tester, find.text('Continue'));
    await _tap(tester, find.text('Continue'));
    await _pumpUntil(tester, find.text("Stop. Don't use this wallet."));
    expect(driver.calls.where((c) => c == 'answer'), isEmpty);
  });

  testWidgets('no Stop once N is out: the words, the device round, the phone '
      'round', (tester) async {
    final driver = await _pumpFlow(tester, CardFlowPreviewScenario.fullRun);
    await _confirmDeviceHalf(tester, driver);
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_PhoneWords,
      what: 'the first page',
    );
    final stop = find.widgetWithText(TextButton, 'Stop');
    expect(driver.state.checkStarted, isTrue);
    expect(stop, findsNothing);
    await _phoneHalf(tester, driver);
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_ReadyForFinalCheck,
      what: 'every proof',
    );
    expect(stop, findsNothing);
    await tester.pump(const Duration(milliseconds: 500));
    await _tap(tester, find.text('Continue'));
    await _pumpUntil(tester, find.text('Answer on your device'));
    expect(stop, findsNothing);
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_Question,
      what: 'the phone round',
    );
    expect(stop, findsNothing);
    expect(driver.calls, isNot(contains('stop')));
  });

  testWidgets('"No, I didn\'t" after a disconnect: the suspect alarm', (
    tester,
  ) async {
    final driver = await _pumpFlow(
      tester,
      CardFlowPreviewScenario.fullRunWithUnplug,
    );
    await _confirmDeviceHalf(tester, driver);
    await _tapNext(tester);
    await _tapNext(tester);
    await _pumpUntil(tester, find.text(interruptedTitle));
    await tester.ensureVisible(find.text(interruptedNo));
    final hold = await tester.startGesture(
      tester.getCenter(find.text(interruptedNo)),
    );
    for (var i = 0; i < 60; i++) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    await hold.up();
    await tester.pump();
    await _pumpUntil(tester, find.text("Stop. Don't use this wallet."));
    expect(driver.calls, contains("it wasn't me"));
    expect(find.text(nextStepLine(NextStep.withoutThisDevice)), findsOne);
    await _close(tester);
  });

  testWidgets('the question on opening, then the words', (tester) async {
    final driver = await _pumpFlow(
      tester,
      CardFlowPreviewScenario.interruptedOnOpen,
    );
    await _pumpUntil(tester, find.text(interruptedTitle));
    expect(find.text(interruptedClosedLine), findsOne);
    await _tap(tester, find.text(interruptedYes));
    await _pumpUntilTrue(
      tester,
      () => driver.state.stage is CardFlowStage_PhoneWords,
      what: 'the words',
    );
    await _close(tester);
  });

  for (final scenario in CardFlowPreviewScenario.values) {
    testWidgets('${scenario.title} shows its page', (tester) async {
      await _pumpFlow(tester, scenario);
      await tester.pump(const Duration(seconds: 2));
      expect(tester.takeException(), isNull);
    });
  }

  Future<PreviewCardRestoreDriver> pumpRestore(
    WidgetTester tester, {
    PreviewRestoreOutcome outcome = PreviewRestoreOutcome.couldNotRead,
    RestoreFailures? failures,
  }) async {
    tester.view.physicalSize = const Size(412, 1200) * 2;
    tester.view.devicePixelRatio = 2;
    addTearDown(tester.view.reset);
    final driver = PreviewCardRestoreDriver(tick: _tick, outcome: outcome);
    await tester.pumpWidget(
      MaterialApp(
        theme: appTheme(),
        home: Scaffold(
          body: EnterCardView(
            driver: driver,
            deviceName: 'Preview',
            lookup: WordLookup(isWord: (_) => true),
            onFinished: (_) {},
            onCancel: () {},
            onError: (_) {},
            failures: failures,
            failureKey: failures == null ? null : 'preview',
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 200));
    return driver;
  }

  testWidgets('a restore: words 33–64 here, sent, then the device decodes', (
    tester,
  ) async {
    final driver = await pumpRestore(tester);
    for (var i = 0; i < phoneHalfWords; i++) {
      final field = find.byKey(ValueKey('word-${phoneHalfFirst + i}'));
      await tester.ensureVisible(field);
      await tester.enterText(field, 'zoo');
      await tester.pump();
    }
    await _tap(tester, find.byKey(const ValueKey('send-half')));
    expect(driver.sent, hasLength(1));
    await _pumpUntil(
      tester,
      find.text('Sent. Now finish words 1–32 on your device.'),
    );
    await _pumpUntil(
      tester,
      find.text('Checking your card on the device… (about 10 seconds)'),
    );
    await _pumpUntil(tester, find.text('Edit words 33–64'));
  });

  // The preview walks through each failure step (owner B14): the fake device
  // keeps words 1–32 after a failed read, so each send decodes again.
  testWidgets('a restore that never reads: each failure step, and its button', (
    tester,
  ) async {
    final failures = RestoreFailures();
    final driver = await pumpRestore(tester, failures: failures);
    for (var i = 0; i < phoneHalfWords; i++) {
      final field = find.byKey(ValueKey('word-${phoneHalfFirst + i}'));
      await tester.ensureVisible(field);
      await tester.enterText(field, 'zoo');
      await tester.pump();
    }
    await _tap(tester, find.byKey(const ValueKey('send-half')));
    await _pumpUntil(
      tester,
      find.textContaining('Check the words you entered'),
    );
    await _tap(tester, find.text('Edit words 33–64'));
    await _tap(tester, find.byKey(const ValueKey('send-half')));
    expect(driver.sent, hasLength(2));
    await _pumpUntil(
      tester,
      find.textContaining('Try the Frostsnap app on a different device'),
    );
    await _tap(tester, find.text("I've already tried it in a different app."));
    expect(find.textContaining('It could be the Frostsnap device.'), findsOne);
    await _tap(tester, find.text("I don't have another Frostsnap device"));
    expect(find.textContaining('Use offline recovery'), findsOne);
    expect(failures.step('preview'), RestoreFailureStep.offlineRecovery);
  });

  testWidgets("a restore of another wallet's card: its own page", (
    tester,
  ) async {
    final failures = RestoreFailures();
    await pumpRestore(
      tester,
      outcome: PreviewRestoreOutcome.wrongWallet,
      failures: failures,
    );
    for (var i = 0; i < phoneHalfWords; i++) {
      final field = find.byKey(ValueKey('word-${phoneHalfFirst + i}'));
      await tester.ensureVisible(field);
      await tester.enterText(field, 'zoo');
      await tester.pump();
    }
    await _tap(tester, find.byKey(const ValueKey('send-half')));
    await _pumpUntil(tester, find.text(wrongWalletLine));
    expect(failures.count('preview'), 0);
  });
}
