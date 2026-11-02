import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/restoration/enter_card_view.dart';
import 'package:frostsnap/restoration/phone_half_entry.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:google_fonts/google_fonts.dart';

// The card restore's page (design §5.3 E1 to E6) against a fake restore:
// each CardRestoreView and its line, the half sent once entered, an unplug
// that keeps the words and sends them again after the device's
// acknowledgement, and a refusal that ends the page. The app shows nothing
// decoded: the device's report has the card's threshold and its warnings,
// and a card whose masks fail is "couldn't be read" (owner B7).

final _device = DeviceId(field0: U8Array33(Uint8List(33)..[0] = 7));

class _Driver implements CardRestoreDriver {
  var controller = StreamController<EnterPhysicalBackupState>();
  final connected = StreamController<bool>();
  final sent = <List<String?>>[];
  var starts = 0;

  @override
  Stream<EnterPhysicalBackupState> start() {
    starts += 1;
    if (starts > 1) controller = StreamController();
    return controller.stream;
  }

  @override
  Future<void> sendPhoneHalf(List<String?> words) async => sent.add(words);

  @override
  Stream<bool> deviceConnected() => connected.stream;

  void view(CardRestoreView view) => controller.add(
    EnterPhysicalBackupState(
      deviceId: _device,
      saved: false,
      cardRestore: view,
    ),
  );

  void abort(String why) => controller.add(
    EnterPhysicalBackupState(deviceId: _device, saved: false, abort: why),
  );
}

final _lookup = WordLookup(isWord: (_) => true);

// The four steps of a failed read (owner B14), with line H under the two
// that send the user outside this app (failflow-review M3).
const _lineH =
    'No one from Frostsnap will ask for your words, what your device shows, '
    'or to see your screen.';
const _helpLine = 'Get help only from the links in Settings > About.';
const _checkWords =
    "Couldn't read this card. Check the words you entered: words 1–32 on "
    'your device and words 33–64 here. Then try again.';
const _otherApp =
    "Still couldn't read this card. Try the Frostsnap app on a different "
    'device, such as another phone or your laptop. Get it only from an '
    'official source.';
const _otherDevice =
    "Still couldn't read this card. It could be the Frostsnap device. Try a "
    'different Frostsnap device, and contact Frostsnap support.'
    '\n\n$_lineH $_helpLine';
const _offline =
    "This card still can't be read. Use offline recovery: it restores your "
    'key from all 64 words on one computer you trust, with no phone. Use only '
    "Frostsnap's own offline recovery tool, card_offline_recover.\n\n"
    "If that tool can't recover it either, this card can't be recovered. It "
    'may have been written wrong. You can still restore the wallet if you have '
    'enough other backups. Treat the Frostsnap device that wrote it as '
    'suspect.\n\n$_lineH';
const _triedOtherApp = "I've already tried it in a different app.";
const _noOtherDevice = "I don't have another Frostsnap device";

Future<_Driver> _pump(
  WidgetTester tester, {
  void Function(String)? onError,
  RestoreFailures? failures,
  Object? failureKey,
  Object failureDevice = defaultRestoreDevice,
}) async {
  tester.view.physicalSize = const Size(412, 1400) * 2;
  tester.view.devicePixelRatio = 2;
  addTearDown(tester.view.reset);
  final driver = _Driver();
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: EnterCardView(
          deviceName: 'Alpha',
          driver: driver,
          lookup: _lookup,
          onFinished: (_) {},
          onCancel: () {},
          onError: onError ?? (_) {},
          failures: failures ?? RestoreFailures(),
          failureKey: failureKey,
          failureDevice: failureDevice,
        ),
      ),
    ),
  );
  driver.connected.add(true);
  await tester.pump();
  return driver;
}

/// Enters all 32 as ZOO, then sends them.
Future<void> _enterAndSend(WidgetTester tester) async {
  for (var i = 0; i < phoneHalfWords; i++) {
    final field = find.byKey(ValueKey('word-${phoneHalfFirst + i}'));
    await tester.ensureVisible(field);
    await tester.enterText(field, 'zoo');
    await tester.pump();
  }
  await tester.ensureVisible(find.byKey(const ValueKey('send-half')));
  await tester.tap(find.byKey(const ValueKey('send-half')));
  await tester.pump();
}

void main() {
  setUpAll(() => GoogleFonts.config.allowRuntimeFetching = false);

  testWidgets('the steps, and never words 1–32 here', (tester) async {
    await _pump(tester);
    expect(find.text('1. Enter words 1–32 on your device Alpha.'), findsOne);
    expect(find.text('2. Enter words 33–64 here.'), findsOne);
    expect(find.text('Never type words 1–32 into this phone.'), findsOne);
    // The entry is open at once, from word 33.
    expect(find.byKey(const ValueKey('word-33')), findsOne);
  });

  for (final (view, line) in [
    (const CardRestoreView.waitingForDevice(), 'Connecting to your device…'),
    (
      const CardRestoreView.enterHalves(deviceHalfIn: true),
      'Words 1–32 are in on your device.',
    ),
    (
      const CardRestoreView.waitingForDeviceHalf(),
      'Sent. Now finish words 1–32 on your device.',
    ),
    (
      const CardRestoreView.checking(),
      'Checking your card on the device… (about 10 seconds)',
    ),
    (
      const CardRestoreView.finishOnDevice(),
      'Your card was read. Finish on your device: read any warnings it shows, '
          'then hold to restore.',
    ),
  ]) {
    testWidgets('${view.runtimeType}: its line', (tester) async {
      final driver = await _pump(tester);
      driver.view(view);
      await tester.pump();
      await tester.pump();
      expect(find.text(line), findsOne);
      expect(restoreStatusLine(view), line);
    });
  }

  testWidgets('no line while both halves are still to come', (tester) async {
    final driver = await _pump(tester);
    driver.view(const CardRestoreView.enterHalves(deviceHalfIn: false));
    await tester.pump();
    expect(
      restoreStatusLine(const CardRestoreView.enterHalves(deviceHalfIn: false)),
      isNull,
    );
    expect(find.textContaining('on your device.'), findsOne, reason: 'step 1');
  });

  testWidgets('the half goes out once sent; a failed read reopens it', (
    tester,
  ) async {
    final driver = await _pump(tester);
    driver.view(const CardRestoreView.enterHalves(deviceHalfIn: false));
    await tester.pump();
    await _enterAndSend(tester);
    expect(driver.sent, hasLength(1));
    expect(driver.sent.single, everyElement('ZOO'));
    expect(find.byKey(const ValueKey('word-33')), findsNothing);

    driver.view(const CardRestoreView.couldNotRead());
    await tester.pump();
    expect(find.text(_checkWords), findsOne);
    await tester.tap(find.byKey(const ValueKey('edit-half')));
    await tester.pump();
    // The entry, with the words kept; sent again as they are.
    expect(find.text('ZOO'), findsNWidgets(phoneHalfWords));
    await tester.ensureVisible(find.byKey(const ValueKey('send-half')));
    await tester.tap(find.byKey(const ValueKey('send-half')));
    await tester.pump();
    expect(driver.sent, hasLength(2));
  });

  // Owner follow-up 2 to B14: the count and both answers are kept per app
  // install, so a restart doesn't go back to step 1.
  test('the failure count and the answers survive an app restart', () async {
    final dir = await Directory.systemTemp.createTemp('restore-failures');
    addTearDown(() => dir.delete(recursive: true));
    final file = File('${dir.path}/restore_failures.json');
    final first = RestoreFailures();
    await first.loadFrom(file);
    first.failed('wallet:aa', device: 'A');
    first.failed('wallet:aa', device: 'A');
    first.triedOtherApp('wallet:aa');
    first.failed('new wallet', device: 'B');
    expect(first.step('wallet:aa'), RestoreFailureStep.tryOtherDevice);

    final restarted = RestoreFailures();
    await restarted.loadFrom(file);
    expect(restarted.count('wallet:aa'), 2);
    expect(restarted.devices('wallet:aa'), 1);
    expect(restarted.step('wallet:aa'), RestoreFailureStep.tryOtherDevice);
    expect(restarted.count('new wallet'), 1);
    restarted.noOtherDevice('wallet:aa');
    expect(restarted.step('wallet:aa'), RestoreFailureStep.offlineRecovery);
    // A card that reads clears it, on disk too.
    restarted.read('new wallet');
    final again = RestoreFailures();
    await again.loadFrom(file);
    expect(again.step('wallet:aa'), RestoreFailureStep.offlineRecovery);
    expect(again.count('new wallet'), 0);
    // An unreadable file is a fresh start.
    await file.writeAsString('not json');
    final fresh = RestoreFailures();
    await fresh.loadFrom(file);
    expect(fresh.count('wallet:aa'), 0);
  });

  group('a failed read escalates by the app\'s own count (owner, 2 Nov)', () {
    /// One read on the device that fails: the device decodes, then the
    /// phone hears the same `CouldNotRead` as every other failure.
    Future<void> failOnce(WidgetTester tester, _Driver driver) async {
      driver.view(const CardRestoreView.checking());
      await tester.pump();
      driver.view(const CardRestoreView.couldNotRead());
      await tester.pump();
    }

    Future<void> tapStep(WidgetTester tester, String label) async {
      await tester.ensureVisible(find.text(label));
      await tester.tap(find.text(label));
      await tester.pump();
    }

    testWidgets('check the words, then another app, staying there until the '
        'user says it failed too; then another device, then offline recovery '
        'once another device failed too', (tester) async {
      final failures = RestoreFailures();
      var driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'w',
        failureDevice: 'A',
      );
      await failOnce(tester, driver);
      expect(find.text(_checkWords), findsOne);
      expect(find.text(_otherApp), findsNothing);
      expect(find.text(_triedOtherApp), findsNothing, reason: 'no button yet');
      // The device's state again with nothing decoded since: not a new
      // failure.
      driver.view(const CardRestoreView.couldNotRead());
      await tester.pump();
      expect(find.text(_checkWords), findsOne);
      expect(failures.count('w'), 1);

      await failOnce(tester, driver);
      expect(find.text(_otherApp), findsOne);
      expect(find.text(_triedOtherApp), findsOne);
      expect(find.text(_checkWords), findsNothing);
      expect(find.textContaining(_lineH), findsNothing);
      // The entry stays: the user can still fix a word.
      expect(find.byKey(const ValueKey('phone-half-entry')), findsOne);

      // More failures in this app, on this device or another, stay at step
      // 2: only the user's answer moves on (owner B14). A restoring device
      // can decode and fail on its own, with no send from here.
      for (var i = 0; i < 5; i++) {
        await failOnce(tester, driver);
      }
      await tester.pumpWidget(const SizedBox());
      driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'w',
        failureDevice: 'B',
      );
      await failOnce(tester, driver);
      expect(find.text(_otherApp), findsOne);
      expect(find.text(_offline), findsNothing);
      expect(failures.count('w'), 8);
      expect(failures.devices('w'), 2);

      // The other app failed too: it could be the Frostsnap device. Two
      // devices failed already, so straight on to offline recovery.
      await tapStep(tester, _triedOtherApp);
      expect(find.text(_offline), findsOne);
      expect(find.byKey(const ValueKey('phone-half-entry')), findsOne);
      await failOnce(tester, driver);
      expect(find.text(_offline), findsOne, reason: 'it stays there');
      expect(find.text(_noOtherDevice), findsNothing);
    });

    testWidgets('another app failed too: try another device; offline recovery '
        'once a second device fails, never on the first alone', (tester) async {
      final failures = RestoreFailures();
      var driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'w',
        failureDevice: 'A',
      );
      await failOnce(tester, driver);
      await failOnce(tester, driver);
      await tapStep(tester, _triedOtherApp);
      expect(find.text(_otherDevice), findsOne);
      expect(find.text(_triedOtherApp), findsNothing);
      expect(find.text(_noOtherDevice), findsOne);
      // More failures on the same device (failflow-review M2).
      for (var i = 0; i < 5; i++) {
        await failOnce(tester, driver);
      }
      expect(find.text(_otherDevice), findsOne);
      expect(find.text(_offline), findsNothing);

      // A different Frostsnap device fails too: offline recovery.
      await tester.pumpWidget(const SizedBox());
      driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'w',
        failureDevice: 'B',
      );
      expect(failures.step('w'), RestoreFailureStep.tryOtherDevice);
      await failOnce(tester, driver);
      expect(find.text(_offline), findsOne);
      expect(find.text(_otherDevice), findsNothing);
    });

    testWidgets('no other Frostsnap device: offline recovery', (tester) async {
      final failures = RestoreFailures();
      final driver = await _pump(tester, failures: failures, failureKey: 'w');
      await failOnce(tester, driver);
      await failOnce(tester, driver);
      await tapStep(tester, _triedOtherApp);
      await tapStep(tester, _noOtherDevice);
      expect(find.text(_offline), findsOne);
      expect(find.text(_noOtherDevice), findsNothing);
      expect(failures.devices('w'), 1);
      await failOnce(tester, driver);
      expect(find.text(_offline), findsOne, reason: 'it stays there');
    });

    testWidgets('the count is per restore: another device on this phone '
        'carries on from it, another restore starts again', (tester) async {
      final failures = RestoreFailures();
      var driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'w',
        failureDevice: 'A',
      );
      await failOnce(tester, driver);
      await failOnce(tester, driver);
      await tapStep(tester, _triedOtherApp);
      expect(find.text(_otherDevice), findsOne);

      // The user comes back with a different Frostsnap device: a new page
      // for the same restore.
      await tester.pumpWidget(const SizedBox());
      driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'w',
        failureDevice: 'B',
      );
      await failOnce(tester, driver);
      expect(find.text(_offline), findsOne);

      // Another restore on this phone has its own count, and its own
      // answers.
      await tester.pumpWidget(const SizedBox());
      driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'x',
        failureDevice: 'B',
      );
      await failOnce(tester, driver);
      expect(find.text(_checkWords), findsOne);
      await failOnce(tester, driver);
      expect(find.text(_otherApp), findsOne);
    });

    testWidgets('a card this device already holds read: the count clears', (
      tester,
    ) async {
      final failures = RestoreFailures();
      String? error;
      final driver = await _pump(
        tester,
        failures: failures,
        failureKey: 'w',
        onError: (e) => error = e,
      );
      await failOnce(tester, driver);
      await failOnce(tester, driver);
      await tapStep(tester, _triedOtherApp);
      expect(failures.count('w'), 2);
      driver.abort(alreadyHeldAbort);
      await tester.pump();
      expect(error, alreadyHeldAbort);
      expect(failures.count('w'), 0);
      expect(failures.step('w'), RestoreFailureStep.checkWords);
    });

    testWidgets('another refusal leaves the count', (tester) async {
      final failures = RestoreFailures();
      final driver = await _pump(tester, failures: failures, failureKey: 'w');
      await failOnce(tester, driver);
      driver.abort(
        'This card needs newer firmware. Update your device, then restore '
        'again.',
      );
      await tester.pump();
      expect(failures.count('w'), 1);
    });

    testWidgets('an unplug between reads keeps the count', (tester) async {
      final failures = RestoreFailures();
      final driver = await _pump(tester, failures: failures, failureKey: 'w');
      await failOnce(tester, driver);
      driver.connected.add(false);
      driver.abort('device was unplugged');
      await tester.pump();
      driver.connected.add(true);
      await tester.pump();
      driver.view(const CardRestoreView.waitingForDevice());
      await tester.pump();
      await failOnce(tester, driver);
      expect(find.text(_otherApp), findsOne);
    });

    // Owner B11: a genuine card for another wallet is its own page, never a
    // failed read: it neither counts nor clears the count.
    testWidgets('another wallet\'s card: its own page, outside the count', (
      tester,
    ) async {
      final failures = RestoreFailures();
      var closed = 0;
      final driver = _Driver();
      tester.view.physicalSize = const Size(412, 1400) * 2;
      tester.view.devicePixelRatio = 2;
      addTearDown(tester.view.reset);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: EnterCardView(
              driver: driver,
              lookup: _lookup,
              onFinished: (_) {},
              onCancel: () => closed += 1,
              onError: (_) {},
              failures: failures,
              failureKey: 'w',
            ),
          ),
        ),
      );
      driver.connected.add(true);
      await tester.pump();
      await failOnce(tester, driver);
      await failOnce(tester, driver);
      expect(failures.count('w'), 2);
      driver.view(const CardRestoreView.checking());
      await tester.pump();
      driver.view(const CardRestoreView.wrongWallet());
      await tester.pump();
      expect(
        find.text(
          "This card doesn't match this wallet. It may be another wallet's "
          'card, or it may have been written wrong.',
        ),
        findsOne,
      );
      expect(find.text("Doesn't match this wallet"), findsOne);
      expect(find.text("Restore with this wallet's card."), findsOne);
      expect(find.byKey(const ValueKey('phone-half-entry')), findsNothing);
      expect(find.text(_otherApp), findsNothing);
      expect(failures.count('w'), 2, reason: 'not counted, not cleared');
      await tester.tap(find.text('Close'));
      await tester.pump();
      expect(closed, 1);
      expect(
        restoreStatusLine(const CardRestoreView.wrongWallet()),
        wrongWalletLine,
      );
    });

    testWidgets('a card that reads clears the count', (tester) async {
      final failures = RestoreFailures();
      final driver = await _pump(tester, failures: failures, failureKey: 'w');
      await failOnce(tester, driver);
      await failOnce(tester, driver);
      await tapStep(tester, _triedOtherApp);
      await tapStep(tester, _noOtherDevice);
      driver.view(const CardRestoreView.checking());
      await tester.pump();
      driver.view(const CardRestoreView.finishOnDevice());
      await tester.pump();
      expect(failures.count('w'), 0);
      expect(failures.step('w'), RestoreFailureStep.checkWords);
      await failOnce(tester, driver);
      expect(find.text(_checkWords), findsOne);
    });

    test('the steps: by count, then by the user\'s answers and devices', () {
      expect(restoreFailureStep(1), RestoreFailureStep.checkWords);
      expect(restoreFailureStep(2), RestoreFailureStep.tryOtherApp);
      expect(restoreFailureStep(9), RestoreFailureStep.tryOtherApp);
      expect(
        restoreFailureStep(9, devices: 3),
        RestoreFailureStep.tryOtherApp,
        reason: 'more failures in this app stay at step 2',
      );
      expect(
        restoreFailureStep(9, devices: 3, noOtherDevice: true),
        RestoreFailureStep.tryOtherApp,
        reason: 'step 4 only through step 3',
      );
      expect(
        restoreFailureStep(2, triedOtherApp: true),
        RestoreFailureStep.tryOtherDevice,
      );
      expect(
        restoreFailureStep(9, triedOtherApp: true),
        RestoreFailureStep.tryOtherDevice,
      );
      expect(
        restoreFailureStep(2, devices: 2, triedOtherApp: true),
        RestoreFailureStep.offlineRecovery,
      );
      expect(
        restoreFailureStep(2, triedOtherApp: true, noOtherDevice: true),
        RestoreFailureStep.offlineRecovery,
      );
      const view = CardRestoreView.couldNotRead();
      expect(restoreStatusLine(view), _checkWords);
      for (final (step, line, button) in [
        (RestoreFailureStep.checkWords, _checkWords, null),
        (RestoreFailureStep.tryOtherApp, _otherApp, _triedOtherApp),
        (RestoreFailureStep.tryOtherDevice, _otherDevice, _noOtherDevice),
        (RestoreFailureStep.offlineRecovery, _offline, null),
      ]) {
        expect(restoreStatusLine(view, step: step), line);
        expect(restoreFailureButton(step), button);
        // Owner B11: no step hedges about another wallet's card.
        expect(line, isNot(contains('wallet\'s card')));
      }
    });

    test('the key: one per wallet and restoration, one for a new wallet', () {
      AccessStructureRef wallet(int b) => AccessStructureRef(
        keyId: KeyId(field0: U8Array32(Uint8List(32)..[0] = b)),
        accessStructureId: AccessStructureId(
          field0: U8Array32(Uint8List(32)..[0] = b),
        ),
      );
      RestorationId restoration(int b) =>
          RestorationId(field0: U8Array16(Uint8List(16)..[0] = b));
      Object key(RecoveryContext c) => restoreFailureKey(c);
      final w1 = AddingToWalletContext(accessStructureRef: wallet(1));
      expect(
        key(w1),
        key(AddingToWalletContext(accessStructureRef: wallet(1))),
      );
      expect(
        key(w1),
        isNot(key(AddingToWalletContext(accessStructureRef: wallet(2)))),
      );
      final r1 = ContinuingRestorationContext(restorationId: restoration(1));
      expect(
        key(r1),
        key(ContinuingRestorationContext(restorationId: restoration(1))),
      );
      expect(
        key(r1),
        isNot(key(ContinuingRestorationContext(restorationId: restoration(2)))),
      );
      expect(key(const NewRestorationContext()), 'new wallet');
      expect(key(w1), isNot(key(const NewRestorationContext())));
    });
  });

  testWidgets('an unplug keeps the words, and the device gets them again '
      'after it acknowledges', (tester) async {
    final driver = await _pump(tester);
    driver.view(const CardRestoreView.enterHalves(deviceHalfIn: false));
    await tester.pump();
    await _enterAndSend(tester);
    expect(driver.sent, hasLength(1));

    driver.connected.add(false);
    driver.abort('device was unplugged');
    await tester.pump();
    expect(find.textContaining('Your device was unplugged.'), findsOne);
    expect(driver.starts, 1);

    driver.connected.add(true);
    await tester.pump();
    expect(driver.starts, 2, reason: 'EnterCard again');
    expect(driver.sent, hasLength(1), reason: 'not before the restore is up');
    driver.view(const CardRestoreView.waitingForDevice());
    await tester.pump();
    expect(driver.sent, hasLength(2));
    expect(find.textContaining('Your device was unplugged.'), findsNothing);
  });

  testWidgets('a refusal ends the page with its reason', (tester) async {
    String? error;
    final driver = await _pump(tester, onError: (e) => error = e);
    driver.abort('Your device was reset. Start the restore again.');
    await tester.pump();
    expect(error, 'Your device was reset. Start the restore again.');
  });
}
