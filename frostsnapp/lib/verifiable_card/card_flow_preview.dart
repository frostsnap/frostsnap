import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration/enter_card_view.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/verifiable_card/card_flow.dart';

// DEBUG ONLY. A fake card flow and a fake card restore, for looking at every
// page without a device or the coordinator. Settings reaches them only
// behind `kDebugMode`, so release builds tree-shake them away. The fake
// card's words are made up here, which no real flow ever does: in the app
// words 33–64 come from the device and are checked against its pinned seal.

/// The fake seal's size: the real card's (`card_seal_size`, 311), so the
/// bars move as they do in the app. A test can pass another.
const previewSealSize = 311;

/// Words 33–64 of the fake card.
@visibleForTesting
const previewPhoneHalf = <String>[
  'SIZE', 'CLOCK', 'MARGIN', 'ORBIT', 'TRIBE', 'VALVE', 'HOLIDAY', 'LEMON', //
  'PUZZLE', 'ANCHOR', 'GRAVITY', 'BUNDLE', 'CANYON', 'DOLPHIN', 'EMBRACE', //
  'FABRIC', 'GLIMPSE', 'HARVEST', 'IMPULSE', 'JUNGLE', 'KITCHEN', 'LAPTOP', //
  'MEADOW', 'NEPHEW', 'OYSTER', 'PELICAN', 'QUANTUM', 'RIBBON', 'SALMON', //
  'TORNADO', 'UNVEIL', 'VOYAGE',
];

/// Each word's two closest lookalikes, as the quiz offers them.
const _previewLookalikes = <String, List<String>>{
  'SIZE': ['SIDE', 'SISTER'],
  'CLOCK': ['CLICK', 'CLOTH'],
  'MARGIN': ['MARINE', 'MARKET'],
  'ORBIT': ['ORANGE', 'ORDER'],
  'TRIBE': ['BRIDE', 'TRIP'],
};

List<String> _lookalikes(String word) =>
    _previewLookalikes[word] ?? const ['ABANDON', 'ZOO'];

/// The phone round's five words of 1–32, and their pools' picks.
@visibleForTesting
const previewRound = <({int wordNumber, String word})>[
  (wordNumber: 7, word: 'SIZE'),
  (wordNumber: 12, word: 'CLOCK'),
  (wordNumber: 3, word: 'MARGIN'),
  (wordNumber: 18, word: 'ORBIT'),
  (wordNumber: 31, word: 'TRIBE'),
];

/// An alarm as the coordinator gives it (`failed_stage`, fc card_backup.rs).
CardAlarm previewAlarm(CardAlarmKind kind) => CardAlarm(
  kind: kind,
  deviceSuspect: switch (kind) {
    CardAlarmKind.phoneRoundFailed ||
    CardAlarmKind.endedAfterN ||
    CardAlarmKind.stoppedAfterN ||
    CardAlarmKind.deviceFault ||
    CardAlarmKind.sealChanged ||
    CardAlarmKind.disconnectedAfterN ||
    CardAlarmKind.interruptedTooOften ||
    CardAlarmKind.deviceRoundFailedAfterN => true,
    _ => false,
  },
  nextStep: switch (kind) {
    CardAlarmKind.deviceRoundFailed => NextStep.differentPhone,
    CardAlarmKind.deviceRoundFailedAfterN => NextStep.differentPhoneAndDevice,
    CardAlarmKind.stoppedBeforeN => NextStep.setUpAgain,
    _ => NextStep.withoutThisDevice,
  },
);

enum CardFlowPreviewScenario {
  fullRun(
    'Full run',
    'Words 1–32 on the device, words 33–64 here, both quizzes, then both '
        'challenges',
  ),
  fullRunWithUnplug(
    'Full run with a disconnect',
    'The device is unplugged on the words, after N: "Was that you?" (owner '
        'B9); after "Yes" the pages keep working',
  ),
  interruptedOnOpen(
    'Question on opening',
    'The app was closed while the check ran: the same question first (owner '
        'B9)',
  ),
  recheck('Later check', 'A PASSED card: the quiz again, nothing recorded'),
  deviceRoundFails(
    'Device round fails',
    "From the checked card: the device's challenge fails",
  ),
  paused('Paused', 'The device was unplugged, then plugged back in'),
  stoppedBeforeN('Alarm: stopped before N', 'A Stop before the phone round'),
  phoneRoundFailed('Alarm: phone round failed', 'Two misses on one word'),
  disconnectedAfterN(
    'Alarm: disconnected, not by you',
    '"No, I didn\'t" after a disconnect: a suspect device (owner B9)',
  ),
  interruptedTooOften(
    'Alarm: interrupted too often',
    'A fourth "Yes, it was me": past the 3 resumes a check allows, a suspect '
        'device (owner R2-2)',
  ),
  endedAfterN('Alarm: ended after N', 'A stall or refusal after N'),
  deviceFault(
    'Alarm: device fault',
    "Words or proofs that didn't match its seal",
  ),
  differentCard('Different card', 'The device shows another seal'),
  sealChanged(
    'Alarm: seal changed',
    'A different backup partway through: a suspect device (owner B10)',
  ),
  anotherPhone('Started on another phone', 'This phone has no N'),
  unverified(
    'Unverified card',
    "This phone didn't create the wallet: it writes the card, with both "
        'quizzes, and never verifies it (owner B15)',
  ),
  storageError('Storage error', "The phone couldn't persist"),
  refused('Refused', 'The device refused the flow'),
  startError('Start error', 'The coordinator refused to start');

  final String title;
  final String subtitle;
  const CardFlowPreviewScenario(this.title, this.subtitle);

  /// The alarm a static alarm scenario shows.
  CardAlarmKind? get alarm => switch (this) {
    stoppedBeforeN => CardAlarmKind.stoppedBeforeN,
    phoneRoundFailed => CardAlarmKind.phoneRoundFailed,
    disconnectedAfterN => CardAlarmKind.disconnectedAfterN,
    interruptedTooOften => CardAlarmKind.interruptedTooOften,
    endedAfterN => CardAlarmKind.endedAfterN,
    deviceFault => CardAlarmKind.deviceFault,
    _ => null,
  };
}

/// A fake device and coordinator for the card flow. It plays their part the
/// way the real ones do (owner B1): the seal and its pin, words 1–32 on the
/// device, the confirm here, then words 33–64 and their quiz while the
/// device's proofs come in behind them, and the challenges once both halves
/// and every proof are in.
class PreviewCardFlowDriver implements CardFlowDriver {
  final CardFlowPreviewScenario scenario;

  /// How fast the fake device works: 1 is about the real pace divided by 20.
  final Duration tick;

  @override
  final int sealSize;

  /// How many proofs the fake device sends a tick: the real ~63 s of proofs
  /// run while the user writes words 33–64.
  final int proofsPerTick;

  var _controller = StreamController<CardFlowState>();
  final _connected = StreamController<bool>();
  bool _plugged = true;
  int _starts = 0;

  CardFlowState _state = const CardFlowState(
    stage: CardFlowStage.starting(),
    deviceConnected: true,
    pointsReceived: 0,
    proofsVerified: 0,
    checkStarted: false,
    boxesDone: 0,
    halvesRecorded: false,
    recheckDeviceDone: false,
    recordChanged: false,
    unverified: false,
  );

  /// Owner B15: this phone didn't create the wallet.
  bool get _unverified => scenario == CardFlowPreviewScenario.unverified;
  bool _left = false;

  /// The state last given, for tests.
  @visibleForTesting
  CardFlowState get state => _state;

  /// Bumped when a flow ends (an unplug), so the old flow's timers stop.
  int _generation = 0;

  int _proofs = 0;
  bool _proving = false;
  int _page = 0;
  bool _unpluggedOnce = false;
  int _quiz = 0;
  bool _recheck = false;
  bool _afterN = false;

  /// The page the interruption's question covered (owner B9).
  CardFlowStage? _covered;
  final _missed = List.filled(previewRound.length, false);

  /// What the page asked for, in order, for tests.
  @visibleForTesting
  final calls = <String>[];

  PreviewCardFlowDriver(
    this.scenario, {
    this.tick = const Duration(milliseconds: 100),
    this.sealSize = previewSealSize,
    this.proofsPerTick = 2,
  });

  @override
  Stream<CardFlowState> start() {
    _starts += 1;
    if (_starts > 1) {
      unawaited(_controller.close());
      _controller = StreamController<CardFlowState>();
      unawaited(_resume());
    } else {
      unawaited(_run());
    }
    return _controller.stream;
  }

  @override
  Stream<bool> deviceConnected() {
    _connected.add(_plugged);
    return _connected.stream;
  }

  void _plug(bool plugged) {
    _plugged = plugged;
    if (!_left) _connected.add(plugged);
  }

  void _emit({
    CardFlowStage? stage,
    bool? connected,
    int? points,
    int? proofs,
    int? boxes,
    bool? halves,
    bool? deviceDone,
    bool recordChanged = false,
  }) {
    if (_left) return;
    _state = CardFlowState(
      stage: stage ?? _state.stage,
      deviceConnected: connected ?? _state.deviceConnected,
      pointsReceived: points ?? _state.pointsReceived,
      proofsVerified: proofs ?? _state.proofsVerified,
      checkStarted: _afterN && !_ended(stage ?? _state.stage),
      boxesDone: boxes ?? _state.boxesDone,
      halvesRecorded: halves ?? _state.halvesRecorded,
      recheckDeviceDone: deviceDone ?? _state.recheckDeviceDone,
      recordChanged: recordChanged,
      unverified: _unverified,
    );
    _controller.add(_state);
  }

  static bool _ended(CardFlowStage stage) =>
      stage is CardFlowStage_Passed || stage is CardFlowStage_Failed;

  Future<void> _wait(int ticks) => Future.delayed(tick * ticks);

  /// Waits, then says whether the flow that waited is still the one running.
  Future<bool> _still(int generation, int ticks) async {
    await _wait(ticks);
    return !_left && generation == _generation;
  }

  Future<void> _run() async {
    final alarm = scenario.alarm;
    if (alarm != null) {
      _emit(stage: CardFlowStage.failed(alarm: previewAlarm(alarm)));
      return;
    }
    switch (scenario) {
      case CardFlowPreviewScenario.fullRun:
      case CardFlowPreviewScenario.fullRunWithUnplug:
        await _prepare(_generation);
      case CardFlowPreviewScenario.recheck:
        await _startRecheck();
      case CardFlowPreviewScenario.interruptedOnOpen:
        // As core: N went out with the confirm, and the app was closed.
        _afterN = true;
        _proving = true;
        _emit(
          stage: const CardFlowStage.checkInterrupted(
            disconnected: false,
            resumesLeft: 3,
          ),
          points: sealSize,
        );
      case CardFlowPreviewScenario.deviceRoundFails:
        _proofs = sealSize;
        _emit(
          stage: const CardFlowStage.readyForFinalCheck(),
          proofs: sealSize,
          halves: true,
        );
      case CardFlowPreviewScenario.paused:
        _plug(false);
        _emit(stage: const CardFlowStage.paused(), connected: false);
        await _wait(30);
        _plug(true);
      case CardFlowPreviewScenario.differentCard:
        _emit(stage: const CardFlowStage.differentCard());
      case CardFlowPreviewScenario.sealChanged:
        _emit(
          stage: CardFlowStage.failed(
            alarm: previewAlarm(CardAlarmKind.sealChanged),
          ),
          recordChanged: true,
        );
      case CardFlowPreviewScenario.anotherPhone:
        _emit(stage: const CardFlowStage.startedOnAnotherPhone());
      case CardFlowPreviewScenario.unverified:
        // As core: the device's SEALED status, and the choice first.
        _emit(stage: const CardFlowStage.starting());
        await _wait(6);
        _emit(stage: const CardFlowStage.unverifiedIntro());
      case CardFlowPreviewScenario.storageError:
        _emit(stage: const CardFlowStage.storageError());
      case CardFlowPreviewScenario.refused:
        _emit(
          stage: const CardFlowStage.refused(
            message:
                "This device's key was restored, or made before backup cards, so it has no card to write. Keep the backup it has.",
          ),
        );
      case CardFlowPreviewScenario.startError:
        if (!_left) {
          _controller.addError(Exception('Plug in this device first'));
        }
      default:
        break;
    }
  }

  /// The seal and its pin (P1), then words 1–32 on the device (P2), then
  /// the device's quiz passing: the confirm.
  Future<void> _prepare(int generation) async {
    _emit(stage: const CardFlowStage.starting());
    if (!await _still(generation, 6)) return;
    _emit(stage: const CardFlowStage.preparingCard());
    if (!await _still(generation, 15)) return;
    for (var points = 0; points <= sealSize; points += 31) {
      _emit(stage: const CardFlowStage.pinning(), points: points);
      if (!await _still(generation, 1)) return;
    }
    _emit(stage: const CardFlowStage.deviceHalf(), points: sealSize);
    _emit(recordChanged: true);
    // The user writes and checks words 1–32 on the device.
    if (!await _still(generation, 40)) return;
    _emit(recordChanged: true);
    _emit(stage: const CardFlowStage.confirmDeviceHalf());
  }

  /// Owner B15: as core, words 33–64 come from the device for its own seal,
  /// unpinned, and the device shows words 1–32.
  @override
  Future<void> writeUnverified() async {
    calls.add('write unverified');
    await _wait(2);
    final generation = _generation;
    _emit(stage: const CardFlowStage.deviceHalf(), recordChanged: true);
    if (!await _still(generation, 40)) return;
    _emit(stage: const CardFlowStage.confirmDeviceHalf());
  }

  @override
  Future<void> deviceHalfWritten() async {
    calls.add('device half written');
    await _wait(1);
    if (_unverified) {
      // Owner B15: nothing goes to the device; no proof, no N.
      _showPage(0);
      return;
    }
    // As core (owner B8): N goes out with the confirm; no Stop from here on.
    _afterN = true;
    _proving = true;
    unawaited(_prove(_generation));
    _showPage(0);
  }

  /// The device's bit proofs, a few a tick while the device is plugged in,
  /// each verified here as it comes. A page of words stays up meanwhile;
  /// once both halves are recorded the proofs are what's left.
  Future<void> _prove(int generation) async {
    if (_proveRunning) return;
    _proveRunning = true;
    try {
      await _proveLoop(generation);
    } finally {
      _proveRunning = false;
    }
  }

  bool _proveRunning = false;

  Future<void> _proveLoop(int generation) async {
    while (_proofs < sealSize) {
      if (!await _still(generation, 1)) return;
      if (!_plugged) continue;
      _proofs = (_proofs + proofsPerTick).clamp(0, sealSize);
      final stage = _state.stage;
      final behindPage =
          stage is CardFlowStage_CheckInterrupted ||
          !_state.halvesRecorded &&
              (stage is CardFlowStage_ConfirmDeviceHalf ||
                  stage is CardFlowStage_PhoneWords ||
                  stage is CardFlowStage_PhoneAllWords ||
                  stage is CardFlowStage_PhoneConfirm ||
                  stage is CardFlowStage_SelfCheck);
      _emit(
        stage: behindPage ? stage : const CardFlowStage.proving(),
        proofs: _proofs,
      );
    }
    if (_state.halvesRecorded) _readyIfProven();
  }

  /// Continue comes once both halves are recorded and every proof is in.
  void _readyIfProven() {
    if (_proofs >= sealSize) {
      _emit(stage: const CardFlowStage.readyForFinalCheck(), proofs: sealSize);
    } else {
      _emit(stage: const CardFlowStage.proving());
    }
  }

  static int _pageEnd(int page) => page >= 10 ? 32 : 3 * page + 3;

  void _showPage(int page) {
    _page = page;
    if (page >= 11) {
      _emit(stage: const CardFlowStage.phoneAllWords(words: previewPhoneHalf));
    } else {
      _emit(
        stage: CardFlowStage.phoneWords(
          page: page,
          firstWord: 33 + 3 * page,
          words: previewPhoneHalf.sublist(3 * page, _pageEnd(page)),
        ),
      );
    }
    if (scenario == CardFlowPreviewScenario.fullRunWithUnplug &&
        page == 2 &&
        !_unpluggedOnce) {
      _unpluggedOnce = true;
      unawaited(_unplugOnWords());
    }
  }

  /// An unplug on a page of words in RAM, after N (owner B9): "Was that
  /// you?" covers the page. After "Yes" the page keeps working, and the
  /// proofs wait for the device, which comes back a little later.
  Future<void> _unplugOnWords() async {
    _plug(false);
    _covered = _state.stage;
    _emit(
      connected: false,
      stage: const CardFlowStage.checkInterrupted(
        disconnected: true,
        resumesLeft: 3,
      ),
    );
  }

  Future<void> _replugLater() async {
    final generation = _generation;
    if (!await _still(generation, 40)) return;
    _plug(true);
    _emit(connected: true);
  }

  @override
  Future<CardFlowState?> answerInterruption(bool causedByUser) async {
    calls.add(causedByUser ? 'it was me' : "it wasn't me");
    await _wait(1);
    if (!causedByUser) {
      _emit(
        stage: CardFlowStage.failed(
          alarm: previewAlarm(CardAlarmKind.disconnectedAfterN),
        ),
        recordChanged: true,
      );
      return null;
    }
    final covered = _covered;
    _covered = null;
    if (covered != null) {
      _emit(stage: covered);
      unawaited(_replugLater());
    } else {
      unawaited(_resume());
    }
    return null;
  }

  @override
  Future<void> phoneNext() async {
    calls.add('next');
    await _wait(1);
    if (_state.stage is CardFlowStage_PhoneAllWords) {
      _emit(stage: const CardFlowStage.phoneConfirm());
    } else {
      _showPage(_page + 1);
    }
  }

  @override
  Future<void> phoneBack() async {
    calls.add('back');
    await _wait(1);
    if (_state.stage is CardFlowStage_PhoneConfirm) {
      _showPage(11);
    } else if (_page > 0) {
      _showPage(_page - 1);
    }
  }

  @override
  Future<void> phoneHalfWritten() async {
    calls.add('written');
    await _wait(1);
    _quiz = 0;
    _question(wrong: false);
  }

  List<({int number, String word})> get _quizItems => [
    for (var i = 0; i < 32; i++) (number: 33 + i, word: previewPhoneHalf[i]),
    if (_recheck)
      for (final w in previewRound) (number: w.wordNumber, word: w.word),
  ];

  void _question({required bool wrong}) {
    final items = _quizItems;
    final item = items[_quiz];
    final options = [item.word, ..._lookalikes(item.word)]..shuffle();
    _emit(
      stage: CardFlowStage.selfCheck(
        k: _quiz,
        total: items.length,
        wordNumber: item.number,
        options: options,
        wrong: wrong,
        recheck: _recheck,
      ),
    );
  }

  @override
  Future<bool> selfCheckAnswer(String word) async {
    calls.add('pick');
    await _wait(1);
    final items = _quizItems;
    if (word != items[_quiz].word) {
      _question(wrong: true);
      return false;
    }
    _quiz += 1;
    if (_quiz < items.length) {
      _question(wrong: false);
    } else if (_recheck) {
      _emit(stage: const CardFlowStage.recheckPassed());
      unawaited(() async {
        await _wait(20);
        _emit(deviceDone: true);
      }());
    } else if (_unverified) {
      _emit(
        stage: const CardFlowStage.unverifiedWritten(),
        recordChanged: true,
      );
    } else {
      _emit(halves: true, recordChanged: true);
      _readyIfProven();
    }
    return true;
  }

  Future<void> _startRecheck() async {
    _recheck = true;
    _emit(stage: const CardFlowStage.starting(), halves: true);
    await _wait(6);
    _emit(stage: const CardFlowStage.recheckWaiting());
    await _wait(10);
    _quiz = 0;
    _question(wrong: false);
  }

  /// A Continue after an unplug starts a new flow, as the coordinator does.
  Future<void> _resume() async {
    final generation = _generation;
    _emit(stage: const CardFlowStage.starting(), connected: true);
    if (!await _still(generation, 6)) return;
    _showPage(_page);
    if (_proving) unawaited(_prove(generation));
  }

  @override
  Future<void> finalBegin() async {
    calls.add('final');
    // As core: only with both halves recorded and every proof verified.
    if (!_state.halvesRecorded || _proofs < sealSize) {
      throw StateError('Continue before the card is proven');
    }
    final generation = _generation;
    if (!await _still(generation, 2)) return;
    _emit(stage: const CardFlowStage.deviceRound());
    // As the coordinator's, the call returns once the round has started.
    unawaited(_deviceRound(generation));
  }

  /// The five words on the device, then the phone round.
  Future<void> _deviceRound(int generation) async {
    if (!await _still(generation, 50)) return;
    if (_state.stage is! CardFlowStage_DeviceRound) return;
    if (scenario == CardFlowPreviewScenario.deviceRoundFails) {
      _emit(
        stage: CardFlowStage.failed(
          alarm: previewAlarm(CardAlarmKind.deviceRoundFailed),
        ),
        recordChanged: true,
      );
      return;
    }
    _afterN = true;
    await _prepareBox(0);
  }

  Future<void> _prepareBox(int k) async {
    _emit(
      stage: CardFlowStage.preparingBox(k: k),
      boxes: k,
    );
    await _wait(15);
    _ask(k, secondLook: false);
  }

  void _ask(int k, {required bool secondLook}) {
    final w = previewRound[k];
    final options = [w.word, ..._lookalikes(w.word)]..shuffle();
    _emit(
      stage: CardFlowStage.question(
        k: k,
        wordNumber: w.wordNumber,
        options: options,
        secondLook: secondLook,
      ),
    );
  }

  @override
  Future<void> answer(CardAnswer answer) async {
    calls.add('answer');
    await _wait(2);
    final k = _state.boxesDone;
    final w = previewRound[k];
    final correct = switch (answer) {
      CardAnswer_Picked(:final word) => word == w.word,
      CardAnswer_NoneOfThese() => false,
    };
    if (correct) {
      final done = k + 1;
      if (done < previewRound.length) {
        _emit(boxes: done, recordChanged: true);
        unawaited(_prepareBox(done));
        return;
      }
      _emit(stage: const CardFlowStage.roundSent(), boxes: done);
      await _wait(15);
      _emit(stage: const CardFlowStage.passed(), recordChanged: true);
    } else if (!_missed[k]) {
      _missed[k] = true;
      _emit(stage: CardFlowStage.tryAgain(k: k), recordChanged: true);
      _ask(k, secondLook: true);
    } else {
      _emit(
        stage: CardFlowStage.failed(
          alarm: previewAlarm(CardAlarmKind.phoneRoundFailed),
        ),
        recordChanged: true,
      );
    }
  }

  @override
  Future<void> recordChanged() async {}

  /// How many times the page marked the backup complete, for tests.
  @visibleForTesting
  int backupMarks = 0;

  @override
  Future<void> markBackupComplete() async => backupMarks += 1;

  @override
  void leave() {
    _left = true;
    _controller.close();
    _connected.close();
  }
}

/// How the preview's fake restore ends each time it decodes.
enum PreviewRestoreOutcome {
  /// "Couldn't read": each send fails, so the app's count walks through the
  /// failure steps (owner B14).
  couldNotRead,

  /// A genuine card for another wallet (owner B11).
  wrongWallet,
}

/// The preview's own failure count, apart from the app's, so the preview's
/// restores walk through the steps together (two fake devices, one restore).
final previewRestoreFailures = RestoreFailures();

/// The one restore the preview's failures count for.
const previewRestoreKey = 'preview restore';

/// A fake card restore (design §5.3): the device acknowledges, takes words
/// 1–32 on its own a little later, and decodes once it has both halves. It
/// keeps them after a failed read, so the next send decodes again.
class PreviewCardRestoreDriver implements CardRestoreDriver {
  final Duration tick;
  final PreviewRestoreOutcome outcome;
  final _connected = StreamController<bool>.broadcast();
  StreamController<EnterPhysicalBackupState>? _controller;
  bool _deviceHalf = false;
  bool _phoneHalf = false;

  /// Each half the page sent, for tests.
  @visibleForTesting
  final sent = <List<String?>>[];

  PreviewCardRestoreDriver({
    this.tick = const Duration(milliseconds: 100),
    this.outcome = PreviewRestoreOutcome.couldNotRead,
  });

  static final _deviceId = DeviceId(
    field0: U8Array33(Uint8List(33)..[0] = 0xa5),
  );

  void _emit(CardRestoreView view) => _controller?.add(
    EnterPhysicalBackupState(
      deviceId: _deviceId,
      saved: false,
      cardRestore: view,
    ),
  );

  void _update() {
    if (_deviceHalf && _phoneHalf) {
      _emit(const CardRestoreView.checking());
      unawaited(
        Future.delayed(
          tick * 100,
          () => _emit(switch (outcome) {
            PreviewRestoreOutcome.couldNotRead =>
              const CardRestoreView.couldNotRead(),
            PreviewRestoreOutcome.wrongWallet =>
              const CardRestoreView.wrongWallet(),
          }),
        ),
      );
    } else if (_phoneHalf) {
      _emit(const CardRestoreView.waitingForDeviceHalf());
    } else {
      _emit(CardRestoreView.enterHalves(deviceHalfIn: _deviceHalf));
    }
  }

  @override
  Stream<EnterPhysicalBackupState> start() {
    final controller = _controller = StreamController();
    _phoneHalf = false;
    _deviceHalf = false;
    unawaited(() async {
      _emit(const CardRestoreView.waitingForDevice());
      await Future<void>.delayed(tick * 10);
      _update();
      await Future<void>.delayed(tick * 200);
      if (controller.isClosed) return;
      _deviceHalf = true;
      _update();
    }());
    return controller.stream;
  }

  @override
  Future<void> sendPhoneHalf(List<String?> words) async {
    sent.add(words);
    _phoneHalf = true;
    _update();
  }

  @override
  Stream<bool> deviceConnected() => _connected.stream;
}

/// Lists the preview scenarios. Debug builds only.
class CardFlowPreviewPage extends StatelessWidget {
  const CardFlowPreviewPage({super.key});

  @override
  Widget build(BuildContext context) {
    assert(kDebugMode, 'the card flow preview is for debug builds only');
    final theme = Theme.of(context);
    final answers = previewRound
        .map((w) => 'word ${w.wordNumber} is ${w.word}')
        .join(', ');
    return ListView(
      children: [
        Padding(
          padding: const EdgeInsets.all(16),
          child: Text(
            'Fake card flows, without a device. The fake device moves on by '
            'itself. Words 33–64 are on the pages; in the phone round, '
            '$answers.',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        for (final scenario in CardFlowPreviewScenario.values)
          ListTile(
            title: Text(scenario.title),
            subtitle: Text(scenario.subtitle),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => VerifiableCardFlow.show(
              context,
              driver: PreviewCardFlowDriver(scenario),
            ),
          ),
        for (final (title, subtitle, device, outcome) in [
          (
            'Restore from a card',
            "Words 33–64 here, a fake device for 1–32. Every send couldn't "
                'be read: send again for each failure step',
            'Preview A',
            PreviewRestoreOutcome.couldNotRead,
          ),
          (
            'Restore on another device',
            'The same restore on a second fake device: after "a different '
                'app", a failure here is the second device',
            'Preview B',
            PreviewRestoreOutcome.couldNotRead,
          ),
          (
            "Restore another wallet's card",
            'A valid card for a different wallet (owner B11)',
            'Preview A',
            PreviewRestoreOutcome.wrongWallet,
          ),
        ])
          ListTile(
            title: Text(title),
            subtitle: Text(subtitle),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => _showRestore(context, device, outcome),
          ),
        ListTile(
          title: const Text('Restore: start the failure steps again'),
          subtitle: const Text(
            "Clears the preview restores' count: the next failure is step 1",
          ),
          onTap: () {
            previewRestoreFailures.read(previewRestoreKey);
            showMessageSnackbar(
              context,
              'The preview restore starts at step 1',
            );
          },
        ),
      ],
    );
  }

  static void _showRestore(
    BuildContext context,
    String device,
    PreviewRestoreOutcome outcome,
  ) => unawaited(
    MaybeFullscreenDialog.show(
      context: context,
      child: Builder(
        builder: (dialogContext) => EnterCardView(
          deviceName: device,
          driver: PreviewCardRestoreDriver(outcome: outcome),
          failures: previewRestoreFailures,
          failureKey: previewRestoreKey,
          failureDevice: device,
          onFinished: (_) {},
          onCancel: () => Navigator.pop(dialogContext),
          onError: (message) {
            Navigator.pop(dialogContext);
            showMessageSnackbar(context, message);
          },
        ),
      ),
    ),
  );
}
