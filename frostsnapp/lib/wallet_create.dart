import 'dart:async';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_action_upgrade.dart';
import 'package:frostsnap/hex.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/keygen.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'package:sliver_tools/sliver_tools.dart';
import 'global.dart';
import 'maybe_fullscreen_dialog.dart';
import 'wallet_device_list.dart';

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
  final Map<DeviceId, String> deviceNames = deviceIdMap<String>();

  int? threshold;

  bool get allDevicesNamed =>
      selectedDevices.every((id) => deviceNames.containsKey(id));
}

class WalletCreateController extends ChangeNotifier {
  WalletCreateStep _step = WalletCreateStep.values.first;
  final WalletCreateForm _form = WalletCreateForm();
  final _nameController = TextEditingController();
  String? _nameError;
  late final StreamSubscription _deviceListSub;
  late DeviceListState _deviceList;

  bool _hasAutoAdvanced = false;
  Stream<NonceReplenishState>? _nonceStream;

  KeyGenState? _keygenState;
  late final FullscreenActionDialogController _keygenController;
  AccessStructureRef? _asRef;

  WalletCreateController() {
    {
      bool firstRun = true;
      _nameController.addListener(() {
        final name = _nameController.text;
        if (!firstRun) {
          if (name.isEmpty) {
            nameError = 'Wallet name required';
            return;
          }
          if (name.length > 21) {
            nameError = 'Wallet name cannot be over 21 chars';
            return;
          }
        } else if (name.isNotEmpty) {
          firstRun = false;
          notifyListeners();
        }
        nameError = null;
      });
    }
    {
      _deviceListSub = GlobalStreams.deviceListSubject.listen((update) {
        _deviceList = update.state;
        resetDeviceNames(update.state.devices);
        notifyListeners();
      });
    }
    _keygenController = FullscreenActionDialogController(
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
                'Confirm that this code is shown on all devices',
                textAlign: TextAlign.center,
              ),
              Card.filled(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: AnimatedCrossFade(
                    firstChild: const Padding(
                      padding: EdgeInsets.all(8),
                      child: CircularProgressIndicator(),
                    ),
                    secondChild: Text(
                      keygenChecksum,
                      style: theme.textTheme.headlineLarge?.copyWith(
                        fontFamily: monospaceTextStyle.fontFamily,
                      ),
                    ),
                    crossFadeState: sessionHash == null
                        ? CrossFadeState.showFirst
                        : CrossFadeState.showSecond,
                    duration: Durations.medium1,
                  ),
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
    _deviceListSub.cancel();
    for (final device in _deviceList.devices) {
      coord.sendCancel(id: device.id);
    }
    _keygenController.dispose();
    super.dispose();
  }

  @override
  void notifyListeners() {
    if (hasListeners) super.notifyListeners();
  }

  void _onCancel() async {
    await coord.cancelProtocol();
  }

  Future<void> resetDeviceNames(Iterable<ConnectedDevice> devices) async {
    for (final device in devices) {
      final id = device.id;
      final name = form.deviceNames[id];
      (name != null)
          ? await coord.updateNamePreview(id: id, name: name)
          : await coord.sendCancel(id: id);
    }
  }

  Future<void> resetKeygenState(Iterable<ConnectedDevice> devices) async {
    await _keygenController.clearAllActionsNeeded();
    _keygenState = null;
    await resetDeviceNames(_deviceList.devices);
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

  int get connectedDeviceCount => _deviceList.devices.length;
  bool get devicesNeedUpgrade =>
      _deviceList.devices.any((dev) => dev.needsFirmwareUpgrade());
  bool get devicesUsed => _deviceList.devices.any((dev) => dev.name != null);
  bool get allWalletDevicesConnected => _form.selectedDevices.every(
    (selectedId) =>
        _deviceList.devices.any((dev) => deviceIdEquals(dev.id, selectedId)),
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
    WalletCreateStep.deviceCount =>
      _deviceList.devices.isNotEmpty && !devicesNeedUpgrade && !devicesUsed,
    WalletCreateStep.nonceReplenish => false, // Auto-advances, no manual next
    WalletCreateStep.deviceNames =>
      allWalletDevicesConnected && _form.allDevicesNamed,
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
        _form.name = _nameController.text;
        return true;
      case WalletCreateStep.deviceCount:
        _form.selectedDevices.clear();
        _form.selectedDevices.addAll(_deviceList.devices.map((dev) => dev.id));
        // Check if nonces are needed after selecting devices
        final needsNonces = await _shouldShowNonceStep();
        if (needsNonces) {
          // Prepare the nonce stream for the next step
          final devices = _form.selectedDevices.toList();
          final nonceRequest = await coord.createNonceRequest(devices: devices);
          _nonceStream = coord
              .replenishNonces(nonceRequest: nonceRequest, devices: devices)
              .toBehaviorSubject();
          _hasAutoAdvanced = false; // Reset for this run
        }
        return true;
      case WalletCreateStep.nonceReplenish:
        return true;
      case WalletCreateStep.deviceNames:
        return true;
      case WalletCreateStep.threshold:
        final selectedDevices = form.selectedDevices.toList();
        final stream = coord
            .generateNewKey(
              threshold: form.threshold!,
              devices: selectedDevices,
              keyName: form.name!,
              network: form.network,
            )
            .toBehaviorSubject();
        for (final id in selectedDevices) {
          _keygenController.addActionNeeded(context, id);
        }
        await for (final state in stream) {
          _keygenState = state;
          notifyListeners();

          for (final id in state.sessionAcks) {
            await _keygenController.removeActionNeeded(id);
          }

          if (state.aborted != null) {
            await resetKeygenState(_deviceList.devices);
            return false;
          }

          if (state.allAcks) {
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
                                  child: Text(
                                    keygenChecksum,
                                    style: theme.textTheme.headlineLarge
                                        ?.copyWith(
                                          fontFamily:
                                              monospaceTextStyle.fontFamily,
                                        ),
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
            if (!keygenCodeMatches) return false;
            try {
              final encryptionKey = await SecureKeyProvider.getEncryptionKey();
              _asRef = await coord.finalizeKeygen(
                keygenId: state.keygenId,
                encryptionKey: encryptionKey,
              );
            } catch (Exception) {
              return false;
            }
            return true;
          }
        }
        throw StateError('Unreachable: keygen completions handled');
    }
  }

  void next(BuildContext context) async {
    if (!await _handleNext(context)) {
      return;
    }
    if (!context.mounted) {
      return;
    }

    // Determine next step, potentially skipping nonce replenishment
    WalletCreateStep? nextStep;
    if (_step == WalletCreateStep.deviceCount) {
      // Check if we should skip nonce replenishment
      if (_nonceStream == null) {
        // No nonces needed, skip to device names
        nextStep = WalletCreateStep.deviceNames;
      } else {
        // Nonces needed, go to nonce replenishment
        nextStep = WalletCreateStep.nonceReplenish;
      }
    } else if (_step == WalletCreateStep.nonceReplenish) {
      // After nonce replenishment, go to device names
      nextStep = WalletCreateStep.deviceNames;
    } else {
      // Normal progression
      nextStep = WalletCreateStep.values.elementAtOrNull(_step.index + 1);
    }
    
    if (nextStep != null) {
      _step = nextStep;
      notifyListeners();
    } else {
      Navigator.pop(context, _asRef);
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
    
    // Handle back navigation, skipping nonce step if it was skipped forward
    WalletCreateStep? prevStep;
    if (_step == WalletCreateStep.deviceNames) {
      // Check if we should skip nonce step when going back
      if (_nonceStream == null) {
        // Nonce step was skipped, go directly to deviceCount
        prevStep = WalletCreateStep.deviceCount;
      } else {
        // Nonce step was shown, go back to it
        prevStep = WalletCreateStep.nonceReplenish;
      }
    } else {
      // Normal back navigation
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
    WalletCreateStep.deviceCount => switch (_deviceList.devices.length) {
      1 => 'Continue with 1 device',
      _ => 'Continue with ${_deviceList.devices.length} devices',
    },
    WalletCreateStep.nonceReplenish => null,
    WalletCreateStep.deviceNames =>
      _form.allDevicesNamed ? null : 'Name all devices to continue',
    WalletCreateStep.threshold => 'Generate keys',
  };

  String get title => switch (_step) {
    WalletCreateStep.name => 'Name wallet',
    WalletCreateStep.deviceCount => 'Pick devices',
    WalletCreateStep.nonceReplenish => "Preparing devices",
    WalletCreateStep.deviceNames => 'Name devices',
    WalletCreateStep.threshold => 'Choose threshold',
  };

  String get subtitle => switch (_step) {
    WalletCreateStep.name => 'Choose a name for this wallet',
    WalletCreateStep.deviceCount =>
      'Connect devices to become keys for "${form.name ?? ''}"',
    WalletCreateStep.nonceReplenish => '',
    WalletCreateStep.deviceNames => 'Each device needs a name to idenitfy it',
    WalletCreateStep.threshold =>
      'Decide how many devices will be required to sign transactions or to make changes to this wallet',
  };

  void setDeviceName(DeviceId id, String name) async {
    if (name.isNotEmpty) {
      _form.deviceNames[id] = name;
      notifyListeners();
      await coord.updateNamePreview(id: id, name: name);
    } else {
      _form.deviceNames.remove(id);
      notifyListeners();
      await coord.sendCancel(id: id);
    }
  }
}

enum WalletCreateStep {
  name,
  deviceCount,
  nonceReplenish,
  deviceNames,
  threshold,
}

class WalletCreatePage extends StatefulWidget {
  const WalletCreatePage({super.key});

  @override
  State<WalletCreatePage> createState() => _WalletCreatePageState();
}

class _WalletCreatePageState extends State<WalletCreatePage> {
  static const topSectionPadding = EdgeInsets.fromLTRB(16, 0, 16, 16);
  static const sectionPadding = EdgeInsets.fromLTRB(16, 16, 16, 24);
  late WalletCreateController _controller;
  final _upgradeController = DeviceActionUpgradeController();

  @override
  void initState() {
    super.initState();
    _controller = WalletCreateController();
    _controller.addListener(() => mounted ? setState(() {}) : null);
  }

  @override
  void dispose() {
    // dispose all dynamic controllers
    for (final c in _nameControllers.values) {
      c.dispose();
    }
    _upgradeController.dispose();
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
        maxLength: 21,
        textCapitalization: TextCapitalization.words,
        onSubmitted: (_) {
          _controller.next(context);
        },
      ),
    );
  }

  Widget buildDevicesBody(BuildContext context) {
    final theme = Theme.of(context);
    final parentCtx = context;
    return MultiSliver(
      children: [
        SliverDeviceList(
          deviceBuilder: (context, device) => buildDevice(
            context,
            device,
            trailing: device.name != null
                ? buildDeviceTrailingInfo(
                    context,
                    text: 'Already holds a key',
                    subText: 'Unplug to continue',
                    icon: Icons.warning_rounded,
                    color: Theme.of(context).colorScheme.error,
                  )
                : device.needsFirmwareUpgrade()
                ? buildDeviceTrailingInfo(
                    context,
                    text: 'Old firmware',
                    subText: "Upgrade to continue",
                    icon: Icons.system_update_alt_rounded,
                    color: Colors.orange,
                  )
                : buildDeviceTrailingInfo(
                    context,
                    text: 'Ready',
                    icon: Icons.check_circle_rounded,
                    color: Colors.green,
                  ),
            onPressed: device.needsFirmwareUpgrade()
                ? () async => await _upgradeController.run(parentCtx)
                : null,
          ),
        ),

        if (_controller.devicesNeedUpgrade)
          SliverToBoxAdapter(
            child: Card.outlined(
              margin: EdgeInsets.symmetric(vertical: 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                mainAxisSize: MainAxisSize.min,
                children: [
                  ListTile(
                    dense: true,
                    contentPadding: EdgeInsets.symmetric(horizontal: 16),
                    title: Text(
                      'One or more devices require a firmware update before continuing.',
                    ),
                    leading: Icon(
                      Icons.system_update_alt_rounded,
                      color: Colors.orange,
                    ),
                    trailing: TextButton(
                      onPressed: () async =>
                          await _upgradeController.run(context),
                      child: Text('Start Upgrade'),
                    ),
                    onTap: () async => await _upgradeController.run(context),
                  ),
                ],
              ),
            ),
          ),
        SliverToBoxAdapter(
          child: AnimatedGradientBorder(
            stretchAlongAxis: true,
            borderSize: 1.0,
            glowSize: 4.0,
            animationTime: 6,
            borderRadius: BorderRadius.circular(12.0),
            gradientColors: [
              theme.colorScheme.outlineVariant,
              theme.colorScheme.primary,
              theme.colorScheme.secondary,
              theme.colorScheme.tertiary,
            ],
            child: Card(
              margin: EdgeInsets.zero,
              child: ListTile(
                dense: true,
                title: Text('Plug in devices to include them in this wallet.'),
                contentPadding: EdgeInsets.symmetric(horizontal: 16),
                leading: Icon(Icons.info_rounded),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget buildDevice(
    BuildContext context,
    ConnectedDevice device, {
    Widget? trailing,
    double? rightPadding,
    void Function()? onPressed,
    bool enabled = true,
  }) {
    final theme = Theme.of(context);
    return Card.filled(
      margin: EdgeInsets.symmetric(vertical: 4),
      color: theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: ListTile(
        title: Text(
          device.name ?? _controller.form.deviceNames[device.id] ?? '',
          style: monospaceTextStyle,
        ),
        leading: Icon(Icons.key),
        contentPadding: EdgeInsets.symmetric(
          horizontal: 16,
        ).copyWith(right: rightPadding),
        trailing: trailing,
        onTap: onPressed,
        enabled: enabled,
      ),
    );
  }

  Widget buildDeviceTrailingInfo(
    BuildContext context, {
    String? text,
    String? subText,
    IconData? icon,
    Color? color,
  }) => Row(
    mainAxisSize: MainAxisSize.min,
    spacing: 8,
    children: [
      Flexible(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            if (text != null)
              Text(
                text,
                style: Theme.of(
                  context,
                ).textTheme.titleSmall?.copyWith(color: color),
              ),
            if (subText != null)
              Text(
                subText,
                style: Theme.of(
                  context,
                ).textTheme.labelSmall?.copyWith(color: color),
              ),
          ],
        ),
      ),
      if (icon != null) Icon(icon, color: color),
    ],
  );

  final Map<DeviceId, TextEditingController> _nameControllers = deviceIdMap();

  void showRenameDeviceDialog(
    BuildContext context,
    ConnectedDevice device,
  ) async {
    await showBottomSheetOrDialog(
      context,
      title: Text("Name device"),
      builder: (context, _) {
        final mediaQuery = MediaQuery.of(context);
        return SafeArea(
          minimum: const EdgeInsets.symmetric(
            horizontal: 20,
          ).copyWith(bottom: 32 + mediaQuery.viewInsets.bottom, top: 32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            spacing: 12,
            children: [
              TextFormField(
                autofocus: true,
                decoration: InputDecoration(
                  border: OutlineInputBorder(),
                  labelText: 'Device Name',
                ),
                initialValue: _controller.form.deviceNames[device.id],
                onChanged: (name) => _controller.setDeviceName(device.id, name),
                onFieldSubmitted: (_) => Navigator.pop(context),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context),
                child: Text('Done'),
              ),
            ],
          ),
        );
      },
    );
  }

  Widget _buildDeviceForNaming(BuildContext context, ConnectedDevice device) {
    final form = _controller.form;
    final isPart = form.selectedDevices.contains(device.id);
    final currentName = form.deviceNames[device.id] ?? '';

    // obtain or create controller for this device
    final textController = _nameControllers.putIfAbsent(
      device.id,
      () => TextEditingController(text: currentName),
    );

    // keep text in sync when form updates
    if (textController.text != currentName) {
      textController.text = currentName;
    }

    return Card.filled(
      margin: EdgeInsets.symmetric(vertical: 4),
      color: Theme.of(context).colorScheme.surface,
      clipBehavior: Clip.hardEdge,
      child: ListTile(
        leading: Icon(Icons.key),
        contentPadding: EdgeInsets.symmetric(horizontal: 12),
        title: TextField(
          decoration: InputDecoration(
            hintText: 'Enter device name',
            border: OutlineInputBorder(
              borderSide: BorderSide.none,
              borderRadius: BorderRadius.all(Radius.circular(8)),
            ),
            suffixIcon: Icon(Icons.edit_rounded),
            filled: true,
          ),
          style: monospaceTextStyle,
          controller: textController,
          onChanged: isPart
              ? (name) => _controller.setDeviceName(device.id, name)
              : null,
          enabled: isPart,
        ),
      ),
    );
  }

  Widget buildNameDevicesBody(BuildContext context) {
    return MultiSliver(
      children: [
        SliverDeviceList(
          deviceBuilder: (ctx, device) => _buildDeviceForNaming(ctx, device),
        ),
        if (!_controller.allWalletDevicesConnected)
          SliverToBoxAdapter(child: buildDisconnectedWarningCard(context)),
      ],
    );
  }

  Widget buildThresholdBody(BuildContext context) {
    final theme = Theme.of(context);
    final form = _controller.form;
    final totalCount = form.selectedDevices.length;
    assert(totalCount > 0);
    if (form.threshold == null) {
      final threshold = max((totalCount * 2 / 3).toInt(), 1);
      setState(() => form.threshold = threshold);
    }
    return SliverList.list(
      children: [
        if (totalCount > 1)
          Slider(
            value: (form.threshold!).toDouble(),
            label: '${form.threshold}',
            onChanged: (value) =>
                setState(() => form.threshold = value.toInt()),
            min: 1,
            max: totalCount.toDouble(),
            divisions: max(totalCount - 1, 1),
          ),
        Center(
          child: Card.filled(
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 12, horizontal: 16),
              child: Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text: '${form.threshold}',
                      style: TextStyle(
                        decoration: TextDecoration.underline,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    TextSpan(text: ' of $totalCount'),
                  ],
                  style: theme.textTheme.headlineSmall,
                ),
              ),
            ),
          ),
        ),
        if (!_controller.allWalletDevicesConnected)
          buildDisconnectedWarningCard(context),
      ],
    );
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

    // Nonces needed - show progress
    return StreamBuilder<NonceReplenishState>(
      stream: stream,
      builder: (context, snapshot) {
        final state = snapshot.data;
        
        Widget content;
        if (state == null) {
          content = Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 24),
              Text('Connecting...', style: theme.textTheme.bodyLarge),
            ],
          );
        } else {
          final progress = state.receivedFrom.length;
          final total = state.devices.length;
          final isComplete = progress == total;

          // Auto-advance when complete
          if (isComplete && !_controller._hasAutoAdvanced) {
            _controller._hasAutoAdvanced = true;
            WidgetsBinding.instance.addPostFrameCallback((_) {
              if (mounted) {
                _controller.next(context);
              }
            });
          }

          content = Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                'Preparing devices',
                style: theme.textTheme.headlineMedium,
              ),
              SizedBox(height: 32),
              Stack(
                alignment: Alignment.center,
                children: [
                  SizedBox(
                    width: 120,
                    height: 120,
                    child: CircularProgressIndicator(
                      value: total > 0 ? progress / total : null,
                      strokeWidth: 8,
                      backgroundColor: theme.colorScheme.surfaceVariant,
                    ),
                  ),
                  Text(
                    '$progress of $total',
                    style: theme.textTheme.headlineSmall,
                  ),
                ],
              ),
              SizedBox(height: 16),
              Text(
                isComplete ? 'Complete!' : 'Please wait...',
                style: theme.textTheme.bodyMedium?.copyWith(
                  color: isComplete 
                      ? theme.colorScheme.primary
                      : theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          );
        }
        
        return SliverToBoxAdapter(
          child: Padding(
            padding: EdgeInsets.symmetric(vertical: 32),
            child: Align(
              alignment: Alignment.topCenter,
              child: content,
            ),
          ),
        );
      },
    );
  }

  Widget buildBody(BuildContext context) {
    switch (_controller.step) {
      case WalletCreateStep.name:
        return buildWalletNameBody(context);
      case WalletCreateStep.deviceCount:
        return buildDevicesBody(context);
      case WalletCreateStep.nonceReplenish:
        return buildNonceReplenish(context);
      case WalletCreateStep.deviceNames:
        return buildNameDevicesBody(context);
      case WalletCreateStep.threshold:
        return buildThresholdBody(context);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mediaQuery = MediaQuery.of(context);
    final windowSize = WindowSizeContext.of(context);
    final isFullscreen = windowSize == WindowSizeClass.compact;

    final network = _controller.form.network;
    final appBarTrailingText = network.isMainnet()
        ? ''
        : ' (${network.name()})';

    final header = TopBarSliver(
      title: Text('${_controller.title}$appBarTrailingText'),
      leading: IconButton(
        icon: Icon(Icons.arrow_back_rounded),
        onPressed: () => goBackOrClose(context),
        tooltip: 'Back',
      ),
    );

    final column = Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: AnimatedSwitcher(
            duration: Durations.medium4,
            reverseDuration: Duration.zero,
            transitionBuilder: (child, animation) {
              final curvedAnimation = CurvedAnimation(
                parent: animation,
                curve: Curves.easeInOutCubicEmphasized,
              );
              return SlideTransition(
                position: Tween<Offset>(
                  begin: _controller.isAnimationForward
                      ? const Offset(1, 0)
                      : const Offset(-1, 0),
                  end: Offset.zero,
                ).animate(curvedAnimation),
                child: FadeTransition(opacity: animation, child: child),
              );
            },
            child: CustomScrollView(
              key: ValueKey<WalletCreateStep>(_controller.step),
              physics: ClampingScrollPhysics(),
              shrinkWrap: windowSize != WindowSizeClass.compact,
              slivers: [
                header,
                SliverToBoxAdapter(
                  child: Padding(
                    padding: topSectionPadding.copyWith(
                      top: isFullscreen ? null : 8,
                    ),
                    child: Text(
                      _controller.subtitle,
                      style: theme.textTheme.titleMedium,
                    ),
                  ),
                ),
                SliverPadding(
                  padding: sectionPadding,
                  sliver: buildBody(context),
                ),
                SliverPadding(padding: EdgeInsets.only(bottom: 32)),
              ],
            ),
          ),
        ),
        Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Divider(height: 0),
            if (SettingsContext.of(context)?.settings.isInDeveloperMode() ??
                false)
              buildAdvancedOptions(context),
            Padding(
              padding: EdgeInsets.all(
                16,
              ).add(EdgeInsets.only(bottom: mediaQuery.viewInsets.bottom)),
              child: SafeArea(
                top: false,
                child: Align(
                  alignment: Alignment.centerRight,
                  child: FilledButton(
                    onPressed: _controller.canGoNext
                        ? () => _controller.next(context)
                        : null,
                    child: Text(
                      _controller.nextText ?? 'Next',
                      softWrap: false,
                      overflow: TextOverflow.fade,
                    ),
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
        print('didPop=$didPop, result=$result');
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

  bool _isAdvancedOptionsHidden = true;
  StatefulBuilder buildAdvancedOptions(BuildContext context) {
    final theme = Theme.of(context);
    return StatefulBuilder(
      builder: (context, setState) {
        final mayHide = Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 12,
          children: [
            Text(
              'Network',
              style: theme.textTheme.labelMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            SegmentedButton<String>(
              showSelectedIcon: false,
              segments: BitcoinNetwork.supportedNetworks()
                  .map(
                    (network) => ButtonSegment(
                      value: network.name(),
                      label: Text(
                        network.name(),
                        overflow: TextOverflow.fade,
                        softWrap: false,
                      ),
                    ),
                  )
                  .toList(),
              selected: {_controller.form.network.name()},
              onSelectionChanged: (selectedSet) {
                _isAdvancedOptionsHidden = true;
                final selected = selectedSet.first;
                _controller.setNetwork(
                  BitcoinNetwork.fromString(string: selected)!,
                );
              },
            ),
            SizedBox(height: 8),
          ],
        );
        return Padding(
          padding: EdgeInsets.symmetric(horizontal: 16).copyWith(top: 12),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              AnimatedCrossFade(
                firstChild: SizedBox(),
                secondChild: mayHide,
                crossFadeState: _isAdvancedOptionsHidden
                    ? CrossFadeState.showFirst
                    : CrossFadeState.showSecond,
                duration: Durations.medium2,
                sizeCurve: Curves.easeInOutCubicEmphasized,
              ),
              Row(
                mainAxisAlignment: MainAxisAlignment.end,
                spacing: 8,
                children: [
                  if (!_controller.form.network.isMainnet())
                    InputChip(
                      surfaceTintColor: theme.colorScheme.error,
                      label: Text(_controller.form.network.name()),
                      deleteIcon: Icon(Icons.clear_rounded),
                      onDeleted: () {
                        _isAdvancedOptionsHidden = true;
                        _controller.setNetwork(BitcoinNetwork.bitcoin);
                      },
                    ),
                  TextButton.icon(
                    onPressed: () => setState(
                      () =>
                          _isAdvancedOptionsHidden = !_isAdvancedOptionsHidden,
                    ),
                    icon: Icon(
                      _isAdvancedOptionsHidden
                          ? Icons.arrow_drop_up_rounded
                          : Icons.arrow_drop_down_rounded,
                    ),
                    label: Text(
                      'Developer',
                      overflow: TextOverflow.fade,
                      softWrap: false,
                    ),
                  ),
                ],
              ),
            ],
          ),
        );
      },
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
