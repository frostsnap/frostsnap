import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_setup_step.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/hex.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/network_advanced_options.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/threshold_selector.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/keygen.dart';
import 'package:frostsnap/src/rust/api/name.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:rxdart/rxdart.dart';
import 'global.dart';

/// Smallest strict majority of `totalDevices` — the recommended / default
/// signing threshold for a new wallet.
int recommendedThresholdFor(int totalDevices) => (totalDevices ~/ 2) + 1;

class WalletCreateException implements Exception {
  final String message;
  WalletCreateException(this.message);

  @override
  String toString() => 'WalletCreateException: $message';
}

class WalletCreateForm {
  BitcoinNetwork network = BitcoinNetwork.bitcoin;
  String? name;

  final Set<DeviceId> selectedDevices = deviceIdSet([]);

  int? threshold;
}

class WalletCreateController extends ChangeNotifier {
  WalletCreateStep _step = WalletCreateStep.values.first;
  final WalletCreateForm _form = WalletCreateForm();
  final _nameController = TextEditingController();
  String? _nameError;
  final DeviceSetupController _deviceSetup = DeviceSetupController();

  bool _hasAutoAdvanced = false;
  ValueStream<NonceReplenishState>? _nonceStream;

  KeyGenState? _keygenState;
  FullscreenActionDialogController? _keygenController;
  AccessStructureRef? _asRef;

  WalletCreateController() {
    bool firstRun = true;
    _nameController.addListener(() {
      final name = _nameController.text;
      if (!firstRun) {
        if (name.isEmpty) {
          nameError = 'Wallet name required';
          return;
        }
        if (name.length > keyNameMaxLength()) {
          nameError = 'Wallet name cannot be over ${keyNameMaxLength()} chars';
          return;
        }
      } else if (name.isNotEmpty) {
        firstRun = false;
        notifyListeners();
      }
      nameError = null;
    });
    // Re-broadcast device-setup changes so existing listeners that
    // watch `WalletCreateController` pick them up (canGoNext/nextText
    // both depend on the connected device list).
    _deviceSetup.addListener(notifyListeners);
  }

  DeviceSetupController get deviceSetup => _deviceSetup;

  FullscreenActionDialogController _buildKeygenController(
    BuildContext context,
    List<DeviceId> devices,
  ) {
    return FullscreenActionDialogController(
      context: context,
      devices: devices,
      title: 'Security Check',
      body: (context) => ListenableBuilder(
        listenable: this,
        builder: (context, _) {
          final theme = Theme.of(context);
          final state = _keygenState;
          if (state == null) return const SizedBox();

          final sessionHash = state.sessionHash;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.center,
            spacing: 12,
            children: [
              const Text(
                'Check that this code is identical and matches on every device',
                textAlign: TextAlign.center,
              ),
              Card.filled(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: AnimatedCrossFade(
                    firstChild: Padding(
                      padding: const EdgeInsets.all(8),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        spacing: 12,
                        children: [
                          CircularProgressIndicator(),
                          Text(
                            'This can take a few seconds...',
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ),
                    secondChild: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          '${form.threshold}-of-${form.selectedDevices.length}',
                          style: theme.textTheme.labelLarge,
                        ),
                        Text(
                          keygenChecksum,
                          style: theme.textTheme.headlineLarge?.copyWith(
                            fontFamily: monospaceTextStyle.fontFamily,
                          ),
                        ),
                      ],
                    ),
                    crossFadeState: sessionHash == null
                        ? CrossFadeState.showFirst
                        : CrossFadeState.showSecond,
                    duration: Durations.medium1,
                  ),
                ),
              ),
              Text(
                'The security check code confirms that all devices have behaved honestly during key generation.',
                textAlign: TextAlign.center,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          );
        },
      ),
      actionButtons: [
        OutlinedButton(onPressed: _onCancel, child: Text('Cancel')),
        ListenableBuilder(
          listenable: this,
          builder: (context, _) {
            final theme = Theme.of(context);
            final state = _keygenState;
            if (state == null) return const SizedBox();
            return Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 12,
              children: [
                Text(
                  'Confirm on device',
                  style: theme.textTheme.labelMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
                LargeCircularProgressIndicator(
                  size: 36,
                  progress: state.sessionAcks.length,
                  total: state.devices.length,
                ),
              ],
            );
          },
        ),
      ],
    );
  }

  @override
  void dispose() {
    _nameController.dispose();
    _deviceSetup.removeListener(notifyListeners);
    _deviceSetup.dispose();
    // Null the field first so the page's `_beginThresholdKeygen` finally (if
    // it's racing) doesn't double-dispose via its `identical` check.
    final keygenController = _keygenController;
    _keygenController = null;
    keygenController?.dispose();
    super.dispose();
  }

  @override
  void notifyListeners() {
    if (hasListeners) super.notifyListeners();
  }

  void _onCancel() async {
    await coord.cancelProtocol();
  }

  Future<void> resetKeygenState() async {
    await _keygenController?.clearAllActionsNeeded();
    _keygenState = null;
    await _deviceSetup.resendNamePreviews();
    notifyListeners();
  }

  WalletCreateForm get form => _form;
  WalletCreateStep get step => _step;
  KeyGenState? get keygenState => _keygenState;
  bool get keygenComplete => _keygenState?.allAcks ?? false;

  String get keygenChecksum => toSpacedHex(
    Uint8List.fromList(
      keygenState?.sessionHash?.field0.sublist(0, 4) ?? [0, 0, 0, 0],
    ),
  );

  TextEditingController get nameController => _nameController;
  String? get nameError => _nameError;
  set nameError(String? errorStr) {
    if (errorStr == _nameError) return;
    _nameError = errorStr;
    notifyListeners();
  }

  int get connectedDeviceCount => _deviceSetup.connectedDeviceCount;
  bool get allWalletDevicesConnected => _form.selectedDevices.every(
    (selectedId) =>
        _deviceSetup.devices.any((dev) => deviceIdEquals(dev.id, selectedId)),
  );
  bool get devicesNeedNonceReplenishment {
    final nonceRequest = coord.createNonceRequest(
      devices: _form.selectedDevices.toList(),
    );
    return nonceRequest.someNoncesRequested();
  }

  Future<bool> _shouldShowNonceStep() async {
    final devices = _form.selectedDevices.toList();
    final nonceRequest = await coord.createNonceRequest(devices: devices);
    return nonceRequest.someNoncesRequested();
  }

  bool get canGoNext => switch (_step) {
    WalletCreateStep.name =>
      _nameError == null && _nameController.value.text.isNotEmpty,
    WalletCreateStep.devices => _deviceSetup.ready,
    WalletCreateStep.nonceReplenish => false, // Auto-advances, no manual next
    WalletCreateStep.threshold =>
      allWalletDevicesConnected &&
          _form.threshold != null &&
          _form.threshold! > 0 &&
          _form.threshold! <= _form.selectedDevices.length,
  };
  bool get canGoBack => _step.index != 0;

  bool setNetwork(BitcoinNetwork network) {
    if (_asRef != null) return false;
    _form.network = network;
    notifyListeners();
    return true;
  }

  bool _isAnimationForward = true;
  bool get isAnimationForward => _isAnimationForward;

  /// Does additional checks (maybe) and tries to populate the _form.
  Future<bool> _handleNext(BuildContext context) async {
    _isAnimationForward = true;
    // Skip canGoNext check for nonceReplenish since it auto-advances
    if (_step != WalletCreateStep.nonceReplenish && !canGoNext) return false;
    switch (_step) {
      case WalletCreateStep.name:
        _form.name = _nameController.text.trim();
        return true;
      case WalletCreateStep.devices:
        _form.selectedDevices.clear();
        _form.selectedDevices.addAll(_deviceSetup.devices.map((dev) => dev.id));
        final needsNonces = await _shouldShowNonceStep();
        if (needsNonces) {
          final devices = _form.selectedDevices.toList();
          final nonceRequest = await coord.createNonceRequest(devices: devices);
          _nonceStream = coord
              .replenishNonces(nonceRequest: nonceRequest, devices: devices)
              .toBehaviorSubject();
          _hasAutoAdvanced = false;
        }
        return true;
      case WalletCreateStep.nonceReplenish:
        return true;
      case WalletCreateStep.threshold:
        // Keygen is driven by `_WalletCreatePageState._beginThresholdKeygen`
        // directly from the Next button, not through `next()`. Assert so
        // we trip in debug if someone wires it back up, then no-op.
        assert(false, 'threshold keygen is driven by the page, not next()');
        return false;
    }
  }

  void next(BuildContext context) async {
    if (_step == WalletCreateStep.devices &&
        connectedDeviceCount == 1 &&
        canGoNext) {
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Only one device'),
          content: const Text(
            'Make sure you\'ve connected all the devices you want to include '
            'in this wallet.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Go back'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Continue anyway'),
            ),
          ],
        ),
      );
      if (confirmed != true || !context.mounted) return;
    }

    if (!await _handleNext(context)) {
      return;
    }
    if (!context.mounted) {
      return;
    }

    WalletCreateStep? nextStep;
    if (_step == WalletCreateStep.devices) {
      if (_nonceStream == null) {
        nextStep = WalletCreateStep.threshold;
      } else {
        nextStep = WalletCreateStep.nonceReplenish;
      }
    } else if (_step == WalletCreateStep.nonceReplenish) {
      nextStep = WalletCreateStep.threshold;
    } else {
      nextStep = WalletCreateStep.values.elementAtOrNull(_step.index + 1);
    }

    if (nextStep != null) {
      if (nextStep == WalletCreateStep.threshold) {
        // Seed the default threshold eagerly on transition so `canGoNext` is
        // true as soon as the threshold step first renders.
        final totalCount = _form.selectedDevices.length;
        if (totalCount > 0) {
          _form.threshold ??= recommendedThresholdFor(totalCount);
        }
      }
      _step = nextStep;
      notifyListeners();
    }
  }

  bool _handleBack(BuildContext context) {
    _isAnimationForward = false;
    switch (_step) {
      case _:
        return true;
    }
  }

  void back(context) {
    if (!_handleBack(context)) return;

    WalletCreateStep? prevStep;
    if (_step == WalletCreateStep.threshold ||
        _step == WalletCreateStep.nonceReplenish) {
      // Skip nonce step on the way back since nonce generation is automatic
      // and shouldn't be re-shown. Clear the stream so it can be re-generated.
      prevStep = WalletCreateStep.devices;
      _nonceStream = null;
      _hasAutoAdvanced = false;
    } else {
      final prevIndex = _step.index - 1;
      prevStep = WalletCreateStep.values.elementAtOrNull(prevIndex);
    }

    if (prevStep != null) {
      _step = prevStep;
      notifyListeners();
    }
  }

  String? get backText => switch (_step) {
    WalletCreateStep.name => 'Close',
    _ => null,
  };

  String? get nextText => switch (_step) {
    WalletCreateStep.name => null,
    WalletCreateStep.devices => switch (_deviceSetup.connectedDeviceCount) {
      1 => 'Continue with 1 device',
      final n => 'Continue with $n devices',
    },
    WalletCreateStep.nonceReplenish => null,
    WalletCreateStep.threshold => 'Generate keys',
  };

  String get title => switch (_step) {
    WalletCreateStep.name => 'Name wallet',
    WalletCreateStep.devices => 'Add devices',
    WalletCreateStep.nonceReplenish => "Preparing devices",
    WalletCreateStep.threshold => 'Choose threshold',
  };

  String get subtitle => switch (_step) {
    WalletCreateStep.name => 'Choose a name for this wallet',
    WalletCreateStep.devices =>
      'Connect all devices you want to hold a key in "${form.name ?? ''}".\nGive each a name you will recognise later.',
    WalletCreateStep.nonceReplenish => '',
    WalletCreateStep.threshold => '',
  };

  Future<void> setDeviceName(DeviceId id, String name) =>
      _deviceSetup.setDeviceName(id, name);
}

enum WalletCreateStep { name, devices, nonceReplenish, threshold }

/// Shows a fullscreen dialog instructing the user to unplug all currently
/// connected devices. Returns when every device that was connected at call
/// time has been disconnected. No-op if nothing is connected.
Future<void> showUnplugDevicesDialog(BuildContext context) async {
  final deviceListUpdate = await GlobalStreams.deviceListSubject.first;
  final connectedDevices = deviceListUpdate.state.devices;
  if (connectedDevices.isEmpty) return;

  final controller = FullscreenActionDialogController<void>(
    context: context,
    devices: connectedDevices.map((d) => d.id).toList(),
    title: 'Wallet created!',
    body: (context) =>
        Text('Unplug devices to continue', textAlign: TextAlign.center),
    actionButtons: [
      DeviceActionHint(label: 'Unplug devices', icon: Icons.usb_off),
    ],
    onDismissed: () {},
  );
  try {
    await controller.awaitDismissed();
  } finally {
    controller.dispose();
  }
}

class WalletCreatePage extends StatefulWidget {
  const WalletCreatePage({super.key});

  @override
  State<WalletCreatePage> createState() => _WalletCreatePageState();
}

class _WalletCreatePageState extends State<WalletCreatePage> {
  late WalletCreateController _controller;
  bool _keygenInFlight = false;

  @override
  void initState() {
    super.initState();
    _controller = WalletCreateController();
    _controller.addListener(() => mounted ? setState(() {}) : null);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Widget buildWalletNameBody(BuildContext context) {
    return SliverToBoxAdapter(
      child: TextField(
        autofocus: true,
        controller: _controller.nameController,
        decoration: InputDecoration(
          border: OutlineInputBorder(),
          errorText: _controller.nameError,
        ),
        maxLength: keyNameMaxLength(),
        inputFormatters: [nameInputFormatter],
        textCapitalization: TextCapitalization.words,
        onSubmitted: (_) {
          _controller.next(context);
        },
      ),
    );
  }

  Widget buildDevicesBody(BuildContext context) =>
      DeviceSetupView(controller: _controller.deviceSetup);

  Widget buildThresholdBody(BuildContext context) {
    final form = _controller.form;
    final totalCount = form.selectedDevices.length;
    assert(totalCount > 0);
    // `form.threshold` is seeded by `WalletCreateController.next()` when it
    // transitions into the threshold step; just read it here.
    return SliverList.list(
      children: [
        ThresholdSelector(
          threshold: form.threshold!,
          totalDevices: totalCount,
          recommendedThreshold: recommendedThresholdFor(totalCount),
          onChanged: (value) => setState(() => form.threshold = value),
        ),
        if (!_controller.allWalletDevicesConnected)
          buildDisconnectedWarningCard(context),
      ],
    );
  }

  /// Runs the full keygen flow: starts the Rust keygen stream, pumps each
  /// state into the controller (for reactive rebuilds of the dialog body
  /// and footer), shows the Final-check alert when all devices have ack'd,
  /// and on a matching code finalizes the keygen and pops the page with
  /// the resulting AccessStructureRef.
  ///
  /// This lives on the page (not `WalletCreateController.next()`) because
  /// the keygen step is driven directly by the "Generate keys" button —
  /// advancing past the threshold step IS running keygen. `next()` has no
  /// threshold case; the button skips `next()` and calls this instead.
  Future<void> _beginThresholdKeygen(BuildContext context) async {
    if (_keygenInFlight) return;
    setState(() => _keygenInFlight = true);
    try {
      final form = _controller.form;
      final selectedDevices = form.selectedDevices.toList();
      final stream = coord
          .generateNewKey(
            threshold: form.threshold!,
            devices: selectedDevices,
            keyName: form.name!,
            network: form.network,
          )
          .toBehaviorSubject();

      // Dismiss any leftover keygen dialog from a previous attempt before
      // spinning up a new one. Awaits the dismissal animation so the new
      // dialog doesn't stack on top of the old one.
      final previous = _controller._keygenController;
      _controller._keygenController = null;
      await previous?.clearAllActionsNeeded();
      previous?.dispose();

      final keygenController = _controller._buildKeygenController(
        context,
        selectedDevices,
      );
      _controller._keygenController = keygenController;

      try {
        await for (final state in stream) {
          _controller._keygenState = state;
          _controller.notifyListeners();

          for (final id in state.sessionAcks) {
            await keygenController.removeActionNeeded(id);
          }

          if (state.aborted != null) {
            await _controller.resetKeygenState();
            return;
          }

          if (!state.allAcks) continue;

          final keygenCodeMatches =
              await showDialog<bool>(
                context: context,
                barrierDismissible: false,
                builder: (context) {
                  final theme = Theme.of(context);
                  return BackdropFilter(
                    filter: blurFilter,
                    child: AlertDialog(
                      title: Text('Final check'),
                      content: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        spacing: 16,
                        children: [
                          Text('Do all devices show this code?'),
                          Card.filled(
                            child: Center(
                              child: Padding(
                                padding: EdgeInsets.symmetric(
                                  vertical: 12,
                                  horizontal: 16,
                                ),
                                child: Column(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    Text(
                                      '${form.threshold}-of-${form.selectedDevices.length}',
                                      style: theme.textTheme.labelLarge,
                                    ),
                                    Text(
                                      _controller.keygenChecksum,
                                      style: theme.textTheme.headlineLarge
                                          ?.copyWith(
                                            fontFamily:
                                                monospaceTextStyle.fontFamily,
                                          ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                      actionsAlignment: MainAxisAlignment.spaceBetween,
                      actions: [
                        TextButton(
                          onPressed: () => Navigator.pop(context, false),
                          child: Text('No'),
                        ),
                        TextButton(
                          onPressed: () => Navigator.pop(context, true),
                          child: Text('Yes'),
                        ),
                      ],
                    ),
                  );
                },
              ) ??
              false;
          if (!keygenCodeMatches) {
            _controller._keygenState = null;
            _controller.notifyListeners();
            return;
          }

          try {
            final encryptionKey = await SecureKeyProvider.getEncryptionKey();
            final asRef = await coord.finalizeKeygen(
              keygenId: state.keygenId,
              encryptionKey: encryptionKey,
            );
            _controller._asRef = asRef;
            if (context.mounted) Navigator.pop(context, asRef);
          } catch (e) {
            _controller._keygenState = null;
            _controller.notifyListeners();
            if (context.mounted) {
              showErrorSnackbar(context, 'Failed to finalize keygen: $e');
            }
          }
          return;
        }
      } finally {
        // Only dispose if we're still the active keygen controller. The
        // field could have been nulled out from under us by
        // `WalletCreateController.dispose()` or by a follow-up call to
        // `_beginThresholdKeygen` that swapped in a fresh controller.
        if (identical(_controller._keygenController, keygenController)) {
          _controller._keygenController = null;
          keygenController.dispose();
        }
      }
    } finally {
      if (mounted) {
        setState(() => _keygenInFlight = false);
      } else {
        _keygenInFlight = false;
      }
    }
  }

  Widget buildDisconnectedWarningCard(BuildContext context) => Card.outlined(
    margin: EdgeInsets.symmetric(vertical: 16),
    child: ListTile(
      dense: true,
      contentPadding: EdgeInsets.symmetric(horizontal: 16),
      leading: Icon(Icons.warning_rounded),
      title: Text(
        'One or more devices have been disconnected. Reconnect to continue.',
      ),
    ),
  );

  void _resetNonceReplenishStep() {
    coord.cancelProtocol();
    _controller._nonceStream = null;
    _controller._step = WalletCreateStep.devices;
    _controller.notifyListeners();
  }

  Widget buildNonceReplenish(BuildContext context) {
    final theme = Theme.of(context);

    // Use the pre-initialized stream
    final stream = _controller._nonceStream;
    if (stream == null) {
      // This shouldn't happen as we skip the step when no nonces are needed
      // But if it does, auto-advance immediately
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && !_controller._hasAutoAdvanced) {
          _controller._hasAutoAdvanced = true;
          _controller.next(context);
        }
      });
      return SliverToBoxAdapter(
        child: Padding(
          padding: EdgeInsets.symmetric(vertical: 32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 24),
              Text('Initializing...', style: theme.textTheme.bodyLarge),
            ],
          ),
        ),
      );
    }

    return SliverToBoxAdapter(
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 32),
        child: NonceReplenishIndicator(
          stream: stream,
          onTerminal: (terminal) {
            switch (terminal) {
              case NonceReplenishCompleted():
                if (mounted && !_controller._hasAutoAdvanced) {
                  _controller._hasAutoAdvanced = true;
                  _controller.next(context);
                }
                break;
              case NonceReplenishAborted():
              case NonceReplenishFailed():
                if (mounted) _resetNonceReplenishStep();
                break;
            }
          },
        ),
      ),
    );
  }

  Widget buildBody(BuildContext context) {
    switch (_controller.step) {
      case WalletCreateStep.name:
        return buildWalletNameBody(context);
      case WalletCreateStep.devices:
        return buildDevicesBody(context);
      case WalletCreateStep.nonceReplenish:
        return buildNonceReplenish(context);
      case WalletCreateStep.threshold:
        return buildThresholdBody(context);
    }
  }

  @override
  Widget build(BuildContext context) {
    final network = _controller.form.network;
    final appBarTrailingText = network.isMainnet()
        ? ''
        : ' (${network.name()})';

    final isAnimationForward = _controller.isAnimationForward;
    final step = _controller.step;

    final animatedStep = MultiStepDialogSwitcher(
      forward: isAnimationForward,
      // Outgoing steps dispose immediately so per-step streams (e.g.
      // `nonceReplenish`) don't keep running during the slide.
      reverseDuration: Duration.zero,
      child: FullscreenDialogBody(
        key: ValueKey<WalletCreateStep>(step),
        title: Text('${_controller.title}$appBarTrailingText'),
        subtitle: _controller.subtitle.isEmpty ? null : _controller.subtitle,
        leading: IconButton(
          icon: const Icon(Icons.arrow_back_rounded),
          onPressed: () => goBackOrClose(context),
          tooltip: 'Back',
        ),
        body: buildBody(context),
      ),
    );

    final column = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(child: animatedStep),
        if (step != WalletCreateStep.nonceReplenish)
          Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (SettingsContext.of(context)?.settings.isInDeveloperMode() ??
                  false)
                buildAdvancedOptions(context),
              FullscreenDialogFooter(
                child: Align(
                  alignment: Alignment.centerRight,
                  child: FilledButton(
                    onPressed:
                        !_controller.canGoNext ||
                            (step == WalletCreateStep.threshold &&
                                _keygenInFlight)
                        ? null
                        : () {
                            if (step == WalletCreateStep.threshold) {
                              _beginThresholdKeygen(context);
                            } else {
                              _controller.next(context);
                            }
                          },
                    child: Text(
                      _controller.nextText ?? 'Next',
                      softWrap: false,
                      overflow: TextOverflow.fade,
                    ),
                  ),
                ),
              ),
            ],
          ),
      ],
    );

    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) return;
        goBackOrClose(context);
      },
      child: column,
    );
  }

  void goBackOrClose(BuildContext context) {
    if (_controller.canGoBack) {
      _controller.back(context);
    } else {
      Navigator.pop(context, null);
    }
  }

  void close(BuildContext context) {
    Navigator.pop(context, null);
  }

  Widget buildAdvancedOptions(BuildContext context) {
    return NetworkAdvancedOptions(
      selected: _controller.form.network,
      onChanged: (n) => _controller.setNetwork(n),
    );
  }
}

class LargeCircularProgressIndicator extends StatefulWidget {
  final int progress;
  final int total;
  final double size;

  const LargeCircularProgressIndicator({
    super.key,
    required this.progress,
    required this.total,
    this.size = 70,
  });

  @override
  State<LargeCircularProgressIndicator> createState() =>
      _LargeCircularProgressIndicatorState();
}

class _LargeCircularProgressIndicatorState
    extends State<LargeCircularProgressIndicator>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late Animation<double> _animation;
  double _oldFraction = 0;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 600),
    );
    _initAnimation();
  }

  void _initAnimation() {
    final newFraction = widget.total == 0
        ? 0.0
        : (widget.progress / widget.total).clamp(0.0, 1.0);
    _animation = Tween<double>(begin: _oldFraction, end: newFraction).animate(
      CurvedAnimation(parent: _controller, curve: Curves.easeOutCubic),
    )..addListener(() => setState(() {}));
    _controller.forward(from: 0);
    _oldFraction = newFraction;
  }

  @override
  void didUpdateWidget(covariant LargeCircularProgressIndicator oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.progress != widget.progress ||
        oldWidget.total != widget.total) {
      _initAnimation();
    }
  }

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final complete = widget.total > 0 && widget.progress >= widget.total;
    final fraction = complete ? 1.0 : _animation.value;

    return UnconstrainedBox(
      child: SizedBox.square(
        dimension: widget.size,
        child: Stack(
          alignment: Alignment.center,
          children: [
            AspectRatio(
              aspectRatio: 1,
              child: CircularProgressIndicator(
                value: fraction,
                strokeWidth: widget.size * 0.07,
                backgroundColor: cs.surfaceContainerHighest,
                color: cs.primary,
              ),
            ),
            complete
                ? Icon(Icons.check, size: widget.size * 0.5, color: cs.primary)
                : SizedBox(
                    width: widget.size * 0.6,
                    height: widget.size * 0.6,
                    child: FittedBox(
                      fit: BoxFit.scaleDown,
                      child: Text(
                        '${widget.progress}/${widget.total}',
                        style: Theme.of(context).textTheme.titleLarge,
                        textAlign: TextAlign.center,
                      ),
                    ),
                  ),
          ],
        ),
      ),
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }
}
