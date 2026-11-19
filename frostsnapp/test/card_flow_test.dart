import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/verifiable_card/alarm_view.dart';
import 'package:frostsnap/verifiable_card/card_finished_view.dart';
import 'package:frostsnap/verifiable_card/card_flow.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';
import 'package:frostsnap/verifiable_card/card_quiz.dart';
import 'package:frostsnap/verifiable_card/card_word.dart';
import 'package:frostsnap/verifiable_card/card_words.dart';
import 'package:frostsnap/verifiable_card/hold_button.dart';

import 'support/card_flow_pages.dart';
import 'support/card_fonts.dart';

// The split card flow (design §5.2) against a driver the test moves by hand:
// what each stage draws, what each input sends, and the cases where the
// flow's state changes under an open sheet, or doesn't change at all.

class _ManualDriver implements CardFlowDriver {
  var controller = StreamController<CardFlowState>();
  final connected = StreamController<bool>();
  final sent = <String>[];
  var starts = 0;

  /// What the next self-check picks return.
  var pickRight = true;

  /// Each start is a new flow, with its own stream.
  @override
  Stream<CardFlowState> start() {
    starts += 1;
    if (starts > 1) controller = StreamController<CardFlowState>();
    return controller.stream;
  }

  @override
  Stream<bool> deviceConnected() => connected.stream;

  @override
  int get sealSize => fakeSealSize;

  @override
  Future<void> deviceHalfWritten() async => sent.add('device half written');

  @override
  Future<void> writeUnverified() async => sent.add('write unverified');

  @override
  Future<void> phoneNext() async => sent.add('next');

  @override
  Future<void> phoneBack() async => sent.add('back');

  @override
  Future<void> phoneHalfWritten() async => sent.add('written');

  @override
  Future<bool> selfCheckAnswer(String word) async {
    sent.add('pick $word');
    return pickRight;
  }

  @override
  Future<void> finalBegin() async => sent.add('final');

  @override
  Future<void> answer(CardAnswer answer) async => sent.add(switch (answer) {
    CardAnswer_Picked(:final word) => 'answer $word',
    CardAnswer_NoneOfThese() => 'answer none',
  });

  /// What an answer to the interruption's question returns: its verdict when
  /// no flow heard it.
  CardFlowState? unheardAnswer;

  @override
  Future<CardFlowState?> answerInterruption(bool causedByUser) async {
    sent.add(causedByUser ? 'it was me' : "it wasn't me");
    return unheardAnswer;
  }

  var recordChanges = 0;

  @override
  Future<void> recordChanged() async => recordChanges += 1;

  var backupMarks = 0;

  @override
  Future<void> markBackupComplete() async => backupMarks += 1;

  @override
  void leave() {
    controller.close();
    connected.close();
  }
}

const _question = CardFlowStage.question(
  k: 0,
  wordNumber: 7,
  options: ['SIZE', 'SIDE', 'SISTER'],
  secondLook: false,
);

const _page0 = CardFlowStage.phoneWords(
  page: 0,
  firstWord: 33,
  words: ['SIZE', 'CLOCK', 'MARGIN'],
);

CardFlowState _state(
  CardFlowStage stage, {
  bool connected = true,
  int points = fakeSealSize,
  int proofs = 0,
  bool started = false,
  bool halves = false,
  bool deviceDone = false,
  bool unverified = false,
}) => cardFlowState(
  stage,
  connected: connected,
  points: points,
  proofs: proofs,
  started: started,
  halves: halves,
  deviceDone: deviceDone,
  unverified: unverified,
);

/// The flow as the app shows it on a 412x915 phone: a fullscreen route with
/// the status and gesture bars.
Future<void> _pumpFlow(WidgetTester tester, Widget flow) async {
  tester.view.physicalSize = const Size(412, 915) * 2;
  tester.view.devicePixelRatio = 2;
  tester.view.padding = const FakeViewPadding(
    top: statusBarHeight * 2,
    bottom: navigationBarHeight * 2,
  );
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      theme: appTheme(),
      debugShowCheckedModeBanner: false,
      home: Scaffold(body: SafeArea(top: false, child: flow)),
    ),
  );
}

/// Fixed-step pumps: some stages hold a spinner, so pumpAndSettle would
/// never return.
Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 12; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

Future<_ManualDriver> _pump(WidgetTester tester, CardFlowState first) async {
  final driver = _ManualDriver();
  driver.connected.add(true);
  await _pumpFlow(tester, VerifiableCardFlow(driver: driver));
  driver.controller.add(first);
  await _settle(tester);
  return driver;
}

Future<void> _emit(
  WidgetTester tester,
  _ManualDriver driver,
  CardFlowState state,
) async {
  driver.controller.add(state);
  await _settle(tester);
}

/// Holds [finder] for [duration], pumping frames so the hold runs.
Future<void> _hold(
  WidgetTester tester,
  Finder finder,
  Duration duration,
) async {
  final hold = await tester.startGesture(tester.getCenter(finder));
  const step = Duration(milliseconds: 50);
  for (var t = Duration.zero; t < duration; t += step) {
    await tester.pump(step);
  }
  await hold.up();
  await tester.pump();
}

Finder get _stopButton => find.widgetWithText(TextButton, 'Stop');

ButtonStyleButton _button(WidgetTester tester, String label) =>
    tester.widget<ButtonStyleButton>(
      find.ancestor(
        of: find.text(label),
        matching: find.bySubtype<ButtonStyleButton>(),
      ),
    );

Finder _option(String word) =>
    find.byWidgetPredicate((w) => w is CardWord && w.word == word);

Future<void> _tapButton(WidgetTester tester, String label) async {
  final button = find.ancestor(
    of: find.text(label),
    matching: find.bySubtype<ButtonStyleButton>(),
  );
  await tester.ensureVisible(button);
  await tester.tap(button);
  await _settle(tester);
}

void main() {
  setUpAll(loadAppFonts);

  group('writing', () {
    testWidgets(
      'the intro: what the card is, the bar and the experimental label',
      (tester) async {
        await _pump(tester, _state(const CardFlowStage.preparingCard()));
        expect(find.text('Preparing your card'), findsOne);
        expect(find.text(introLine), findsOne);
        expect(
          find.text(
            'Keep your device plugged in. This takes about 20 seconds.',
          ),
          findsOne,
        );
        expect(find.text(experimentalLabel), findsOne);
        final bar = tester.widget<LinearProgressIndicator>(
          find.byType(LinearProgressIndicator),
        );
        expect(bar.value, isNull, reason: "the seal's own time isn't known");
        expect(find.byType(CardWordRow), findsNothing);
      },
    );

    testWidgets('pinning: the bar covers the points', (tester) async {
      await _pump(
        tester,
        _state(const CardFlowStage.pinning(), points: fakeSealSize ~/ 2),
      );
      expect(find.text('Preparing your card'), findsOne);
      final bar = tester.widget<LinearProgressIndicator>(
        find.byType(LinearProgressIndicator),
      );
      expect(bar.value, closeTo(0.5, 0.01));
      expect(find.byType(CardWordRow), findsNothing);
    });

    testWidgets('words 1–32 are on the device; the page moves on by itself', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.deviceHalf()),
      );
      expect(find.text('Write words 1–32 from your device'), findsOne);
      expect(find.text('Words 33–64 come on this phone after that.'), findsOne);
      expect(find.byType(CardWordRow), findsNothing);
      expect(find.byType(LinearProgressIndicator), findsNothing);
      // It moves on by itself: no button but the header's Close.
      expect(find.bySubtype<ButtonStyleButton>(), findsOne);
      expect(find.byTooltip('Close'), findsOne);
      expect(driver.sent, isEmpty);
    });

    // Core enforces that words 33–64 follow the hold (S3's tests); this checks
    // the page side: no word on the confirm, and the hold sends the call.
    testWidgets("the confirm shows no word of 33–64 and its hold sends \"I've "
        'written words 1–32"', (tester) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.confirmDeviceHalf()),
      );
      expect(find.text('Have you written words 1–32?'), findsOne);
      expect(find.text("I've written words 1–32"), findsOne);
      // No word of 33–64, and no Stop: to pause, unplug.
      expect(find.byType(CardWordRow), findsNothing);
      expect(_stopButton, findsNothing);
      // A tap isn't a hold.
      await tester.tap(find.byType(HoldToConfirmButton));
      await _settle(tester);
      expect(driver.sent, isEmpty);
      await _hold(
        tester,
        find.byType(HoldToConfirmButton),
        const Duration(milliseconds: 2200),
      );
      await _settle(tester);
      expect(driver.sent, ['device half written']);
      // The pages follow, with the device's proofs coming in under them.
      await _emit(tester, driver, _state(_page0, proofs: 40));
      expect(find.byType(CardWordRow), findsNWidgets(3));
      expect(find.text('Your device is checking your card.'), findsOne);
    });

    testWidgets('a page of three words, with Back off on the first page', (
      tester,
    ) async {
      final driver = await _pump(tester, _state(_page0));
      expect(find.text('Write words 33–64'), findsOne);
      expect(find.text('33.'), findsOne);
      expect(find.text('35.'), findsOne);
      expect(find.text('CLOCK'), findsOne);
      expect(_button(tester, 'Back').onPressed, isNull);
      await _tapButton(tester, 'Next');
      expect(driver.sent, ['next']);

      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.phoneWords(
            page: 10,
            firstWord: 63,
            words: ['UNVEIL', 'VOYAGE'],
          ),
        ),
      );
      expect(find.byType(CardWordRow), findsNWidgets(2));
      expect(find.text('64.'), findsOne);
      await _tapButton(tester, 'Back');
      expect(driver.sent, ['next', 'back']);
    });

    testWidgets('pages that come back while they fade out are drawn once', (
      tester,
    ) async {
      // Every word of 33–64 is in RAM at once, so Back, Next and Back again
      // can all land inside the pages' cross-fade, and a page can be fading
      // out twice over.
      const page1 = CardFlowStage.phoneWords(
        page: 1,
        firstWord: 36,
        words: ['ORBIT', 'TRIBE', 'VALVE'],
      );
      final driver = await _pump(tester, _state(page1));
      for (final page in [_page0, page1, _page0]) {
        driver.controller.add(_state(page));
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 40));
        expect(tester.takeException(), isNull);
      }
      await _settle(tester);
      expect(find.text('33.'), findsOne);
      expect(find.text('36.'), findsNothing);
    });

    testWidgets("the device's proofs fill a bar under the pages and the quiz", (
      tester,
    ) async {
      final driver = await _pump(tester, _state(_page0, proofs: 0));
      LinearProgressIndicator bar() => tester.widget<LinearProgressIndicator>(
        find.byType(LinearProgressIndicator),
      );
      expect(find.text('Your device is checking your card.'), findsOne);
      expect(bar().value, 0);
      await _emit(tester, driver, _state(_page0, proofs: fakeSealSize ~/ 2));
      expect(bar().value, closeTo(0.5, 0.01));
      expect(find.byType(CardWordRow), findsNWidgets(3));
      // It stays under the quiz, and the page needs nothing from it.
      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.selfCheck(
            k: 0,
            total: 32,
            wordNumber: 33,
            options: ['SIZE', 'SIDE', 'SISTER'],
            wrong: false,
            recheck: false,
          ),
          proofs: fakeSealSize,
        ),
      );
      expect(find.text('Your device has checked your card.'), findsOne);
      expect(bar().value, 1);
      // Never under a later check's quiz.
      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.selfCheck(
            k: 0,
            total: 37,
            wordNumber: 33,
            options: ['SIZE', 'SIDE', 'SISTER'],
            wrong: false,
            recheck: true,
          ),
          halves: true,
        ),
      );
      expect(find.byType(LinearProgressIndicator), findsNothing);
    });

    testWidgets('all the words, then the hold starts the quiz', (tester) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.phoneAllWords(words: fakePhoneHalf)),
      );
      expect(find.text('All words 33–64'), findsOne);
      expect(find.byType(CardWordRow), findsNWidgets(32));
      // 33–48 down the left, 49–64 down the right.
      expect(
        tester.getTopLeft(find.text('49.')).dx,
        greaterThan(tester.getTopLeft(find.text('48.')).dx),
      );
      await _tapButton(tester, 'Next');
      await _emit(tester, driver, _state(const CardFlowStage.phoneConfirm()));
      expect(find.text("Check you've written"), findsOne);
      await _hold(
        tester,
        find.byType(HoldToConfirmButton),
        const Duration(milliseconds: 2200),
      );
      await _settle(tester);
      expect(driver.sent, ['next', 'written']);
    });

    testWidgets('an unplug on a page of words keeps the page, with a banner', (
      tester,
    ) async {
      final driver = await _pump(tester, _state(_page0));
      await _emit(tester, driver, _state(_page0, connected: false));
      expect(find.byType(CardWordRow), findsNWidgets(3));
      expect(find.textContaining('Your device is unplugged.'), findsOne);
      await _tapButton(tester, 'Next');
      expect(driver.sent, ['next']);
      // The device's proofs wait for it.
      expect(
        find.text('Your device checks your card once it is plugged back in.'),
        findsOne,
      );
    });
  });

  // Owner B15: an install that didn't create the wallet writes the card,
  // with both quizzes, and never verifies it.
  group('an unverified card', () {
    testWidgets('the intro says this phone can\'t verify it, and writes it '
        'only on the user\'s choice', (tester) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.unverifiedIntro(), unverified: true),
      );
      expect(find.text(unverifiedTitle), findsOne);
      expect(find.text(unverifiedLine), findsOne);
      expect(find.text(unverifiedVerifyLine), findsOne);
      expect(find.text(unverifiedKeygenPhoneLine), findsOne);
      // Review B15 protocol F1: it says what writing here costs on the phone
      // that created the wallet.
      expect(unverifiedKeygenPhoneLine, contains('receive addresses'));
      expect(find.byType(CardWordRow), findsNothing);
      expect(driver.sent, isEmpty);
      await _tapButton(tester, unverifiedWriteButton);
      expect(driver.sent, ['write unverified']);
    });

    testWidgets('every writing page carries the banner, and no proofs bar', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.deviceHalf(), unverified: true),
      );
      expect(find.text(unverifiedBanner), findsOne);
      expect(find.text('Write words 1–32 from your device'), findsOne);
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.confirmDeviceHalf(), unverified: true),
      );
      expect(find.text(unverifiedBanner), findsOne);
      expect(
        find.textContaining('your device checks your card'),
        findsNothing,
        reason: 'nothing checks an unverified card',
      );
      expect(
        find.text(
          "Once you confirm, your device won't show words 1–32 again. This "
          'phone then shows words 33–64.',
        ),
        findsOne,
      );
      await _emit(tester, driver, _state(_page0, unverified: true));
      expect(find.text(unverifiedBanner), findsOne);
      expect(find.byType(CardWordRow), findsNWidgets(3));
      expect(find.byType(ProofsProgress), findsNothing);
      expect(find.text('Your device is checking your card.'), findsNothing);
    });

    testWidgets('written, not verified: never "Backup checked", and both '
        'halves are never marked recorded', (tester) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.unverifiedWritten(), unverified: true),
      );
      expect(find.text(unverifiedWrittenTitle), findsOne);
      expect(find.text(unverifiedWrittenLine), findsOne);
      expect(find.text(unverifiedVerifyLine), findsOne);
      expect(find.text('Backup checked'), findsNothing);
      expect(driver.backupMarks, 0);
      // An end page: it stands with the device unplugged.
      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.unverifiedWritten(),
          unverified: true,
          connected: false,
        ),
      );
      expect(find.text(unverifiedWrittenTitle), findsOne);
      expect(find.text('Plug in your device to continue.'), findsNothing);
    });
  });

  group('the quizzes', () {
    testWidgets('a self-check pick goes out as its word', (tester) async {
      final driver = await _pump(
        tester,
        _state(
          const CardFlowStage.selfCheck(
            k: 3,
            total: 32,
            wordNumber: 36,
            options: ['VALVE', 'VALLEY', 'VALID'],
            wrong: false,
            recheck: false,
          ),
        ),
      );
      expect(find.text('Select word 36'), findsOne);
      expect(find.bySemanticsLabel('3 of 32 words checked'), findsOne);
      await tester.tap(find.byType(CardWord).first);
      await _settle(tester);
      expect(driver.sent, ['pick VALVE']);
    });

    testWidgets('both halves recorded marks the backup once; the verdict '
        'never does', (tester) async {
      final driver = await _pump(tester, _state(const CardFlowStage.passed()));
      expect(driver.backupMarks, 0);
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.readyForFinalCheck(), halves: true),
      );
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.deviceRound(), halves: true),
      );
      expect(driver.backupMarks, 1);
    });

    testWidgets('the checked card, then Continue starts the challenges', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(
          const CardFlowStage.readyForFinalCheck(),
          halves: true,
          proofs: fakeSealSize,
        ),
      );
      expect(find.text('Success! Words 33–64 checked'), findsOne);
      expect(find.byType(LinearProgressIndicator), findsNothing);
      await _tapButton(tester, 'Continue');
      expect(driver.sent, ['final']);
    });

    testWidgets('Continue stays off until every proof is verified', (
      tester,
    ) async {
      // Both halves recorded, the proofs still coming: the checked card, with
      // the bar and Continue off.
      final driver = await _pump(
        tester,
        _state(
          const CardFlowStage.proving(),
          halves: true,
          proofs: fakeSealSize - 11,
        ),
      );
      expect(find.text('Success! Words 33–64 checked'), findsOne);
      expect(find.text('Your device is checking your card.'), findsOne);
      expect(_button(tester, 'Continue').onPressed, isNull);
      await tester.tap(find.text('Continue'));
      await _settle(tester);

      // Even a ready page whose proofs aren't all in keeps it off.
      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.readyForFinalCheck(),
          halves: true,
          proofs: fakeSealSize - 1,
        ),
      );
      expect(_button(tester, 'Continue').onPressed, isNull);
      // And one whose phone half isn't recorded.
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.readyForFinalCheck(), proofs: fakeSealSize),
      );
      expect(_button(tester, 'Continue').onPressed, isNull);
      expect(driver.sent, isEmpty);

      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.readyForFinalCheck(),
          halves: true,
          proofs: fakeSealSize,
        ),
      );
      expect(_button(tester, 'Continue').onPressed, isNotNull);
      await _tapButton(tester, 'Continue');
      expect(driver.sent, ['final']);
    });

    testWidgets('proofs with no page of words up: checking your card', (
      tester,
    ) async {
      await _pump(tester, _state(const CardFlowStage.proving(), proofs: 100));
      expect(find.text('Checking your card'), findsOne);
      expect(find.text('Success! Words 33–64 checked'), findsNothing);
      expect(find.text('Continue'), findsNothing);
    });

    testWidgets('a phone-round question: the word or None of these', (
      tester,
    ) async {
      final driver = await _pump(tester, _state(_question));
      expect(find.text('Select word 7'), findsOne);
      expect(find.text(roundLine), findsOne);
      expect(find.byType(CardWord), findsNWidgets(3));
      expect(find.text(noneOfTheseLabel), findsOne);
      await tester.tap(find.text(noneOfTheseLabel));
      await _settle(tester);
      expect(driver.sent, ['answer none']);

      // The miss gets one second look, with a fresh pick.
      await _emit(tester, driver, _state(const CardFlowStage.tryAgain(k: 0)));
      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.question(
            k: 0,
            wordNumber: 7,
            options: ['SISTER', 'SIZE', 'SIDE'],
            secondLook: true,
          ),
        ),
      );
      expect(find.text('Look again at word 7 on your card.'), findsOne);
      await tester.tap(_option('SIZE'));
      await _settle(tester);
      expect(driver.sent, ['answer none', 'answer SIZE']);
    });

    testWidgets('a later check: the quiz, then both checks done', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.recheckWaiting()),
      );
      expect(find.text('Getting your words from your device…'), findsOne);
      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.selfCheck(
            k: 33,
            total: 37,
            wordNumber: 7,
            options: ['SIZE', 'SIDE', 'SISTER'],
            wrong: false,
            recheck: true,
          ),
        ),
      );
      expect(find.bySemanticsLabel('33 of 37 words checked'), findsOne);
      await _emit(tester, driver, _state(const CardFlowStage.recheckPassed()));
      expect(find.text('Finish the quiz on your device.'), findsOne);
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.recheckPassed(), deviceDone: true),
      );
      expect(find.text('Both checks done.'), findsOne);
      expect(driver.backupMarks, 0);
    });

    testWidgets('a phone with no record of words 33–64 says so', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.recheckUnavailable()),
      );
      expect(
        find.text(
          'Words 33–64 can be checked only with the phone that made this '
          'card.',
        ),
        findsOne,
      );
      // Its device's quiz ends: still nothing says the backup is checked
      // (audit F1).
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.recheckUnavailable(), deviceDone: true),
      );
      expect(find.text("Your device's check is done."), findsOne);
      expect(find.text("Your device's half is checked"), findsOne);
      expect(find.text('Your device is checking its half'), findsNothing);
      expect(find.text('Backup checked'), findsNothing);
      expect(find.text('Both checks done.'), findsNothing);
    });
  });

  group('the final check', () {
    // Owner B12: N goes out with the confirm of words 1–32, and from then on
    // no page offers Stop, with or without the device.
    testWidgets('after N no page offers Stop', (tester) async {
      final driver = await _pump(
        tester,
        _state(_page0, started: true, proofs: 40),
      );
      for (final (stage, connected) in [
        (_page0, true),
        (_page0, false),
        (const CardFlowStage.phoneConfirm(), true),
        (const CardFlowStage.proving(), true),
        (const CardFlowStage.readyForFinalCheck(), true),
        (const CardFlowStage.deviceRound(), true),
        (const CardFlowStage.preparingBox(k: 1), true),
        (_question, true),
        (const CardFlowStage.tryAgain(k: 0), true),
        (const CardFlowStage.roundSent(), true),
        (const CardFlowStage.starting(), false),
        (const CardFlowStage.wrongFirmware(), true),
        (
          const CardFlowStage.checkInterrupted(
            disconnected: true,
            resumesLeft: 3,
          ),
          false,
        ),
      ]) {
        await _emit(
          tester,
          driver,
          _state(stage, started: true, connected: connected, halves: true),
        );
        expect(_stopButton, findsNothing, reason: '$stage');
      }
      expect(driver.sent, isEmpty);
    });

    testWidgets('the device round shows no word, and no Stop after N', (
      tester,
    ) async {
      await _pump(
        tester,
        _state(const CardFlowStage.deviceRound(), started: true),
      );
      expect(find.text('Answer on your device'), findsOne);
      expect(find.byType(CardWord), findsNothing);
      expect(_stopButton, findsNothing);
    });

    // Owner follow-up 5 to B12: the Stop before N, kept only for records of
    // pre-release builds, is gone too: a device round with no N offers none.
    testWidgets(
      'before N (an earlier build\'s record) there is no Stop either',
      (tester) async {
        final driver = await _pump(
          tester,
          _state(const CardFlowStage.deviceRound()),
        );
        expect(_stopButton, findsNothing);
        for (final stage in [
          const CardFlowStage.proving(),
          const CardFlowStage.readyForFinalCheck(),
          const CardFlowStage.preparingBox(k: 0),
          _question,
        ]) {
          await _emit(tester, driver, _state(stage));
          expect(_stopButton, findsNothing, reason: '$stage');
        }
        expect(driver.sent, isEmpty);
      },
    );
  });

  // Owner B9: a disconnect while the check runs asks whether the user caused
  // it; the check goes on only after "Yes, it was me".
  group('an interruption after N', () {
    testWidgets('the device disconnected: the warning, the question, and '
        '"Yes" goes to the driver', (tester) async {
      final driver = await _pump(tester, _state(_page0, started: true));
      driver.connected.add(false);
      await _emit(
        tester,
        driver,
        _state(
          const CardFlowStage.checkInterrupted(
            disconnected: true,
            resumesLeft: 3,
          ),
          connected: false,
          started: true,
        ),
      );
      // It stands with the device out: no plug-in page, no words, no Stop.
      expect(find.text(interruptedTitle), findsOne);
      expect(find.text(interruptedDisconnectedLine), findsOne);
      expect(find.text(interruptedSecurityLine), findsOne);
      expect(find.text(interruptedQuestion), findsOne);
      expect(find.text(interruptedFrozenLine), findsOne);
      expect(find.text(interruptedLastResumeLine), findsNothing);
      expect(find.text('Plug in your device to continue.'), findsNothing);
      expect(find.byType(CardWordRow), findsNothing);
      expect(_stopButton, findsNothing);
      await _tapButton(tester, interruptedYes);
      expect(driver.sent, ['it was me']);
      expect(driver.starts, 1, reason: 'the same flow goes on');
    });

    testWidgets('opened on a question left from before: the wording for a '
        'closed app, and "No" goes to the driver', (tester) async {
      final driver = await _pump(
        tester,
        _state(
          const CardFlowStage.checkInterrupted(
            disconnected: false,
            resumesLeft: 3,
          ),
          started: true,
        ),
      );
      expect(find.text(interruptedClosedLine), findsOne);
      expect(find.text(interruptedDisconnectedLine), findsNothing);
      // Owner follow-up 1 to B9: a tap on "No" answers nothing; it's a hold.
      await tester.tap(find.text(interruptedNo));
      await _settle(tester);
      expect(driver.sent, isEmpty);
      await _hold(tester, find.text(interruptedNo), const Duration(seconds: 3));
      await _settle(tester);
      expect(driver.sent, ["it wasn't me"]);
    });

    testWidgets('"No" whose verdict no flow heard: the suspect alarm shows', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(
          const CardFlowStage.checkInterrupted(
            disconnected: true,
            resumesLeft: 3,
          ),
          started: true,
        ),
      );
      driver.unheardAnswer = cardFlowState(
        const CardFlowStage.failed(
          alarm: CardAlarm(
            kind: CardAlarmKind.disconnectedAfterN,
            deviceSuspect: true,
            nextStep: NextStep.withoutThisDevice,
          ),
        ),
        recordChanged: true,
      );
      await _hold(tester, find.text(interruptedNo), const Duration(seconds: 3));
      await _settle(tester);
      expect(
        find.text(alarmCause(CardAlarmKind.disconnectedAfterN)!),
        findsOne,
      );
      expect(find.text(nextStepLine(NextStep.withoutThisDevice)), findsOne);
      expect(driver.recordChanges, 1, reason: 'the checklist picks it up');
    });

    // Review B15 protocol F2: the question whose "Yes" is the last resume
    // says so before the user answers.
    testWidgets('the last resume: the page warns before "Yes"', (tester) async {
      final driver = await _pump(
        tester,
        _state(
          const CardFlowStage.checkInterrupted(
            disconnected: true,
            resumesLeft: 1,
          ),
          started: true,
        ),
      );
      expect(find.text(interruptedLastResumeLine), findsOne);
      await _tapButton(tester, interruptedYes);
      expect(driver.sent, ['it was me']);
    });

    // Owner R2-2: past 3 resumes, "Yes" ends the check too, and the page says
    // so before the user answers.
    testWidgets('a fourth "Yes": the interrupted-too-often alarm shows', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(
          const CardFlowStage.checkInterrupted(
            disconnected: true,
            resumesLeft: 0,
          ),
          started: true,
        ),
      );
      driver.unheardAnswer = cardFlowState(
        const CardFlowStage.failed(
          alarm: CardAlarm(
            kind: CardAlarmKind.interruptedTooOften,
            deviceSuspect: true,
            nextStep: NextStep.withoutThisDevice,
          ),
        ),
        recordChanged: true,
      );
      expect(find.text(interruptedTooOftenLine), findsOne);
      expect(find.text(interruptedLastResumeLine), findsNothing);
      await _tapButton(tester, interruptedYes);
      await _settle(tester);
      expect(driver.sent, ['it was me']);
      expect(
        find.text(alarmCause(CardAlarmKind.interruptedTooOften)!),
        findsOne,
      );
      expect(find.text(nextStepLine(NextStep.withoutThisDevice)), findsOne);
      expect(driver.recordChanges, 1, reason: 'the checklist picks it up');
    });
  });

  group('pause', () {
    testWidgets('an unplug ends the page; Continue waits for the device', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.deviceHalf()),
      );
      driver.connected.add(false);
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.paused(), connected: false),
      );
      expect(
        find.text('Plug your device back in, then press Continue.'),
        findsOne,
      );
      final continueButton = find.widgetWithText(FilledButton, 'Continue');
      expect(tester.widget<FilledButton>(continueButton).onPressed, isNull);
      expect(driver.starts, 1);

      driver.connected.add(true);
      await tester.pumpAndSettle();
      await tester.tap(continueButton);
      await tester.pump();
      expect(driver.starts, 2, reason: 'a new flow');

      // The new flow carries on where the device's record is.
      await _emit(tester, driver, _state(_page0));
      expect(find.byType(CardWordRow), findsNWidgets(3));
      expect(find.text('Continue'), findsNothing);
    });

    testWidgets('Continue waits out a device list that lags the unplug', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.deviceHalf()),
      );
      // The list still says the device is in.
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.paused(), connected: false),
      );
      final continueButton = find.widgetWithText(FilledButton, 'Continue');
      expect(tester.widget<FilledButton>(continueButton).onPressed, isNull);
      driver.connected.add(false);
      await _settle(tester);
      expect(tester.widget<FilledButton>(continueButton).onPressed, isNull);
      driver.connected.add(true);
      await _settle(tester);
      expect(tester.widget<FilledButton>(continueButton).onPressed, isNotNull);
    });

    testWidgets('a replug the list saw before the Paused state counts', (
      tester,
    ) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.deviceHalf()),
      );
      driver.connected.add(false);
      driver.connected.add(true);
      await _settle(tester);
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.paused(), connected: false),
      );
      final continueButton = find.widgetWithText(FilledButton, 'Continue');
      expect(tester.widget<FilledButton>(continueButton).onPressed, isNotNull);
      expect(
        find.text('Plug your device back in, then press Continue.'),
        findsNothing,
      );
    });

    testWidgets('a cancel from elsewhere ends the page with Continue live', (
      tester,
    ) async {
      final driver = await _pump(tester, _state(_question));
      await _emit(tester, driver, _state(const CardFlowStage.paused()));
      expect(find.byType(CardWord), findsNothing);
      expect(find.textContaining('The backup was interrupted.'), findsOne);
      final continueButton = find.widgetWithText(FilledButton, 'Continue');
      expect(tester.widget<FilledButton>(continueButton).onPressed, isNotNull);
      await tester.tap(continueButton);
      await tester.pump();
      expect(driver.starts, 2);
      expect(driver.sent, isEmpty);
    });
  });
  group('alarms', () {
    for (final (kind, step, cause) in [
      (
        CardAlarmKind.deviceRoundFailed,
        NextStep.differentPhone,
        "Your device's check of words 33–64 failed: this phone may have "
            'shown you wrong words.',
      ),
      (
        CardAlarmKind.stoppedBeforeN,
        NextStep.setUpAgain,
        "This card is not checked. Don't use this wallet.",
      ),
      (
        CardAlarmKind.phoneRoundFailed,
        NextStep.withoutThisDevice,
        "The words 1–32 your device showed didn't match what it proved.",
      ),
      (CardAlarmKind.stoppedAfterN, NextStep.withoutThisDevice, null),
      (CardAlarmKind.endedAfterN, NextStep.withoutThisDevice, null),
      (
        CardAlarmKind.disconnectedAfterN,
        NextStep.withoutThisDevice,
        "This device disconnected during the check, and you didn't disconnect "
            "it. Don't trust it.",
      ),
      (
        CardAlarmKind.interruptedTooOften,
        NextStep.withoutThisDevice,
        "This check was interrupted too many times. Don't trust this device.",
      ),
      (
        CardAlarmKind.deviceFault,
        NextStep.withoutThisDevice,
        "Your device sent this phone words or proofs that didn't match its "
            'seal.',
      ),
    ]) {
      testWidgets('${kind.name}: its words, H and the help line', (
        tester,
      ) async {
        await _pump(
          tester,
          _state(
            CardFlowStage.failed(
              alarm: CardAlarm(
                kind: kind,
                deviceSuspect: false,
                nextStep: step,
              ),
            ),
          ),
        );
        expect(
          find.text(
            kind == CardAlarmKind.stoppedBeforeN
                ? 'Check stopped.'
                : "Stop. Don't use this wallet.",
          ),
          findsOne,
        );
        if (cause != null) expect(find.text(cause), findsOne);
        expect(find.text(nextStepLine(step)), findsOne);
        expect(find.text(lineH), findsOne);
        expect(find.text(helpLine), findsOne);
        expect(_stopButton, findsNothing);
      });
    }

    testWidgets('a check on another phone, or a PASSED this phone didn\'t '
        'finish: neutral, and nothing says checked (PK-O7)', (tester) async {
      final driver = await _pump(
        tester,
        _state(const CardFlowStage.startedOnAnotherPhone()),
      );
      expect(find.text(anotherPhoneLine), findsOne);
      expect(
        anotherPhoneLine,
        "This device says this card's backup was started, or checked, on "
        'another phone. A card is checked only on the phone that started it. '
        "This phone hasn't checked it.",
      );
      expect(find.textContaining('Finish it there'), findsNothing);
      expect(find.text('Backup checked'), findsNothing);
      expect(_stopButton, findsNothing);
      expect(driver.backupMarks, 0);
      // An end page: it stands with the device unplugged.
      await _emit(
        tester,
        driver,
        _state(const CardFlowStage.startedOnAnotherPhone(), connected: false),
      );
      expect(find.text(anotherPhoneLine), findsOne);
    });

    // Owner B10: a changed seal is a suspect device, with the suspect alarm
    // and no dead-end "start again on a new blank card".
    testWidgets('a changed seal: the suspect alarm, without this device', (
      tester,
    ) async {
      const alarm = CardFlowStage.failed(
        alarm: CardAlarm(
          kind: CardAlarmKind.sealChanged,
          deviceSuspect: true,
          nextStep: NextStep.withoutThisDevice,
        ),
      );
      final driver = await _pump(tester, _state(alarm));
      const cause =
          "This device showed a different backup partway through. Don't "
          'trust it.';
      const next = 'Set this wallet up again without this device.';
      expect(find.text(cause), findsOne);
      expect(find.text(next), findsOne);
      expect(find.textContaining('new blank card'), findsNothing);
      expect(find.byType(CardWordRow), findsNothing);
      expect(_stopButton, findsNothing);
      // An end page: it stands with the device unplugged.
      await _emit(tester, driver, _state(alarm, connected: false));
      expect(find.text(cause), findsOne);
      expect(find.text('Plug in your device to continue.'), findsNothing);
    });
  });
}
