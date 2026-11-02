import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration/phone_half_entry.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';
import 'package:frostsnap/verifiable_card/flow_page.dart';
import 'package:frostsnap/verifiable_card/secure_screen.dart';

/// The calls a card restore makes. [CoordCardRestoreDriver] talks to the
/// coordinator; a test or the debug preview fakes one.
abstract class CardRestoreDriver {
  /// Sends `EnterCard`: the device opens its entry for words 1–32. Called
  /// when the page opens, and again after the device is plugged back in.
  Stream<EnterPhysicalBackupState> start();

  /// The app's words 33–64 (null: can't read) for the restore [start]
  /// began. They reach the device once it has acknowledged the restore.
  Future<void> sendPhoneHalf(List<String?> words);

  Stream<bool> deviceConnected();
}

class CoordCardRestoreDriver implements CardRestoreDriver {
  final DeviceId deviceId;
  final CardRestoreTarget target;

  CoordCardRestoreDriver({required this.deviceId, required this.target});

  @override
  Stream<EnterPhysicalBackupState> start() =>
      coord.tellDeviceToEnterCard(deviceId: deviceId, target: target);

  @override
  Future<void> sendPhoneHalf(List<String?> words) =>
      coord.cardRestorePhoneHalf(deviceId: deviceId, words: words);

  @override
  Stream<bool> deviceConnected() => GlobalStreams.deviceListSubject
      .map((update) => update.state.getDevice(id: deviceId) != null)
      .distinct();
}

/// What the restore protocol says when its device is unplugged
/// (frostsnap_coordinator enter_physical_backup.rs).
const _unpluggedAbort = 'device was unplugged';

/// What it says for a card this device already holds: decided after the
/// masks passed, so the card read (failflow-review L2).
const alreadyHeldAbort =
    'This device already holds this key. Restore it on a blank device.';

/// The app's own count of "couldn't read" results, per restore, kept for this
/// app install (owner, 2 Nov 2026: the escalating restore-failure flow; owner
/// follow-up 2 to B14: an app restart doesn't go back to step 1). It
/// counts what this phone saw happen, never anything the device says about
/// why: every failure reaches the phone as the same `CouldNotRead`, so the
/// escalation adds nothing to the wire and the device has nothing to steer.
/// The key is the restore's target (the wallet, the restoration, or a new
/// wallet; [restoreFailureKey]), so a count survives the page closing and a
/// different device being used with this phone; another phone, or another
/// install of the app, starts again at step 1 (owner B14). It also keeps which
/// devices failed (the app's own device ids), so offline recovery waits for a
/// second device (failflow-review M2), and the user's answers to steps 2 and
/// 3 (owner B14). A card that reads clears it all.
///
/// With a [file] (the app's, [loadFrom]) every change is written to it, and the
/// next start of the app reads it back. Only [String] keys are kept there,
/// which [restoreFailureKey] gives; a test's or the preview's other keys stay
/// in memory.
class RestoreFailures {
  final _counts = <Object, int>{};
  final _devices = <Object, Set<String>>{};
  final _triedOtherApp = <Object>{};
  final _noOtherDevice = <Object>{};
  File? _file;

  /// Reads the counts this app install kept in [file], and keeps every change
  /// there from now on. A file that's missing or unreadable is a fresh start.
  Future<void> loadFrom(File file) async {
    _file = file;
    try {
      if (!await file.exists()) return;
      final json = jsonDecode(await file.readAsString());
      if (json is! Map || json['v'] != 1 || json['entries'] is! List) return;
      for (final entry in json['entries'] as List) {
        if (entry is! Map || entry['key'] is! String) continue;
        final key = entry['key'] as String;
        final count = entry['count'];
        if (count is int && count > 0) _counts[key] = count;
        final devices = entry['devices'];
        if (devices is List) {
          _devices[key] = {...devices.whereType<String>()};
        }
        if (entry['triedOtherApp'] == true) _triedOtherApp.add(key);
        if (entry['noOtherDevice'] == true) _noOtherDevice.add(key);
      }
    } catch (_) {
      // A fresh start: the count only ever moves the user on more slowly.
    }
  }

  void _save() {
    final file = _file;
    if (file == null) return;
    final keys = {
      ..._counts.keys,
      ..._devices.keys,
      ..._triedOtherApp,
      ..._noOtherDevice,
    }.whereType<String>();
    final json = {
      'v': 1,
      'entries': [
        for (final key in keys)
          {
            'key': key,
            'count': _counts[key] ?? 0,
            'devices': [...?_devices[key]],
            'triedOtherApp': _triedOtherApp.contains(key),
            'noOtherDevice': _noOtherDevice.contains(key),
          },
      ],
    };
    try {
      file.writeAsStringSync(jsonEncode(json), flush: true);
    } catch (_) {
      // Kept in memory for this run of the app.
    }
  }

  /// One more failure for [key], on [device]; returns the count.
  int failed(Object key, {Object device = defaultRestoreDevice}) {
    (_devices[key] ??= {}).add(
      device is DeviceId ? device.toHex() : device.toString(),
    );
    final count = _counts[key] = (_counts[key] ?? 0) + 1;
    _save();
    return count;
  }

  /// A card read for [key]: its count goes back to zero.
  void read(Object key) {
    _counts.remove(key);
    _devices.remove(key);
    _triedOtherApp.remove(key);
    _noOtherDevice.remove(key);
    _save();
  }

  int count(Object key) => _counts[key] ?? 0;

  /// How many different devices a read failed on for [key].
  int devices(Object key) => _devices[key]?.length ?? 0;

  /// Step 2's button: "I've already tried it in a different app."
  void triedOtherApp(Object key) {
    _triedOtherApp.add(key);
    _save();
  }

  /// Step 3's button: "I don't have another Frostsnap device".
  void noOtherDevice(Object key) {
    _noOtherDevice.add(key);
    _save();
  }

  /// Where [key]'s failures have got to.
  RestoreFailureStep step(Object key) => restoreFailureStep(
    count(key),
    devices: devices(key),
    triedOtherApp: _triedOtherApp.contains(key),
    noOtherDevice: _noOtherDevice.contains(key),
  );
}

/// The device a page restores on when it isn't told (one device).
const defaultRestoreDevice = 'device';

/// The app's count, shared by every restore page; `main` loads it from this
/// app install's file before the first page.
final restoreFailures = RestoreFailures();

/// Which restore a failed read counts for: the wallet, the restoration, or a
/// new wallet (whose first card has no target yet). One key for a new wallet,
/// so trying its first card with another device carries the count on. A
/// string, so it's kept across app restarts.
String restoreFailureKey(RecoveryContext context) => switch (context) {
  AddingToWalletContext(:final accessStructureRef) =>
    'wallet:${accessStructureRef.keyId.toHex()}:'
        '${_hex(accessStructureRef.accessStructureId.field0)}',
  ContinuingRestorationContext(:final restorationId) =>
    'restoration:${restorationId.toHex()}',
  NewRestorationContext() => 'new wallet',
};

String _hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

/// What a failed read asks of the user (owner B14, 2 Nov 2026). A read that
/// fails here and then works elsewhere means the card was fine and this phone
/// or Frostsnap device was bad; a bad card fails everywhere, and only the
/// offline tool can help it. The masks can't say which word is wrong, so the
/// first step is a general re-check.
enum RestoreFailureStep {
  /// 1. The first failure: check the words and try again.
  checkWords,

  /// 2. Again: try the Frostsnap app on a different phone or computer. More
  /// failures in this app stay here; only the user's "I've already tried it
  /// in a different app." moves on.
  tryOtherApp,

  /// 3. The other app failed too: it could be the Frostsnap device, so try a
  /// different one and contact support.
  tryOtherDevice,

  /// 4. Failures on two Frostsnap devices, or the user has no other one:
  /// offline recovery with `card_offline_recover`, the command-line tool in
  /// `frost_backup` (spec_v3 PK.8's offline repair tool, owner B13).
  offlineRecovery,
}

/// Steps 1 and 2 by the count; step 3 only once the user says another app
/// failed too; step 4 from there once a read failed on two Frostsnap devices
/// this phone talked to, or the user has no other device. One restoring
/// device, which can fake failures on its own, can't send the user to step 4
/// unasked (failflow-review M2).
RestoreFailureStep restoreFailureStep(
  int failures, {
  int devices = 1,
  bool triedOtherApp = false,
  bool noOtherDevice = false,
}) {
  if (!triedOtherApp) {
    return failures <= 1
        ? RestoreFailureStep.checkWords
        : RestoreFailureStep.tryOtherApp;
  }
  return noOtherDevice || devices >= 2
      ? RestoreFailureStep.offlineRecovery
      : RestoreFailureStep.tryOtherDevice;
}

/// What the step asks (owner B14's copy). Line H goes with the steps that send
/// the user to someone or something outside this app: support, and the
/// offline tool (failflow-review M3).
String restoreFailureLine(RestoreFailureStep step) => switch (step) {
  RestoreFailureStep.checkWords =>
    "Couldn't read this card. Check the words you entered: words 1–32 on your "
        'device and words 33–64 here. Then try again.',
  RestoreFailureStep.tryOtherApp =>
    "Still couldn't read this card. Try the Frostsnap app on a different "
        'device, such as another phone or your laptop. Get it only from an '
        'official source.',
  RestoreFailureStep.tryOtherDevice =>
    "Still couldn't read this card. It could be the Frostsnap device. Try a "
        'different Frostsnap device, and contact Frostsnap support.'
        '\n\n$lineH $helpLine',
  RestoreFailureStep.offlineRecovery =>
    "This card still can't be read. Use offline recovery: it restores your "
        'key from all 64 words on one computer you trust, with no phone. Use '
        "only Frostsnap's own offline recovery tool, card_offline_recover.\n\n"
        "If that tool can't recover it either, $cantRecoverLine"
        '\n\n$lineH',
};

/// Owner B14: what's left when the offline tool can't recover the card
/// either, written for after "If that tool can't recover it either, ". Owner
/// follow-up 3: a general suspect line, since the app can't tell which device
/// wrote the card.
const cantRecoverLine =
    "this card can't be recovered. It may have been written wrong. You can "
    'still restore the wallet if you have enough other backups. Treat the '
    'Frostsnap device that wrote it as suspect.';

/// The button that moves a step on (owner B14), or null.
String? restoreFailureButton(RestoreFailureStep step) => switch (step) {
  RestoreFailureStep.tryOtherApp => "I've already tried it in a different app.",
  RestoreFailureStep.tryOtherDevice => "I don't have another Frostsnap device",
  _ => null,
};

/// A card that doesn't match this wallet (owner B11): it read, and its
/// self-checking masks passed, but s·G or the wallet-bound masks failed. Never
/// a failed read, so it neither counts toward the failure flow nor clears it.
/// Owner follow-up 4: a lying writer chip's card (honest self masks, bad
/// wallet-bound masks) gets this too, so the copy covers it.
const wrongWalletLine =
    "This card doesn't match this wallet. It may be another wallet's card, or "
    'it may have been written wrong.';

/// What to do with another wallet's card.
const wrongWalletNextLine = "Restore with this wallet's card.";

/// The status line for each [CardRestoreView] (design §5.3 E4). Null for
/// nothing extra. [step] is where the app's own count of failed reads for
/// this restore has got to, this one included ([RestoreFailures.step]).
String? restoreStatusLine(
  CardRestoreView view, {
  RestoreFailureStep step = RestoreFailureStep.checkWords,
}) => switch (view) {
  CardRestoreView_WaitingForDevice() => 'Connecting to your device…',
  CardRestoreView_EnterHalves(deviceHalfIn: true) =>
    'Words 1–32 are in on your device.',
  CardRestoreView_EnterHalves() => null,
  CardRestoreView_WaitingForDeviceHalf() =>
    'Sent. Now finish words 1–32 on your device.',
  CardRestoreView_Checking() =>
    'Checking your card on the device… (about 10 seconds)',
  // The same for words that don't fit and a card whose masks fail (owner
  // B7): the device takes no card it can't confirm, so a phone that changed
  // words 33–64 gets this too. Only the app's own count moves the line on.
  CardRestoreView_CouldNotRead() => restoreFailureLine(step),
  CardRestoreView_FinishOnDevice() =>
    'Your card was read. Finish on your device: read any warnings it shows, '
        'then hold to restore.',
  CardRestoreView_WrongWallet() => wrongWalletLine,
};

/// Step 6 of the recovery flow for a 64-word card (design §5.3 E1 to E6):
/// words 1–32 on the device, words 33–64 here, in either order. The device
/// decodes once both are in, and answers as it does for a 25-word backup.
/// An unplug keeps the words typed here; once the device is back the
/// restore starts again on it, and the words go to it again.
class EnterCardView extends StatefulWidget with TitledWidget {
  final String? deviceName;
  final CardRestoreDriver driver;
  final Function(PhysicalBackupPhase) onFinished;
  final VoidCallback onCancel;

  /// The restore ended without a card some other way than an unplug: the
  /// device refused, or was reset.
  final Function(String) onError;

  /// The word check for the entry; the bridge's by default.
  final WordLookup? lookup;

  /// Which restore the app's failure count is for (see [RestoreFailures]);
  /// this page alone by default.
  final Object? failureKey;

  /// The app's failure count; [restoreFailures] by default.
  final RestoreFailures? failures;

  /// The device this page restores on, for the count's devices (see
  /// [RestoreFailures]); [defaultRestoreDevice] by default.
  final Object failureDevice;

  const EnterCardView({
    super.key,
    required this.driver,
    required this.onFinished,
    required this.onCancel,
    required this.onError,
    this.deviceName,
    this.lookup,
    this.failureKey,
    this.failures,
    this.failureDevice = defaultRestoreDevice,
  });

  @override
  State<EnterCardView> createState() => _EnterCardViewState();

  @override
  String get titleText => 'Enter your card';
}

class _EnterCardViewState extends State<EnterCardView> {
  StreamSubscription<EnterPhysicalBackupState>? _subscription;
  late final StreamSubscription<bool> _connectedSubscription;
  bool _deviceConnected = true;

  CardRestoreView? _view;

  /// The words 33–64 last sent, kept for a restart after an unplug and for
  /// an edit.
  List<String?>? _sent;

  /// The entry is open (the user hasn't sent, or is editing).
  bool _editing = true;

  /// The device was unplugged: the restore waits for it.
  bool _unplugged = false;

  /// The current restore hasn't been given its words yet.
  bool _resendDue = false;

  /// The stream has given its one outcome; later states are ignored.
  bool _ended = false;

  late final RestoreFailures _failures = widget.failures ?? restoreFailures;
  late final Object _failureKey = widget.failureKey ?? Object();

  @override
  void initState() {
    super.initState();
    _start();
    _connectedSubscription = widget.driver.deviceConnected().listen((
      connected,
    ) {
      if (!mounted) return;
      _deviceConnected = connected;
      if (connected && _unplugged) {
        setState(() => _unplugged = false);
        _start();
      }
    });
  }

  void _start() {
    unawaited(_subscription?.cancel());
    _view = null;
    _resendDue = _sent != null;
    _subscription = widget.driver.start().listen(
      _onState,
      onError: (Object e) => _fail(displayExceptionMessage(e)),
    );
  }

  void _onState(EnterPhysicalBackupState state) {
    if (_ended || !mounted) return;
    final entered = state.entered;
    final abort = state.abort;
    if (entered != null) {
      _ended = true;
      _failures.read(_failureKey);
      unawaited(_subscription?.cancel());
      widget.onFinished(entered);
      return;
    }
    if (abort != null) {
      if (abort == _unpluggedAbort || !_deviceConnected) {
        unawaited(_subscription?.cancel());
        setState(() {
          _unplugged = true;
          _view = null;
        });
      } else {
        if (abort == alreadyHeldAbort) _failures.read(_failureKey);
        _fail(abort);
      }
      return;
    }
    // Nothing here logs a view (design §5.5).
    final view = state.cardRestore;
    // Each read that ends in "couldn't read" counts once, when the view
    // turns to it, also a re-check typed on the device alone (it decodes
    // again with no send from here); a card that reads clears the count.
    // Another wallet's card (owner B11) does neither: it isn't a failed
    // read, and it isn't the card this restore is for.
    if (view is CardRestoreView_CouldNotRead &&
        _view is! CardRestoreView_CouldNotRead) {
      _failures.failed(_failureKey, device: widget.failureDevice);
    } else if (view is CardRestoreView_FinishOnDevice) {
      _failures.read(_failureKey);
    }
    setState(() => _view = view);
    // The restore is on the stack now, so its handle takes the words.
    final sent = _sent;
    if (_resendDue && sent != null) {
      _resendDue = false;
      unawaited(_send(sent));
    }
  }

  Future<void> _send(List<String?> words) async {
    try {
      await widget.driver.sendPhoneHalf(words);
    } catch (e) {
      if (mounted) _fail(displayExceptionMessage(e));
    }
  }

  void _fail(String message) {
    if (_ended) return;
    _ended = true;
    unawaited(_subscription?.cancel());
    if (mounted) widget.onError(message);
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _connectedSubscription.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_view is CardRestoreView_WrongWallet && !_unplugged) {
      return _wrongWallet(context);
    }
    final content = SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          if (isCompactWindow(context)) CompactTitle(widget.titleText),
          ..._steps(context),
          ?_status(context),
          if (_editing)
            PhoneHalfEntry(
              key: const ValueKey('phone-half-entry'),
              lookup: widget.lookup,
              initial: _sent,
              sendLabel: _sent == null ? 'Send to device' : 'Send again',
              onSend: (words) {
                setState(() {
                  _sent = words;
                  _editing = false;
                });
                if (!_unplugged) unawaited(_send(words));
              },
            )
          else
            _sentSummary(context),
          TextButton(onPressed: widget.onCancel, child: const Text('Cancel')),
        ],
      ),
    );
    return LayoutBuilder(
      builder: (context, box) {
        final secured = SecureScreen(child: content);
        if (box.hasBoundedHeight) return secured;
        // On a desktop the recovery dialog sizes its views to their content;
        // the entry needs a height to scroll in.
        return SizedBox(
          height: math.max(
            0.0,
            math.min(MediaQuery.sizeOf(context).height - 96, 900.0),
          ),
          child: secured,
        );
      },
    );
  }

  // Owner B11: the card read, but it's another wallet's. The device's restore
  // ended on its own page, so there's nothing to edit here.
  Widget _wrongWallet(BuildContext context) {
    final theme = Theme.of(context);
    return SingleChildScrollView(
      key: const ValueKey('wrong-wallet'),
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          if (isCompactWindow(context)) CompactTitle(widget.titleText),
          Text("Doesn't match this wallet", style: theme.textTheme.titleLarge),
          CardBanner(color: cautionColor, text: wrongWalletLine),
          flowBody(context, wrongWalletNextLine),
          FilledButton(onPressed: widget.onCancel, child: const Text('Close')),
        ],
      ),
    );
  }

  // E1
  List<Widget> _steps(BuildContext context) {
    final theme = Theme.of(context);
    final deviceName = widget.deviceName;
    return [
      Text(
        deviceName == null
            ? '1. Enter words 1–32 on your device.'
            : '1. Enter words 1–32 on your device $deviceName.',
        style: theme.textTheme.bodyLarge,
      ),
      Text('2. Enter words 33–64 here.', style: theme.textTheme.bodyLarge),
      Text(
        neverTypeDeviceHalf,
        style: theme.textTheme.bodyLarge?.copyWith(fontWeight: FontWeight.bold),
      ),
    ];
  }

  // E4, and the unplug of E6.
  Widget? _status(BuildContext context) {
    final theme = Theme.of(context);
    if (_unplugged) {
      return CardBanner(
        color: cautionColor,
        text:
            'Your device was unplugged. Plug it back in to continue; the '
            'words you typed here are kept. Enter words 1–32 on it again.',
      );
    }
    final view = _view;
    final step = _failures.step(_failureKey);
    final line = view == null ? null : restoreStatusLine(view, step: step);
    if (line == null) return null;
    final checking = view is CardRestoreView_Checking;
    if (view is CardRestoreView_CouldNotRead) {
      final banner = CardBanner(color: theme.colorScheme.error, text: line);
      final button = restoreFailureButton(step);
      if (button == null) return banner;
      // Owner B14: only the user's answer moves steps 2 and 3 on.
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 8,
        children: [
          banner,
          OutlinedButton(
            key: ValueKey('failure-step-${step.name}'),
            onPressed: () => setState(
              () => step == RestoreFailureStep.tryOtherApp
                  ? _failures.triedOtherApp(_failureKey)
                  : _failures.noOtherDevice(_failureKey),
            ),
            child: Text(button, textAlign: TextAlign.center),
          ),
        ],
      );
    }
    return Row(
      spacing: 12,
      children: [
        if (checking || view is CardRestoreView_WaitingForDevice)
          const SizedBox.square(
            dimension: 20,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        Expanded(child: Text(line, style: theme.textTheme.bodyMedium)),
      ],
    );
  }

  Widget _sentSummary(BuildContext context) {
    final failed = _view is CardRestoreView_CouldNotRead;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      spacing: 8,
      children: [
        flowBody(
          context,
          _unplugged
              ? 'Words 33–64 go to your device once it is back.'
              : 'Words 33–64 are entered here.',
        ),
        OutlinedButton(
          key: const ValueKey('edit-half'),
          onPressed: () => setState(() => _editing = true),
          child: Text(failed ? 'Edit words 33–64' : 'Change words 33–64'),
        ),
      ],
    );
  }
}

bool isCompactWindow(BuildContext context) =>
    WindowSizeContext.of(context) == WindowSizeClass.compact;

/// The view's title on phones. There the recovery flow draws no top bar
/// (`WalletRecoveryFlow` drops it on compact screens), so these views show
/// their own; the status bar keeps its space above it.
class CompactTitle extends StatelessWidget {
  final String text;

  const CompactTitle(this.text, {super.key});

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      bottom: false,
      child: Text(text, style: Theme.of(context).textTheme.headlineSmall),
    );
  }
}
