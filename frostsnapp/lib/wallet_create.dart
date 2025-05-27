import 'dart:async';
import 'dart:math';
import 'dart:ui';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/device_action_fullscreen_dialog.dart';
import 'package:frostsnapp/device_settings.dart';
import 'package:frostsnapp/hex.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/src/rust/api.dart';
import 'package:frostsnapp/src/rust/api/bitcoin.dart';
import 'package:frostsnapp/src/rust/api/device_list.dart';
import 'package:frostsnapp/src/rust/api/keygen.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'package:sliver_tools/sliver_tools.dart';
import 'global.dart';
import 'wallet_device_list.dart';

enum WindowSizeClass {
  compact(maxWidth: 600),
  medium(maxWidth: 840),
  expanded(maxWidth: 1200);

  const WindowSizeClass({required this.maxWidth});

  static WindowSizeClass fromWidth(double width) {
    if (width < 600) {
      return WindowSizeClass.compact;
    }
    if (width < 840) {
      return WindowSizeClass.medium;
    }
    return WindowSizeClass.expanded;
  }

  /// Max width (exclusive).
  final double maxWidth;
}

class WindowSizeContext extends InheritedWidget {
  // final Size windowSize;
  final WindowSizeClass windowSizeClass;

  const WindowSizeContext({
    super.key,
    required this.windowSizeClass,
    required super.child,
  });

  static WindowSizeClass of(BuildContext context) {
    Size size(BuildContext context) {
      final view = View.of(context);
      return view.physicalSize / view.devicePixelRatio;
    }

    return context
            .dependOnInheritedWidgetOfExactType<WindowSizeContext>()
            ?.windowSizeClass ??
        WindowSizeClass.fromWidth(size(context).width);
  }

  @override
  bool updateShouldNotify(covariant InheritedWidget oldWidget) {
    return false;
  }
}

class MaybeFullscreenDialog extends StatefulWidget {
  final Widget? child;
  final Color? backgroundColor;
  const MaybeFullscreenDialog({super.key, this.child, this.backgroundColor});

  static Future<T?> show<T>({
    required BuildContext context,
    bool barrierDismissible = false,
    Color? backgroundColor,
    Widget? child,
  }) {
    return showDialog(
      context: context,
      barrierDismissible: barrierDismissible,
      useSafeArea: false,
      builder: (context) => MaybeFullscreenDialog(
        backgroundColor:
            backgroundColor ?? Theme.of(context).colorScheme.surface,
        child: child,
      ),
    );
  }

  @override
  State<MaybeFullscreenDialog> createState() => _MaybeFullscreenDialogState();
}

class _MaybeFullscreenDialogState extends State<MaybeFullscreenDialog>
    with WidgetsBindingObserver {
  late final ValueNotifier<WindowSizeClass> _sizeClass;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _sizeClass = ValueNotifier(
      WindowSizeClass.fromWidth(getWindowSize().width),
    );
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _sizeClass.dispose();
    super.dispose();
  }

  @override
  void didChangeMetrics() {
    super.didChangeMetrics();
    _sizeClass.value = WindowSizeClass.fromWidth(getWindowSize().width);
  }

  Size getWindowSize() {
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    return view.physicalSize / view.devicePixelRatio;
  }

  final boxKey = GlobalKey();

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder(
      valueListenable: _sizeClass,
      child: ConstrainedBox(
        key: boxKey,
        constraints: const BoxConstraints(maxWidth: 640),
        child: widget.child,
      ),
      builder: (context, sizeClass, child) => WindowSizeContext(
        windowSizeClass: _sizeClass.value,
        child: BackdropFilter(
          filter: switch (sizeClass) {
            WindowSizeClass.compact => ImageFilter.blur(),
            _ => blurFilter,
          },
          child: switch (_sizeClass.value) {
            WindowSizeClass.compact => Dialog.fullscreen(
              backgroundColor: widget.backgroundColor,
              child: child,
            ),
            WindowSizeClass.medium || WindowSizeClass.expanded => Dialog(
              insetPadding: EdgeInsets.zero,
              clipBehavior: Clip.hardEdge,
              backgroundColor: widget.backgroundColor,
              child: child,
            ),
          },
        ),
      ),
    );
  }
}

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
            crossAxisAlignment: CrossAxisAlignment.stretch,
            spacing: 12,
            children: [
              const Text(
                'Confirm that this code is shown on all devices',
                textAlign: TextAlign.center,
              ),
              Card.filled(
                child: Center(
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
              ),
              LargeCircularProgressIndicator(
                size: 70,
                progress: state.sessionAcks.length,
                total: state.devices.length,
              ),
            ],
          );
        },
      ),
      dismissButton: (context) => OutlinedButton(
        onPressed: () async => await coord.cancelProtocol(),
        child: Text('Cancel'),
      ),
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
    _keygenController.clearAllActionsNeeded();
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

  bool get canGoNext => switch (_step) {
    WalletCreateStep.name =>
      _nameError == null && _nameController.value.text.isNotEmpty,
    WalletCreateStep.deviceCount =>
      _deviceList.devices.isNotEmpty && !devicesNeedUpgrade && !devicesUsed,
    WalletCreateStep.deviceNames =>
      allWalletDevicesConnected && _form.allDevicesNamed,
    WalletCreateStep.threshold =>
      allWalletDevicesConnected &&
          _form.threshold != null &&
          _form.threshold! > 0 &&
          _form.threshold! <= _form.selectedDevices.length,
  };
  bool get canGoBack => _step.index > 0;

  bool setNetwork(BitcoinNetwork network) {
    if (_asRef != null) return false;
    _form.network = network;
    notifyListeners();
    return true;
  }

  /// Does additional checks (maybe) and tries to populate the _form.
  Future<bool> _handleNext(BuildContext context) async {
    if (!canGoNext) return false;
    switch (_step) {
      case WalletCreateStep.name:
        _form.name = _nameController.text;
        return true;
      case WalletCreateStep.deviceCount:
        _form.selectedDevices.clear();
        _form.selectedDevices.addAll(_deviceList.devices.map((dev) => dev.id));
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
            if (!context.mounted) return false;
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
            if (keygenCodeMatches && context.mounted) {
              _asRef = await coord.finalizeKeygen(keygenId: state.keygenId);
              return true;
            }
            break;
          }
        }
        throw StateError('Unreachable: keygen completions handled');
    }
  }

  void next(BuildContext context) async {
    if (!await _handleNext(context)) return;
    if (!context.mounted) return;
    final nextStep = WalletCreateStep.values.elementAtOrNull(_step.index + 1);
    if (nextStep != null) {
      _step = nextStep;
      notifyListeners();
    } else {
      Navigator.pop(context, _asRef);
    }
  }

  bool _handleBack(BuildContext context) {
    switch (_step) {
      case _:
        return true;
    }
  }

  void back(context) {
    if (!_handleBack(context)) return;
    final prevIndex = _step.index - 1;
    final prevStep = WalletCreateStep.values.elementAtOrNull(prevIndex);
    if (prevStep != null) {
      _step = prevStep;
      notifyListeners();
    }
  }

  String? get backText => switch (_step) {
    _ => null,
  };

  String? get nextText => switch (_step) {
    WalletCreateStep.name => null,
    WalletCreateStep.deviceCount => switch (_deviceList.devices.length) {
      1 => 'Continue with 1 device',
      _ => 'Continue with ${_deviceList.devices.length} devices',
    },
    WalletCreateStep.deviceNames =>
      _form.allDevicesNamed ? null : 'Name all devices to continue',
    WalletCreateStep.threshold => 'Generate keys',
  };

  InlineSpan get title => switch (_step) {
    WalletCreateStep.name => TextSpan(text: 'Name wallet'),
    WalletCreateStep.deviceCount => TextSpan(text: 'Pick devices'),
    WalletCreateStep.deviceNames => TextSpan(text: 'Name devices'),
    WalletCreateStep.threshold => TextSpan(text: 'Choose threshold'),
  };

  TextSpan get subtitle => switch (_step) {
    WalletCreateStep.name => TextSpan(text: 'Choose a name for this wallet'),
    WalletCreateStep.deviceCount => TextSpan(
      children: [
        TextSpan(text: 'Connect devices to become keys for '),
        TextSpan(
          text: _form.name ?? '',
          style: TextStyle(fontStyle: FontStyle.italic),
        ),
      ],
    ),
    WalletCreateStep.deviceNames => TextSpan(
      text: 'Each device needs a name to idenitfy it.',
    ),
    WalletCreateStep.threshold => TextSpan(
      text:
          'Decide how many devices will be required to sign transactions or to make changes to this wallet',
    ),
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

enum WalletCreateStep { name, deviceCount, deviceNames, threshold }

class WalletCreatePage extends StatefulWidget {
  const WalletCreatePage({super.key});

  @override
  State<WalletCreatePage> createState() => _WalletCreatePageState();
}

class _WalletCreatePageState extends State<WalletCreatePage> {
  static const topSectionPadding = EdgeInsets.fromLTRB(20, 36, 20, 36);
  static const sectionPadding = EdgeInsets.fromLTRB(20, 20, 20, 28);
  late WalletCreateController _controller;

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
                    icon: Icons.warning,
                    color: Colors.orange,
                  )
                : buildDeviceTrailingInfo(
                    context,
                    text: 'Ready',
                    icon: Icons.check_circle_rounded,
                    color: Colors.green,
                  ),
          ),
        ),
        SliverToBoxAdapter(
          child: AnimatedGradientBorder(
            stretchAlongAxis: true,
            borderSize: 1.0,
            glowSize: 5.0,
            animationTime: 6,
            borderRadius: BorderRadius.circular(12.0),
            gradientColors: [
              theme.colorScheme.outlineVariant,
              theme.colorScheme.primary,
              theme.colorScheme.secondary,
              theme.colorScheme.tertiary,
            ],
            child: Card.outlined(
              margin: EdgeInsets.zero,
              child: ListTile(
                title: Text('Plug in devices to include them in this wallet.'),
                leading: Icon(Icons.info_rounded),
              ),
            ),
          ),
        ),
        if (_controller.devicesNeedUpgrade)
          SliverToBoxAdapter(
            child: Card.outlined(
              margin: EdgeInsets.symmetric(vertical: 16),
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  mainAxisSize: MainAxisSize.min,
                  spacing: 12,
                  children: [
                    Row(
                      spacing: 12,
                      children: [
                        Icon(
                          Icons.warning_rounded,
                          size: 32,
                          color: Colors.orange,
                        ),
                        Expanded(
                          child: Text(
                            'One or more devices require a firmware update before continuing.',
                            style: theme.textTheme.bodyMedium?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                          ),
                        ),
                      ],
                    ),
                    FilledButton.tonalIcon(
                      onPressed: () async =>
                          await FirmwareUpgradeDialog.show(context),
                      label: Text('Start upgrade'),
                      icon: Icon(Icons.system_update_alt_rounded),
                    ),
                  ],
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
      titleText: "Name device",
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
      color: Theme.of(context).colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: ListTile(
        leading: Icon(Icons.key),
        title: TextField(
          decoration: InputDecoration(hintText: 'Enter device name'),
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
      leading: Icon(Icons.warning_rounded),
      title: Text(
        'One or more devices have been disconnected. Reconnect to continue.',
      ),
    ),
  );

  Widget buildBody(BuildContext context) {
    switch (_controller.step) {
      case WalletCreateStep.name:
        return buildWalletNameBody(context);
      case WalletCreateStep.deviceCount:
        return buildDevicesBody(context);
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

    final network = _controller.form.network;
    final appBarTrailingText = network.isMainnet()
        ? ''
        : ' (${network.name()})';

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: AnimatedSize(
            duration: Durations.medium1,
            curve: Curves.easeInOutCubicEmphasized,
            child: AnimatedSwitcher(
              duration: Durations.short4,
              child: CustomScrollView(
                key: ValueKey<WalletCreateStep>(_controller.step),
                physics: ClampingScrollPhysics(),
                shrinkWrap: windowSize != WindowSizeClass.compact,
                slivers: [
                  SliverAppBar(
                    title: Text(
                      'Create Wallet$appBarTrailingText',
                      style: theme.textTheme.titleMedium,
                    ),
                    leading: IconButton(
                      onPressed: () => Navigator.pop(context),
                      icon: Icon(Icons.close),
                    ),
                    pinned: true,
                  ),
                  SliverToBoxAdapter(
                    child: Padding(
                      padding: topSectionPadding,
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        spacing: 12,
                        children: [
                          Text.rich(
                            _controller.title,
                            style: theme.textTheme.headlineLarge,
                          ),
                          Text.rich(
                            _controller.subtitle,
                            style: theme.textTheme.bodyLarge,
                          ),
                        ],
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
        ),
        Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (SettingsContext.of(context)?.settings.isInDeveloperMode() ??
                false)
              buildAdvancedOptions(context),
            Divider(height: 0),
            Padding(
              padding: EdgeInsets.all(
                20,
              ).add(EdgeInsets.only(bottom: mediaQuery.viewInsets.bottom)),
              child: SafeArea(
                top: false,
                child: Row(
                  mainAxisSize: MainAxisSize.max,
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Flexible(
                      child: TextButton(
                        onPressed: _controller.canGoBack
                            ? () => _controller.back(context)
                            : null,
                        child: Text(
                          _controller.backText ?? 'Back',
                          softWrap: false,
                          overflow: TextOverflow.fade,
                        ),
                      ),
                    ),
                    Expanded(
                      flex: 2,
                      child: Align(
                        alignment: AlignmentDirectional.centerEnd,
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
                  ],
                ),
              ),
            ),
          ],
        ),
      ],
    );
  }

  bool _isAdvancedOptionsHidden = true;
  StatefulBuilder buildAdvancedOptions(BuildContext context) {
    const titlePadding = EdgeInsets.fromLTRB(0, 20, 0, 12);
    final theme = Theme.of(context);
    return StatefulBuilder(
      builder: (context, setState) {
        final mayHide = Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: titlePadding,
              child: Text(
                'Network',
                style: theme.textTheme.labelMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
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
          ],
        );
        return Padding(
          padding: EdgeInsets.all(20),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
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
              AnimatedCrossFade(
                firstChild: SizedBox(),
                secondChild: mayHide,
                crossFadeState: _isAdvancedOptionsHidden
                    ? CrossFadeState.showFirst
                    : CrossFadeState.showSecond,
                duration: Durations.medium2,
                sizeCurve: Curves.easeInOutCubicEmphasized,
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
        oldWidget.total != widget.total)
      _initAnimation();
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
