import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/animated_gradient_card.dart';
import 'package:frostsnap/device_action_upgrade.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/name.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_device_list.dart';
import 'package:rxdart/rxdart.dart';
import 'package:sliver_tools/sliver_tools.dart';

/// Shared state for the "add devices" step used by local and remote
/// (org) keygen. Tracks connected devices, their pending names, and
/// owns a [DeviceActionUpgradeController] so the UI can drive
/// firmware upgrades without the caller plumbing one separately.
///
/// Device names are stored as *previews*: each `setDeviceName` call
/// forwards to `coord.updateNamePreview` so the device confirms the
/// name when it completes the subsequent protocol (keygen). On
/// device reconnection, the preview is re-sent.
class DeviceSetupController extends ChangeNotifier {
  DeviceSetupController() {
    // Seed the behavior subject with the synchronous current state so
    // late subscribers get a consistent snapshot immediately — they
    // don't have to wait for the next real device event.
    _deviceList = coord.deviceListState();
    _updates.add(DeviceListUpdate(changes: const [], state: _deviceList));
    _sub = GlobalStreams.deviceListSubject.listen((update) {
      _deviceList = update.state;
      for (final change in update.changes) {
        if (change.kind == DeviceListChangeKind.added) {
          final id = change.device.id;
          final name = _deviceNames[id];
          if (name != null) {
            coord.updateNamePreview(id: id, name: name);
          }
        }
      }
      _updates.add(update);
      if (hasListeners) notifyListeners();
    });
  }

  late final StreamSubscription _sub;
  late DeviceListState _deviceList;
  final BehaviorSubject<DeviceListUpdate> _updates = BehaviorSubject();
  // Cached to guarantee identity stability across `ctrl.updates`
  // reads. `Subject.stream` in rxdart returns a fresh `_SubjectStream`
  // wrapper on every call, which would trip the `!identical` check in
  // `SliverDeviceList.didUpdateWidget` and resubscribe / reset the
  // rendered list on every parent rebuild.
  late final Stream<DeviceListUpdate> _updatesStream = _updates.stream;
  final Map<DeviceId, String> _deviceNames = deviceIdMap<String>();
  final DeviceActionUpgradeController upgradeController =
      DeviceActionUpgradeController();

  DeviceListState get deviceList => _deviceList;
  List<ConnectedDevice> get devices => _deviceList.devices;
  Map<DeviceId, String> get deviceNames => _deviceNames;
  int get connectedDeviceCount => _deviceList.devices.length;

  /// Stream of device-list updates, scoped to this controller's
  /// lifetime. Replays the latest update to new subscribers so
  /// [SliverDeviceList] can seed its rendering regardless of when it
  /// mounts relative to the controller. Every emission carries both
  /// the authoritative `state` and the `changes` that produced it.
  Stream<DeviceListUpdate> get updates => _updatesStream;

  bool get devicesNeedUpgrade =>
      _deviceList.devices.any((dev) => dev.needsFirmwareUpgrade());
  bool get devicesCanUpgrade => _deviceList.devices.any((dev) {
    final eligibility = dev.firmwareUpgradeEligibility();
    return eligibility.when(
      upToDate: () => false,
      canUpgrade: () => true,
      cannotUpgrade: (_) => false,
    );
  });
  bool get devicesIncompatible => _deviceList.devices.any((dev) {
    final eligibility = dev.firmwareUpgradeEligibility();
    return eligibility.when(
      upToDate: () => false,
      canUpgrade: () => false,
      cannotUpgrade: (_) => true,
    );
  });
  bool get devicesUsed =>
      _deviceList.devices.any((dev) => dev.name != null);

  /// True when at least one device is connected, none block progress
  /// (firmware issues, already-holds-a-key), and every connected
  /// device has a non-empty name preview.
  bool get ready =>
      _deviceList.devices.isNotEmpty &&
      !devicesNeedUpgrade &&
      !devicesUsed &&
      !devicesIncompatible &&
      _deviceList.devices.every((d) => _deviceNames.containsKey(d.id));

  /// Re-emit name previews for currently-connected devices whose
  /// controller already has a name pending. Useful after a keygen
  /// abort, where the device-side preview may have been cleared.
  Future<void> resendNamePreviews() async {
    for (final device in _deviceList.devices) {
      final name = _deviceNames[device.id];
      if (name != null) {
        await coord.updateNamePreview(id: device.id, name: name);
      } else {
        await coord.sendCancel(id: device.id);
      }
    }
  }

  Future<void> setDeviceName(DeviceId id, String name) async {
    final trimmedName = name.trim();
    if (trimmedName.isNotEmpty) {
      _deviceNames[id] = trimmedName;
      notifyListeners();
      await coord.updateNamePreview(id: id, name: trimmedName);
    } else {
      _deviceNames.remove(id);
      notifyListeners();
      await coord.sendCancel(id: id);
    }
  }

  @override
  void dispose() {
    _sub.cancel();
    _updates.close();
    upgradeController.dispose();
    // Clear any pending name previews on disposal so stale previews
    // don't linger on devices for the next flow.
    for (final device in _deviceList.devices) {
      coord.sendCancel(id: device.id);
    }
    super.dispose();
  }
}

/// Reusable "add devices" sliver list driven by a
/// [DeviceSetupController]. Returns a [MultiSliver] — host inside a
/// `CustomScrollView` (or `MultiSliver` parent).
class DeviceSetupView extends StatefulWidget {
  const DeviceSetupView({super.key, required this.controller, this.onSubmitted});
  final DeviceSetupController controller;
  /// Fired when the user presses Enter on any device-name TextField. The
  /// caller decides whether the form is in a submittable state — this
  /// just forwards the keypress up.
  final VoidCallback? onSubmitted;

  @override
  State<DeviceSetupView> createState() => _DeviceSetupViewState();
}

class _DeviceSetupViewState extends State<DeviceSetupView> {
  final Map<DeviceId, TextEditingController> _nameControllers = deviceIdMap();

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_onChanged);
  }

  @override
  void dispose() {
    widget.controller.removeListener(_onChanged);
    for (final c in _nameControllers.values) {
      c.dispose();
    }
    super.dispose();
  }

  void _onChanged() {
    if (!mounted) return;
    // Drop text controllers for devices that are no longer connected
    // so a fresh plug-in starts from blank rather than resurrecting
    // a previously-typed-but-discarded name.
    final present = deviceIdSet(widget.controller.devices.map((d) => d.id));
    final stale =
        _nameControllers.keys.where((id) => !present.contains(id)).toList();
    for (final id in stale) {
      _nameControllers.remove(id)?.dispose();
    }
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final ctrl = widget.controller;
    final theme = Theme.of(context);
    final parentCtx = context;
    return MultiSliver(
      children: [
        SliverDeviceList(
          updates: ctrl.updates,
          deviceBuilder: (context, device) {
            final cs = Theme.of(context).colorScheme;

            if (device.name != null) {
              return _deviceRow(
                context: context,
                title: Text(
                  device.name!,
                  style: monospaceTextStyle.copyWith(
                    color: cs.onSurfaceVariant,
                  ),
                ),
                trailing: _trailingInfo(
                  context,
                  text: 'Already holds a key',
                  subText: 'Unplug to continue',
                  icon: Icons.warning_rounded,
                  color: cs.error,
                ),
                enabled: false,
              );
            }

            return device.firmwareUpgradeEligibility().when(
              upToDate: () => _deviceRow(
                context: context,
                title: _inlineNameField(context, device),
                trailing: Icon(
                  Icons.edit_rounded,
                  color: cs.onSurfaceVariant,
                  size: 20,
                ),
              ),
              canUpgrade: () => _deviceRow(
                context: context,
                title: const SizedBox.shrink(),
                trailing: _trailingInfo(
                  context,
                  text: 'Old firmware',
                  subText: 'Tap to upgrade',
                  icon: Icons.system_update_alt_rounded,
                  color: Colors.orange,
                ),
                onTap: () async =>
                    await ctrl.upgradeController.run(parentCtx),
              ),
              cannotUpgrade: (reason) => _deviceRow(
                context: context,
                title: const SizedBox.shrink(),
                trailing: _trailingInfo(
                  context,
                  text: 'Incompatible firmware',
                  subText: reason,
                  icon: Icons.warning_rounded,
                  color: cs.error,
                ),
                enabled: false,
              ),
            );
          },
        ),

        if (ctrl.devicesCanUpgrade && !ctrl.devicesIncompatible)
          SliverToBoxAdapter(
            child: Card.outlined(
              margin: const EdgeInsets.symmetric(vertical: 8),
              child: ListTile(
                dense: true,
                contentPadding: const EdgeInsets.symmetric(horizontal: 16),
                title: const Text(
                  'One or more devices require a firmware update before continuing.',
                ),
                leading: const Icon(
                  Icons.system_update_alt_rounded,
                  color: Colors.orange,
                ),
                trailing: TextButton(
                  onPressed: () async =>
                      await ctrl.upgradeController.run(context),
                  child: const Text('Start Upgrade'),
                ),
                onTap: () async =>
                    await ctrl.upgradeController.run(context),
              ),
            ),
          ),
        if (ctrl.devicesIncompatible)
          SliverToBoxAdapter(
            child: Card.outlined(
              margin: const EdgeInsets.symmetric(vertical: 8),
              child: ListTile(
                dense: true,
                contentPadding: const EdgeInsets.symmetric(horizontal: 16),
                title: const Text(
                  'One or more devices have incompatible firmware. Unplug them to continue.',
                ),
                leading: Icon(
                  Icons.warning_rounded,
                  color: theme.colorScheme.error,
                ),
              ),
            ),
          ),
        const SliverToBoxAdapter(
          child: AnimatedGradientCard(
            child: ListTile(
              dense: true,
              title: Text(
                'Plug in all devices to include them in this wallet.',
              ),
              contentPadding: EdgeInsets.symmetric(horizontal: 16),
              leading: Icon(Icons.info_rounded),
            ),
          ),
        ),
      ],
    );
  }

  Widget _deviceRow({
    required BuildContext context,
    required Widget title,
    required Widget trailing,
    VoidCallback? onTap,
    bool enabled = true,
  }) {
    final cs = Theme.of(context).colorScheme;
    return Card.filled(
      margin: const EdgeInsets.symmetric(vertical: 4),
      color: cs.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: ListTile(
        onTap: onTap,
        enabled: enabled,
        leading: Icon(
          Icons.key,
          color: enabled
              ? cs.onSurfaceVariant
              : cs.onSurfaceVariant.withValues(alpha: 0.5),
        ),
        title: title,
        trailing: trailing,
        contentPadding: const EdgeInsets.symmetric(horizontal: 16),
      ),
    );
  }

  Widget _trailingInfo(
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

  Widget _inlineNameField(BuildContext context, ConnectedDevice device) {
    final cs = Theme.of(context).colorScheme;
    final ctrl = widget.controller;
    final currentName = ctrl.deviceNames[device.id] ?? '';
    final textController = _nameControllers.putIfAbsent(
      device.id,
      () => TextEditingController(text: currentName),
    );
    if (textController.text != currentName) {
      textController.text = currentName;
    }
    return TextField(
      controller: textController,
      style: monospaceTextStyle,
      maxLength: DeviceName.maxLength(),
      inputFormatters: [nameInputFormatter],
      textInputAction: TextInputAction.done,
      decoration: InputDecoration(
        hintText: 'Enter device name',
        hintStyle: monospaceTextStyle.copyWith(color: cs.onSurfaceVariant),
        border: InputBorder.none,
        isDense: true,
        contentPadding: EdgeInsets.zero,
        counterText: '',
      ),
      onChanged: (name) => ctrl.setDeviceName(device.id, name),
      onSubmitted: widget.onSubmitted == null ? null : (_) => widget.onSubmitted!(),
    );
  }
}
