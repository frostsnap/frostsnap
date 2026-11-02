import 'dart:async';

import 'package:dynamic_color/dynamic_color.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/verifiable_card/alarm_view.dart';
import 'package:frostsnap/verifiable_card/answer_on_device_view.dart';
import 'package:frostsnap/verifiable_card/card_finished_view.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';
import 'package:frostsnap/verifiable_card/card_quiz.dart';
import 'package:frostsnap/verifiable_card/card_words.dart';
import 'package:frostsnap/verifiable_card/final_check_intro.dart';
import 'package:frostsnap/verifiable_card/five_dots.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';
import 'package:frostsnap/verifiable_card/hold_button.dart';
import 'package:frostsnap/verifiable_card/secure_screen.dart';

/// The calls the card flow makes. [CoordCardFlowDriver] talks to the
/// coordinator; the debug preview (card_flow_preview.dart) fakes one so every
/// page can be looked at without a device.
abstract class CardFlowDriver {
  /// Starts, or resumes, the flow. Called when the page opens, and again on
  /// the Paused page's Continue: an unplug ends the flow, and a new one
  /// resumes from the device's card record (design §3.6).
  Stream<CardFlowState> start();

  /// Whether the device is plugged in, for the Paused page's Continue. The
  /// flow's own state stops at the unplug.
  Stream<bool> deviceConnected();

  /// The seal's commitment points, and so the bit proofs the phone verifies:
  /// the bars' total. The bridge's `card_seal_size`.
  int get sealSize;

  /// The hold on "Have you written words 1–32?" (owner B1): the device starts
  /// checking the card, and the pages of words 33–64 follow.
  Future<void> deviceHalfWritten();

  /// Owner B15: [Write it unverified] on an install that didn't create the
  /// wallet. The device shows words 1–32, then this phone words 33–64, with a
  /// quiz on each half, and nothing is verified.
  Future<void> writeUnverified();

  /// Next and Back on the pages of words 33–64 and the confirm page.
  Future<void> phoneNext();
  Future<void> phoneBack();

  /// The hold on "Check you've written": the phone's quiz starts.
  Future<void> phoneHalfWritten();

  /// One pick in the phone's quiz. Says whether it was right.
  Future<bool> selfCheckAnswer(String word);

  /// [Continue] on "Your card is written and checked" (`FinalBegin`). Offered
  /// only once both halves are recorded and every proof verified.
  Future<void> finalBegin();

  /// One phone-round answer.
  Future<void> answer(CardAnswer answer);

  /// Owner B9: the answer on the interruption's question, "did you cause
  /// it?". [causedByUser]: "Yes, it was me" (the check resumes on the same N
  /// once the device is back) or "No, I didn't" (the device is marked suspect
  /// and the check ends). Returns its verdict if no flow heard it.
  Future<CardFlowState?> answerInterruption(bool causedByUser);

  /// The phone's card record changed: re-emit the backup run so the checklist
  /// and the wallet banner pick up the new status.
  Future<void> recordChanged();

  /// Both halves are recorded on this phone: the backup run's step for this
  /// key is done (design §3.10). Idempotent.
  Future<void> markBackupComplete();

  /// The user left the page. That is a pause, never a stop (spec L1.7.6); after
  /// N it's an interruption the next flow asks about (owner B9, review F1).
  void leave();
}

class CoordCardFlowDriver implements CardFlowDriver {
  final DeviceId deviceId;
  final AccessStructureRef accessStructureRef;
  final SymmetricKey encryptionKey;

  CoordCardFlowDriver({
    required this.deviceId,
    required this.accessStructureRef,
    required this.encryptionKey,
  });

  @override
  Stream<CardFlowState> start() => coord.startCardBackup(
    deviceId: deviceId,
    accessStructureRef: accessStructureRef,
    encryptionKey: encryptionKey,
  );

  @override
  Stream<bool> deviceConnected() => GlobalStreams.deviceListSubject
      .map((update) => update.state.getDevice(id: deviceId) != null)
      .distinct();

  @override
  int get sealSize => cardSealSize();

  @override
  Future<void> deviceHalfWritten() =>
      coord.cardDeviceHalfWritten(deviceId: deviceId);

  @override
  Future<void> writeUnverified() =>
      coord.cardWriteUnverified(deviceId: deviceId);

  @override
  Future<void> phoneNext() => coord.cardPhoneNext(deviceId: deviceId);

  @override
  Future<void> phoneBack() => coord.cardPhoneBack(deviceId: deviceId);

  @override
  Future<void> phoneHalfWritten() =>
      coord.cardPhoneHalfWritten(deviceId: deviceId);

  @override
  Future<bool> selfCheckAnswer(String word) =>
      coord.cardSelfCheckAnswer(deviceId: deviceId, word: word);

  @override
  Future<void> finalBegin() => coord.cardFinalBegin(deviceId: deviceId);

  @override
  Future<void> answer(CardAnswer answer) =>
      coord.cardAnswer(deviceId: deviceId, answer: answer);

  @override
  Future<CardFlowState?> answerInterruption(bool causedByUser) => coord
      .cardInterruptionAnswer(deviceId: deviceId, causedByUser: causedByUser);

  @override
  Future<void> recordChanged() =>
      coord.emitBackupRun(keyId: accessStructureRef.keyId);

  @override
  Future<void> markBackupComplete() async {
    final shareIndex = coord
        .getAccessStructure(asRef: accessStructureRef)
        ?.getDeviceShortShareIndex(deviceId: deviceId);
    if (shareIndex == null) return;
    await coord.markBackupComplete(
      accessStructureRef: accessStructureRef,
      shareIndex: shareIndex,
    );
  }

  /// Synchronous, so the flow is gone before a page opened next can start
  /// one; and it ends only this device's card flow, not every protocol.
  @override
  void leave() => coord.cancelCardBackup(deviceId: deviceId);
}

/// The phone's side of making and checking one 64-word split card (design
/// §5.2, pages P1 to P15; spec PK.7 as amended by owner B1). The device
/// seals the card and this phone pins the seal before any word shows. The
/// device shows words 1–32 and quizzes on them; the user confirms here that
/// they're written, and only then does this page show words 33–64 and quiz
/// on them, while the device proves its seal behind them. Continue needs
/// both halves and every proof. Then the device quizzes 5 of words 33–64,
/// and this page 5 of words 1–32, with "None of these" for a word that isn't
/// the one written.
///
/// Owner B12 and its follow-up 5: there is no Stop on the phone. Once N is
/// out the only way out is a disconnect, or leaving the page, which asks B9's
/// question.
/// Owner B9: a disconnect while the check runs asks whether the user caused
/// it, and the check goes on only after "Yes, it was me".
class VerifiableCardFlow extends StatefulWidget {
  final CardFlowDriver driver;

  const VerifiableCardFlow({super.key, required this.driver});

  static Future<void> show(
    BuildContext context, {
    required CardFlowDriver driver,
  }) => MaybeFullscreenDialog.show(
    context: context,
    child: VerifiableCardFlow(driver: driver),
  );

  @override
  State<VerifiableCardFlow> createState() => _VerifiableCardFlowState();
}

class _VerifiableCardFlowState extends State<VerifiableCardFlow> {
  StreamSubscription<CardFlowState>? _subscription;
  late final StreamSubscription<bool> _connectedSubscription;

  /// What the device list last said.
  bool _deviceConnected = false;

  /// The device list has said the device is out since the flow last saw it
  /// in. The list can lag an unplug, still saying the device is in, so after
  /// an unplug Continue waits for the list to have said it is out.
  bool _listSawUnplug = false;

  CardFlowState? _state;
  Object? _startError;

  /// The last phone-round question, for a `TryAgain`, which the same
  /// question with its second look follows.
  CardFlowStage_Question? _lastQuestion;

  /// An action is on its way to the coordinator. Inputs stay off until the
  /// call returns, so one tap can't be sent twice.
  bool _busy = false;
  bool _passReported = false;
  bool _backupMarked = false;

  @override
  void initState() {
    super.initState();
    _start();
    _connectedSubscription = widget.driver.deviceConnected().listen((
      connected,
    ) {
      if (!mounted) return;
      setState(() {
        _deviceConnected = connected;
        if (!connected) _listSawUnplug = true;
      });
    });
  }

  /// Subscribes to a new flow. Never from build(): every subscription starts
  /// the protocol again.
  void _start() {
    _subscription = widget.driver.start().listen(
      _onState,
      onError: (Object error) {
        if (mounted) setState(() => _startError = error);
      },
    );
  }

  /// The page on screen, and how many times the page has changed. A page
  /// that comes back while it is still fading out (Back then Next within
  /// the switch, now that every word of 33–64 is ready at once) gets a new
  /// key, so the switcher never holds two children with one key.
  String? _shownPage;
  int _pageChanges = 0;

  /// The Paused page's Continue: a new flow, which the device resumes from
  /// its card record. Nothing of the ended flow's page carries over.
  void _continue() {
    unawaited(_subscription?.cancel());
    setState(() {
      _state = null;
      _startError = null;
      _lastQuestion = null;
    });
    _start();
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _connectedSubscription.cancel();
    widget.driver.leave();
    super.dispose();
  }

  void _onState(CardFlowState state) {
    if (!mounted) {
      return;
    }
    final stage = state.stage;
    // Nothing here logs a stage or a word (design §5.5).
    setState(() {
      if (stage is CardFlowStage_Question) _lastQuestion = stage;
      // Both say the device is in: an unplug from here is a new one.
      if (stage is! CardFlowStage_Paused &&
          state.deviceConnected &&
          _deviceConnected) {
        _listSawUnplug = false;
      }
      _state = state;
    });

    if (state.recordChanged) _recordChanged();
    if (stage is CardFlowStage_Passed && !_passReported) {
      _passReported = true;
      _recordChanged();
    }
    // Both halves recorded is the backup run's step, never the verdict
    // (design §3.10).
    if (state.halvesRecorded && !_backupMarked) {
      _backupMarked = true;
      unawaited(
        Future.sync(widget.driver.markBackupComplete).catchError((Object e) {
          debugPrint('card flow: markBackupComplete failed: $e');
        }),
      );
    }
  }

  /// Only the checklist and the banner hang on this, so a failure is logged
  /// rather than shown.
  void _recordChanged() {
    unawaited(
      Future.sync(widget.driver.recordChanged).catchError((Object e) {
        debugPrint('card flow: emitBackupRun failed: $e');
      }),
    );
  }

  Future<void> _act(Future<void> Function() action) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      await action();
    } catch (e) {
      if (mounted) showMessageSnackbar(context, displayExceptionMessage(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// A quiz pick: its result is the quiz's feedback; an error is shown here
  /// and the item stands.
  Future<bool?> _pick(Future<bool?> Function() send) async {
    try {
      return await send();
    } catch (e) {
      if (mounted) showMessageSnackbar(context, displayExceptionMessage(e));
      rethrow;
    }
  }

  /// A page that stands whether or not the device is plugged in.
  static bool _isEnd(CardFlowStage stage) => switch (stage) {
    CardFlowStage_Passed() ||
    CardFlowStage_Failed() ||
    CardFlowStage_DifferentCard() ||
    CardFlowStage_Refused() ||
    CardFlowStage_StartedOnAnotherPhone() ||
    CardFlowStage_StorageError() ||
    CardFlowStage_RecheckPassed() ||
    CardFlowStage_RecheckUnavailable() ||
    CardFlowStage_Paused() ||
    CardFlowStage_CheckInterrupted() ||
    CardFlowStage_UnverifiedWritten() => true,
    _ => false,
  };

  /// A page of words already in this phone's RAM, which keeps working while
  /// the device is unplugged (design §3.7).
  static bool _phoneOnly(CardFlowStage stage) => switch (stage) {
    CardFlowStage_PhoneWords() ||
    CardFlowStage_PhoneAllWords() ||
    CardFlowStage_PhoneConfirm() => true,
    CardFlowStage_SelfCheck(:final recheck) => !recheck,
    _ => false,
  };

  /// The user is writing or checking words 33–64 while the device proves its
  /// seal behind them (owner B1): the proofs' bar goes under the page.
  static bool _writingPhoneHalf(CardFlowState state) =>
      _phoneOnly(state.stage) && !state.halvesRecorded && !state.unverified;

  /// Owner B15: a page of the unverified card's writing, which carries the
  /// banner that this phone can't check it.
  static bool _unverifiedWriting(CardFlowState state) =>
      state.unverified &&
      switch (state.stage) {
        CardFlowStage_DeviceHalf() ||
        CardFlowStage_ConfirmDeviceHalf() ||
        CardFlowStage_PhoneWords() ||
        CardFlowStage_PhoneAllWords() ||
        CardFlowStage_PhoneConfirm() => true,
        CardFlowStage_SelfCheck(:final recheck) => !recheck,
        _ => false,
      };

  /// [Continue] to the final check: both halves recorded and every proof
  /// verified (spec PK.7 (e)). Core refuses it otherwise; the page never
  /// offers it.
  bool _readyToContinue(CardFlowState state) =>
      state.stage is CardFlowStage_ReadyForFinalCheck &&
      state.halvesRecorded &&
      state.proofsVerified >= widget.driver.sealSize;

  /// Owner B9: the answer on the interruption's question. The verdict of "No,
  /// I didn't" comes back here if no flow heard it.
  void _answerInterruption(bool causedByUser) => _act(() async {
    final unheard = await widget.driver.answerInterruption(causedByUser);
    if (unheard != null && mounted) _onState(unheard);
  });

  @override
  Widget build(BuildContext context) {
    final state = _state;
    final isCompact = WindowSizeContext.of(context) == WindowSizeClass.compact;

    final header = AppBar(
      primary: isCompact,
      automaticallyImplyLeading: false,
      leading: IconButton(
        icon: const Icon(Icons.close_rounded),
        tooltip: 'Close',
        onPressed: () => Navigator.pop(context),
      ),
      title: const Text('Backup card'),
    );

    final (key, page) = _buildPage(context, state);
    if (key != _shownPage) {
      _shownPage = key;
      _pageChanges += 1;
    }
    final unpluggedBanner =
        state != null && !state.deviceConnected && _phoneOnly(state.stage);
    final provingStrip =
        state != null && _startError == null && _writingPhoneHalf(state);
    final showUnverified =
        state != null && _startError == null && _unverifiedWriting(state);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        header,
        // The whole route: the flags stay set between pages, and the Linux
        // warning stays in one place.
        Flexible(
          child: SecureScreen(
            child: SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                spacing: 16,
                children: [
                  // Owner B15: every page of an unverified card says so.
                  if (showUnverified)
                    const CardBanner(
                      color: cautionColor,
                      text: unverifiedBanner,
                    ),
                  // P14 over a page of words in RAM: the page keeps working.
                  if (unpluggedBanner)
                    CardBanner(
                      color: cautionColor,
                      text:
                          'Your device is unplugged. Keep going here; plug '
                          'it back in before the next step.',
                    ),
                  AnimatedSwitcher(
                    duration: Durations.medium2,
                    layoutBuilder: topSwitcherLayout,
                    child: KeyedSubtree(
                      key: ValueKey('$key#$_pageChanges'),
                      child: page,
                    ),
                  ),
                  // Outside the switcher, so it stays put as the pages turn.
                  if (provingStrip)
                    Padding(
                      padding: const EdgeInsets.only(top: 8),
                      child: ProofsProgress(
                        proofsVerified: state.proofsVerified,
                        sealSize: widget.driver.sealSize,
                        connected: state.deviceConnected,
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }

  /// The page for [state], with a key that changes only when the page does,
  /// so updates within a page (progress, the next quiz item) don't animate.
  (String, Widget) _buildPage(BuildContext context, CardFlowState? state) {
    final startError = _startError;
    if (startError != null) {
      return (
        'start-error',
        CardFlowPage(
          icon: Icons.error_outline_rounded,
          error: true,
          title: "Couldn't start the backup",
          children: [
            flowBody(context, displayExceptionMessage(startError)),
            _closeButton(context, 'Close'),
          ],
        ),
      );
    }
    if (state == null) return ('intro', _intro(context, null));

    final stage = state.stage;
    // An end stage stands whether or not the device is still plugged in, and
    // so does a page of words in RAM. Otherwise an unplugged flow waits for
    // the device, and resumes by itself once it is back.
    if (!_isEnd(stage) && !_phoneOnly(stage) && !state.deviceConnected) {
      return ('plug-in', _plugIn(context));
    }

    final busy = _busy;
    return switch (stage) {
      CardFlowStage_Starting() ||
      CardFlowStage_PreparingCard() ||
      CardFlowStage_Pinning() => ('intro', _intro(context, state)),
      CardFlowStage_DeviceHalf() => ('device-half', _deviceHalf(context)),
      CardFlowStage_ConfirmDeviceHalf() => (
        'device-half-confirm',
        DeviceHalfConfirmPage(
          unverified: state.unverified,
          onConfirmed: busy
              ? null
              : () => _act(widget.driver.deviceHalfWritten),
        ),
      ),
      CardFlowStage_PhoneWords(:final page, :final firstWord, :final words) => (
        'words-$page',
        PhoneWordsPage(
          page: page,
          firstWord: firstWord,
          words: words,
          onBack: busy ? null : () => _act(widget.driver.phoneBack),
          onNext: busy ? null : () => _act(widget.driver.phoneNext),
        ),
      ),
      CardFlowStage_PhoneAllWords(:final words) => (
        'all-words',
        PhoneAllWordsPage(
          words: words,
          onBack: busy ? null : () => _act(widget.driver.phoneBack),
          onNext: busy ? null : () => _act(widget.driver.phoneNext),
        ),
      ),
      CardFlowStage_PhoneConfirm() => (
        'confirm',
        PhoneConfirmPage(
          onBack: busy ? null : () => _act(widget.driver.phoneBack),
          onConfirmed: busy ? null : () => _act(widget.driver.phoneHalfWritten),
        ),
      ),
      CardFlowStage_SelfCheck(
        :final k,
        :final total,
        :final wordNumber,
        :final options,
        :final recheck,
      ) =>
        (
          recheck ? 'recheck-quiz' : 'self-check',
          CardQuiz(
            wordNumber: wordNumber,
            options: options,
            total: total,
            done: k,
            onPick: (word) => _pick(() => widget.driver.selfCheckAnswer(word!)),
          ),
        ),
      // Both halves recorded: the checked card, with Continue off until the
      // proofs are in. Same key as the page Continue comes on in, so it
      // doesn't turn.
      CardFlowStage_Proving() when state.halvesRecorded => (
        'final',
        FinalCheckIntro(
          proofsVerified: state.proofsVerified,
          sealSize: widget.driver.sealSize,
          onStart: null,
        ),
      ),
      CardFlowStage_Proving() => (
        'proving',
        CardProvingView(
          proofsVerified: state.proofsVerified,
          sealSize: widget.driver.sealSize,
        ),
      ),
      CardFlowStage_ReadyForFinalCheck() => (
        'final',
        FinalCheckIntro(
          proofsVerified: state.proofsVerified,
          sealSize: widget.driver.sealSize,
          onStart: busy || !_readyToContinue(state)
              ? null
              : () => _act(widget.driver.finalBegin),
        ),
      ),
      CardFlowStage_DeviceRound() => (
        'device-round',
        const AnswerOnDeviceView(),
      ),
      CardFlowStage_PreparingBox(:final k) => (
        'preparing-$k',
        _preparingBox(context, k),
      ),
      CardFlowStage_Question(
        :final k,
        :final wordNumber,
        :final options,
        :final secondLook,
      ) =>
        (
          'question-$k-$secondLook',
          _question(context, k, wordNumber, options, secondLook: secondLook),
        ),
      // The second look follows at once; until it does, the question stays.
      CardFlowStage_TryAgain(:final k) => switch (_lastQuestion) {
        final question? when question.k == k => (
          'question-$k-true',
          _question(
            context,
            k,
            question.wordNumber,
            question.options,
            secondLook: true,
            enabled: false,
          ),
        ),
        _ => ('preparing-$k', _preparingBox(context, k)),
      },
      CardFlowStage_RoundSent() => ('round-sent', _roundSent(context)),
      CardFlowStage_Passed() => ('passed', _passed(context)),
      CardFlowStage_RecheckWaiting() => ('recheck', _recheckWaiting(context)),
      CardFlowStage_RecheckPassed() => (
        'recheck-done',
        _recheckDone(context, state, available: true),
      ),
      CardFlowStage_RecheckUnavailable() => (
        'recheck-done',
        _recheckDone(context, state, available: false),
      ),
      CardFlowStage_Failed(:final alarm) => (
        'failed',
        AlarmView(alarm: alarm, onClose: () => Navigator.pop(context)),
      ),
      CardFlowStage_DifferentCard() => ('different', _differentCard(context)),
      CardFlowStage_Refused(:final message) => (
        'refused',
        _refused(context, message),
      ),
      CardFlowStage_StartedOnAnotherPhone() => (
        'other-phone',
        _anotherPhone(context),
      ),
      CardFlowStage_StorageError() => ('storage', _storageError(context)),
      CardFlowStage_Paused() => (
        'paused',
        _paused(context, unplugged: !state.deviceConnected),
      ),
      CardFlowStage_WrongFirmware() => (
        'wrong-firmware',
        _wrongFirmware(context),
      ),
      CardFlowStage_CheckInterrupted(:final disconnected, :final resumesLeft) =>
        (
          'interrupted',
          CheckInterruptedPage(
            disconnected: disconnected,
            resumesLeft: resumesLeft,
            onAnswer: busy ? null : _answerInterruption,
          ),
        ),
      CardFlowStage_UnverifiedIntro() => (
        'unverified-intro',
        UnverifiedIntroPage(
          onWrite: busy ? null : () => _act(widget.driver.writeUnverified),
          onClose: () => Navigator.pop(context),
        ),
      ),
      CardFlowStage_UnverifiedWritten() => (
        'unverified-written',
        UnverifiedWrittenPage(onClose: () => Navigator.pop(context)),
      ),
    };
  }

  // P1, before any word is shown: the device makes the card's seal, and
  // the phone pulls its points and words 33–64 and pins it. The bar covers
  // the points (the seal's own time isn't known here, so it starts once the
  // seal is done).
  Widget _intro(BuildContext context, CardFlowState? state) {
    final sealSize = widget.driver.sealSize;
    final progress = switch (state?.stage) {
      CardFlowStage_Pinning() when sealSize > 0 =>
        (state!.pointsReceived / sealSize).clamp(0.0, 1.0),
      _ => null,
    };
    return CardFlowPage(
      title: 'Preparing your card',
      children: [
        flowBody(context, introLine),
        flowBody(
          context,
          'Keep your device plugged in. This takes about 20 seconds.',
        ),
        LinearProgressIndicator(value: progress),
        const RuleLine(privacyLine),
        const ExperimentalLabel(experimentalLabel),
      ],
    );
  }

  // P2: the standard "look at your device" body. It moves on by itself when the
  // device's quiz passes, to the confirm.
  Widget _deviceHalf(BuildContext context) {
    final theme = Theme.of(context);
    return CardFlowPage(
      leading: ImageIcon(
        const AssetImage('assets/icons/device2.png'),
        size: 64,
        color: theme.colorScheme.primary,
      ),
      title: 'Write words 1–32 from your device',
      children: [
        flowBody(
          context,
          'Write each word next to its number on your card, then answer '
          "your device's quiz.",
        ),
        const Center(
          child: DeviceActionHint(
            label: 'Write words 1–32',
            icon: Icons.edit_note,
          ),
        ),
        Text(
          'Words 33–64 come on this phone after that.',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }

  // P10
  Widget _preparingBox(BuildContext context, int k) => CardFlowPage(
    title: 'Preparing question ${k + 1} of 5…',
    children: [
      flowBody(context, 'Your device is proving the word. About 25 seconds.'),
      const Center(child: CircularProgressIndicator()),
      Center(child: FiveDots(done: k)),
    ],
  );

  // P11: the first pick is final; a miss gets one second look.
  Widget _question(
    BuildContext context,
    int k,
    int wordNumber,
    List<String> options, {
    required bool secondLook,
    bool enabled = true,
  }) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    spacing: 16,
    children: [
      flowBody(context, roundLine),
      CardQuiz(
        wordNumber: wordNumber,
        options: options,
        total: 5,
        done: k,
        progress: Center(child: FiveDots(done: k)),
        noneOfThese: true,
        banner: secondLook
            ? 'Look again at word $wordNumber on your card.'
            : null,
        onPick: enabled
            ? (word) => _pick(() async {
                await widget.driver.answer(
                  word == null
                      ? const CardAnswer.noneOfThese()
                      : CardAnswer.picked(word: word),
                );
                return null;
              })
            : null,
      ),
    ],
  );

  Widget _roundSent(BuildContext context) => CardFlowPage(
    children: [
      flowBody(context, "Waiting for your device's verdict…"),
      const Center(child: CircularProgressIndicator()),
    ],
  );

  // P12: both rounds passed (the two-key verdict).
  Widget _passed(BuildContext context) {
    final theme = Theme.of(context);
    return CardFlowPage(
      leading: const AnimatedCheckCircle(size: 64),
      title: 'Backup checked',
      titleColor: Colors.green.harmonizeWith(theme.colorScheme.primary),
      children: [
        flowBody(
          context,
          'Both halves of your card are checked. Store it safely.',
        ),
        _closeButton(context, 'Done'),
      ],
    );
  }

  // P15
  Widget _recheckWaiting(BuildContext context) => CardFlowPage(
    title: 'Check your backup',
    children: [
      flowBody(context, 'Getting your words from your device…'),
      const Center(child: CircularProgressIndicator()),
      flowBody(context, 'Your device quizzes you on its half too.'),
    ],
  );

  Widget _recheckDone(
    BuildContext context,
    CardFlowState state, {
    required bool available,
  }) {
    final theme = Theme.of(context);
    final deviceDone = state.recheckDeviceDone;
    return CardFlowPage(
      leading: available ? const AnimatedCheckCircle(size: 64) : null,
      icon: available ? null : Icons.info_outline_rounded,
      title: available
          ? 'Backup checked'
          : deviceDone
          ? "Your device's half is checked"
          : 'Your device is checking its half',
      titleColor: available
          ? Colors.green.harmonizeWith(theme.colorScheme.primary)
          : null,
      children: [
        flowBody(
          context,
          available
              ? 'Words 33–64 checked here.'
              : 'Words 33–64 can be checked only with the phone that made '
                    'this card.',
        ),
        // "Both checks" only when this phone checked its half: a phone
        // that didn't pass this card never says it's checked (audit F1).
        flowBody(
          context,
          !deviceDone
              ? 'Finish the quiz on your device.'
              : available
              ? 'Both checks done.'
              : "Your device's check is done.",
        ),
        _closeButton(context, 'Done'),
      ],
    );
  }

  Widget _differentCard(BuildContext context) => CardFlowPage(
    icon: Icons.dangerous_rounded,
    error: true,
    title: 'This device is showing a different card',
    children: [
      const ErrorCard(lines: ["Don't write anything. Don't use this wallet."]),
      _closeButton(context, 'Close'),
    ],
  );

  // A device refusal. The device shows what to do; the refusal's text is
  // shown here as it came.
  Widget _refused(BuildContext context, String message) => CardFlowPage(
    icon: Icons.info_outline_rounded,
    title: 'See your device',
    children: [flowBody(context, message), _closeButton(context, 'Close')],
  );

  Widget _anotherPhone(BuildContext context) => CardFlowPage(
    icon: Icons.phonelink_lock_rounded,
    children: [
      flowBody(context, anotherPhoneLine),
      _closeButton(context, 'Close'),
    ],
  );

  // An end page. An unplug reboots the device to its home screen, and
  // Continue starts a new flow, which carries on from the device's card
  // record. A Cancel from elsewhere ends it the same way, with the device
  // still plugged in.
  Widget _paused(BuildContext context, {required bool unplugged}) {
    final ready = _deviceConnected && (!unplugged || _listSawUnplug);
    return CardFlowPage(
      icon: Icons.usb_off_rounded,
      children: [
        flowBody(
          context,
          unplugged
              ? 'Your device was unplugged. Your progress is saved on it.'
              : 'The backup was interrupted. Your progress is saved on your '
                    'device.',
        ),
        if (!ready)
          flowBody(context, 'Plug your device back in, then press Continue.'),
        FilledButton(
          onPressed: ready ? _continue : null,
          child: const Text('Continue'),
        ),
      ],
    );
  }

  // A flow started after N without its device, which came back with other
  // firmware: nothing is sent to it.
  Widget _wrongFirmware(BuildContext context) => CardFlowPage(
    icon: Icons.system_update_rounded,
    children: [
      flowBody(context, "Update this device's firmware first."),
      flowBody(
        context,
        'Unplug it, update it, then plug it back in to continue.',
      ),
    ],
  );

  // The device is out: the flow carries on by itself once it is back.
  Widget _plugIn(BuildContext context) => CardFlowPage(
    icon: Icons.usb_rounded,
    children: [flowBody(context, 'Plug in your device to continue.')],
  );

  Widget _storageError(BuildContext context) => CardFlowPage(
    icon: Icons.error_outline_rounded,
    error: true,
    children: [
      flowBody(
        context,
        "This phone couldn't save the backup. Restart the app; it continues "
        'with the same words.',
      ),
      _closeButton(context, 'Close'),
    ],
  );

  Widget _closeButton(BuildContext context, String label) =>
      FilledButton(onPressed: () => Navigator.pop(context), child: Text(label));
}

/// Owner B9: the check was interrupted after N. It warns that this matters for
/// security, since a faulty or malicious device could disconnect to get
/// another chance at the check, and asks whether the user caused it.
/// [disconnected]: the device disconnected while this page was open;
/// otherwise the question is left from before (the app was closed, or the
/// device disconnected, while the check ran).
class CheckInterruptedPage extends StatelessWidget {
  final bool disconnected;

  /// How many more "Yes, it was me" answers resume this check (owner R2-2).
  /// At 1 the page warns that this is the last resume (review B15 protocol
  /// F2); at 0 it says that either answer ends the check.
  final int resumesLeft;

  /// The answer: true for "Yes, it was me". Null while an action is on its
  /// way.
  final void Function(bool causedByUser)? onAnswer;

  const CheckInterruptedPage({
    super.key,
    required this.disconnected,
    required this.resumesLeft,
    required this.onAnswer,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final answer = onAnswer;
    return CardFlowPage(
      icon: Icons.warning_amber_rounded,
      title: interruptedTitle,
      children: [
        CardBanner(
          color: cautionColor,
          text: disconnected
              ? interruptedDisconnectedLine
              : interruptedClosedLine,
        ),
        flowBody(context, interruptedSecurityLine),
        flowBody(context, interruptedQuestion),
        flowBody(context, interruptedFrozenLine),
        if (resumesLeft == 1)
          CardBanner(
            color: theme.colorScheme.error,
            text: interruptedLastResumeLine,
          ),
        if (resumesLeft == 0)
          CardBanner(
            color: theme.colorScheme.error,
            text: interruptedTooOftenLine,
          ),
        FilledButton(
          onPressed: answer == null ? null : () => answer(true),
          child: const Text(interruptedYes),
        ),
        // Owner follow-up 1 to B9: "No" marks the device suspect for good, so
        // it's a hold, never one stray tap.
        Center(
          child: HoldToConfirmButton(
            label: const Text(interruptedNo, textAlign: TextAlign.center),
            color: theme.colorScheme.error,
            onColor: theme.colorScheme.onError,
            size: 112,
            onComplete: answer == null ? null : () => answer(false),
          ),
        ),
        Text(
          interruptedNoHold,
          textAlign: TextAlign.center,
          style: theme.textTheme.bodyMedium,
        ),
        Text(
          interruptedNoLine,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }
}

/// Owner B15: this install didn't create the wallet, and the device's card
/// isn't written. The phone can't verify it: it says so, and that the card
/// must be verified on a trusted device that learns all 64 words, and writes
/// it only once the user chooses to. Nothing has gone to the device yet. DRAFT
/// COPY (card_lines.dart).
class UnverifiedIntroPage extends StatelessWidget {
  /// [Write it unverified]. Null while an action is on its way.
  final VoidCallback? onWrite;
  final VoidCallback onClose;

  const UnverifiedIntroPage({
    super.key,
    required this.onWrite,
    required this.onClose,
  });

  @override
  Widget build(BuildContext context) => CardFlowPage(
    icon: Icons.gpp_maybe_outlined,
    title: unverifiedTitle,
    children: [
      flowBody(context, unverifiedLine),
      const CardBanner(color: cautionColor, text: unverifiedVerifyLine),
      flowBody(context, unverifiedKeygenPhoneLine),
      const RuleLine(privacyLine),
      FilledButton(
        onPressed: onWrite,
        child: const Text(unverifiedWriteButton),
      ),
      TextButton(onPressed: onClose, child: const Text('Close')),
    ],
  );
}

/// Owner B15: the unverified card is written and both quizzes passed. Its
/// record is "written, not verified", never "Backup checked". DRAFT COPY.
class UnverifiedWrittenPage extends StatelessWidget {
  final VoidCallback onClose;

  const UnverifiedWrittenPage({super.key, required this.onClose});

  @override
  Widget build(BuildContext context) => CardFlowPage(
    icon: Icons.gpp_maybe_outlined,
    title: unverifiedWrittenTitle,
    children: [
      flowBody(context, unverifiedWrittenLine),
      const CardBanner(color: cautionColor, text: unverifiedVerifyLine),
      flowBody(context, 'Store your card safely.'),
      FilledButton(onPressed: onClose, child: const Text('Done')),
    ],
  );
}
