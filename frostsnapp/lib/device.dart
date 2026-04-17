import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

import 'global.dart';

class DeviceDetails extends StatefulWidget {
  final ScrollController? scrollController;
  final DeviceId deviceId;
  final Future<bool> Function(BuildContext context) firmwareUpgrade;

  const DeviceDetails({
    super.key,
    this.scrollController,
    required this.deviceId,
    required this.firmwareUpgrade,
  });

  @override
  State<DeviceDetails> createState() => _DeviceDetailsState();
}

class _DeviceDetailsState extends State<DeviceDetails> {
  late final StreamSubscription _sub;
  bool _showAdvanced = false;
  bool _gotFirstData = false;
  ConnectedDevice? _device;

  FullscreenActionDialogController<void>? _eraseController;
  final _eraseConfirmed = ValueNotifier<bool>(false);

  @override
  void initState() {
    super.initState();
    _sub = GlobalStreams.deviceListSubject.listen((
      DeviceListUpdate update,
    ) async {
      final device = update.state.devices.firstWhereOrNull(
        (device) => deviceIdEquals(device.id, widget.deviceId),
      );
      final wasConnected = _device != null;
      final nowDisconnected = device == null;
      setState(() {
        _gotFirstData = true;
        _device = device;
      });
      // Only auto-close ourselves if this route is actually on top — otherwise
      // an upgrade / erase / name dialog is stacked in front and Navigator.pop
      // would whack *that* route instead of ours.
      if (wasConnected && nowDisconnected) {
        final route = ModalRoute.of(context);
        if (route?.isCurrent == true) Navigator.pop(context);
      }
    });
  }

  @override
  void dispose() {
    _sub.cancel();
    _eraseController?.dispose();
    _eraseConfirmed.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final device = _device;
    return CustomScrollView(
      controller: widget.scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        if (_gotFirstData)
          SliverToBoxAdapter(
            child: device == null
                ? _buildDisconnectedWidget(context)
                : _buildColumn(context, device),
          ),
        SliverSafeArea(sliver: SliverToBoxAdapter(child: SizedBox(height: 12))),
      ],
    );
  }

  Widget _buildDisconnectedWidget(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: EdgeInsets.symmetric(vertical: 40),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          CircularProgressIndicator(),
          SizedBox(height: 16),
          Text(
            'Waiting for device to connect',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildColumn(BuildContext context, ConnectedDevice device) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;
    final deviceName = device.name;
    final wallet = coord
        .frostKeysInvolvingDevice(deviceId: device.id)
        .firstOrNull;
    final isEmpty = deviceName == null;
    final hasWallet = wallet != null;
    final upgradeEligibility = device.firmwareUpgradeEligibility();
    final noncesAvailable = coord.noncesAvailable(id: device.id);

    final emptyRows = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Fresh Device'),
        subtitle: Text('Can be used to create a wallet'),
        leading: Icon(Icons.ac_unit_rounded),
        enabled: false,
      ),
    ];

    final nonEmptyRows = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Device Name'),
        subtitle: Text(
          deviceName ?? 'Unnamed',
          style: monospaceTextStyle.copyWith(
            color: device.name == null ? theme.disabledColor : null,
          ),
        ),
        leading: Icon(Icons.label_rounded),
        onTap: deviceName == null
            ? null
            : () => copyAction(context, 'Device name', deviceName),
      ),
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Wallet'),
        subtitle: Text(
          wallet?.keyName() ?? 'Available for recovery',
          style: monospaceTextStyle.copyWith(
            color: hasWallet ? null : theme.disabledColor,
          ),
        ),
        leading: Icon(Icons.wallet_rounded),
        trailing: hasWallet ? Icon(Icons.chevron_right_rounded) : null,
        onTap: hasWallet
            ? () {
                Navigator.popUntil(context, (r) => r.isFirst);
                homeCtx.walletListController.selectWallet(wallet.keyId());
              }
            : null,
      ),
    ];

    final advancedHidden = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Device ID'),
        subtitle: Text(
          device.id.toHex(),
          overflow: TextOverflow.ellipsis,
          style: monospaceTextStyle,
        ),
        leading: Icon(Icons.fingerprint_rounded),
        onTap: () => copyAction(context, 'Device ID', device.id.toHex()),
      ),
      if (!isEmpty)
        ListTile(
          contentPadding: EdgeInsets.symmetric(horizontal: 16),
          title: Text('Nonces'),
          subtitle: Text('$noncesAvailable'),
          leading: Icon(Icons.numbers_rounded),
          onTap: () =>
              copyAction(context, 'Remaining nonces', '$noncesAvailable'),
        ),
      if (!isEmpty)
        ListTile(
          contentPadding: EdgeInsets.symmetric(horizontal: 16),
          title: Text('Erase device'),
          subtitle: Text('Delete everything from this device'),
          leading: Icon(Icons.delete_forever_rounded),
          trailing: TextButton(
            onPressed: () => showEraseDialog(context, device.id),
            child: Text('Erase'),
            style: TextButton.styleFrom(
              foregroundColor: theme.colorScheme.error,
            ),
          ),
        ),
    ];

    final advancedRows = [
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Advanced'),
        trailing: Icon(
          _showAdvanced ? Icons.expand_less_rounded : Icons.expand_more_rounded,
        ),
        onTap: () => setState(() => _showAdvanced = !_showAdvanced),
      ),
      AnimatedCrossFade(
        firstChild: Column(
          mainAxisSize: MainAxisSize.min,
          children: advancedHidden,
        ),
        secondChild: SizedBox(width: double.infinity),
        crossFadeState: _showAdvanced
            ? CrossFadeState.showFirst
            : CrossFadeState.showSecond,
        duration: Durations.medium2,
        sizeCurve: Curves.easeInOutCubicEmphasized,
      ),
    ];

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        ...(isEmpty ? emptyRows : nonEmptyRows),
        ListTile(
          contentPadding: EdgeInsets.symmetric(horizontal: 16),
          leading: Icon(Icons.system_update_rounded),
          title: Row(
            children: [
              Text('Firmware'),
              SizedBox(width: 8),
              Card.filled(
                color: theme.colorScheme.primaryContainer.withAlpha(80),
                margin: EdgeInsets.zero,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 2,
                  ),
                  child: Text(
                    device.firmware.versionName(),
                    style: TextStyle(
                      fontSize: 12,
                      color: theme.colorScheme.onPrimaryContainer,
                    ),
                  ),
                ),
              ),
            ],
          ),
          subtitle: Text(
            device.firmware.digest.toString(),
            style: monospaceTextStyle,
            overflow: TextOverflow.ellipsis,
          ),
          trailing: upgradeEligibility.when(
            canUpgrade: () => TextButton.icon(
              onPressed: () async => await widget.firmwareUpgrade(context),
              label: Text('Upgrade'),
            ),
            upToDate: () => Card.outlined(
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  vertical: 4.0,
                  horizontal: 8.0,
                ),
                child: Text('Latest'),
              ),
            ),
            cannotUpgrade: (reason) => Tooltip(
              message: reason,
              child: Card.outlined(
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    vertical: 4.0,
                    horizontal: 8.0,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(Icons.warning_rounded, size: 16),
                      SizedBox(width: 4),
                      Text('Incompatible'),
                    ],
                  ),
                ),
              ),
            ),
          ),
          onTap: () => copyAction(
            context,
            "Device firmware",
            device.firmware.digest.toString(),
          ),
        ),
        ...advancedRows,
      ],
    );
  }

  copyAction(BuildContext context, String what, String data) {
    Clipboard.setData(ClipboardData(text: data));
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text('$what copied to clipboard')));
  }

  void showEraseDialog(BuildContext context, DeviceId id) async {
    // Check if device is involved in any wallet with an active signing session
    final accessStructureRefs = coord.accessStructuresInvolvingDevice(
      deviceId: id,
    );
    for (final ref in accessStructureRefs) {
      final sessions = coord.activeSigningSessions(keyId: ref.keyId);
      if (sessions.isNotEmpty) {
        final walletName =
            coord.getFrostKey(keyId: ref.keyId)?.keyName() ?? 'Unknown';
        if (context.mounted) {
          showDialog(
            context: context,
            builder: (context) => AlertDialog(
              title: Text('Cannot Erase Device'),
              content: Text(
                'This device is part of wallet "$walletName" which has an active signing session.\n'
                'Finish or cancel the signing session to continue with erasing.',
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: Text('OK'),
                ),
              ],
            ),
          );
        }
        return;
      }
    }

    _eraseConfirmed.value = false;

    late final FullscreenActionDialogController<void> controller;
    controller = FullscreenActionDialogController<void>(
      context: context,
      devices: [id],
      title: 'Erase Device',
      body: (context) {
        final theme = Theme.of(context);
        return ValueListenableBuilder<bool>(
          valueListenable: _eraseConfirmed,
          builder: (context, confirmed, _) {
            if (confirmed) {
              return Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CircularProgressIndicator(),
                  SizedBox(height: 24),
                  Text(
                    'Waiting for device reset',
                    style: theme.textTheme.titleMedium,
                    textAlign: TextAlign.center,
                  ),
                  SizedBox(height: 8),
                  Text(
                    'Do not disconnect device',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.error,
                    ),
                    textAlign: TextAlign.center,
                  ),
                ],
              );
            }
            return Card.filled(
              margin: EdgeInsets.zero,
              color: theme.colorScheme.errorContainer,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  ListTile(
                    leading: Icon(Icons.warning_rounded),
                    title: Text(
                      'This will wipe the key from the device.',
                      style: TextStyle(fontWeight: FontWeight.bold),
                    ),
                    subtitle: Padding(
                      padding: EdgeInsets.only(top: 6),
                      child: Text(
                        'The device will be rendered blank.\nThis action can not be reverted, and the only way to restore this key is by loading its backup.',
                      ),
                    ),
                    isThreeLine: true,
                    textColor: theme.colorScheme.onErrorContainer,
                    iconColor: theme.colorScheme.onErrorContainer,
                    contentPadding: EdgeInsets.symmetric(horizontal: 16),
                  ),
                ],
              ),
            );
          },
        );
      },
      actionButtons: [
        ValueListenableBuilder<bool>(
          valueListenable: _eraseConfirmed,
          builder: (context, confirmed, _) => confirmed
              ? SizedBox.shrink()
              : OutlinedButton(
                  child: Text('Cancel'),
                  onPressed: () async {
                    await coord.sendCancel(id: id);
                    await controller.clearAllActionsNeeded();
                  },
                ),
        ),
        ValueListenableBuilder<bool>(
          valueListenable: _eraseConfirmed,
          builder: (context, confirmed, _) => confirmed
              ? DeviceActionHint(label: 'Confirmed', icon: Icons.check_rounded)
              : DeviceActionHint(),
        ),
      ],
      onDismissed: () {},
    );
    _eraseController = controller;

    try {
      final stream = coord.eraseDevice(deviceId: id);
      String? removedFromWallet;

      await for (final state in stream) {
        if (state == EraseDeviceState.confirmed) {
          final accessStructureRefs = coord.accessStructuresInvolvingDevice(
            deviceId: id,
          );
          for (final ref in accessStructureRefs) {
            removedFromWallet = coord.getFrostKey(keyId: ref.keyId)?.keyName();
            await coord.deleteShare(accessStructureRef: ref, deviceId: id);
          }
          _eraseConfirmed.value = true;
          break;
        }
      }

      // `Confirmed` means the device has *started* erasing, not finished.
      // Don't remove from action-needed here — instead wait for the device
      // to actually disconnect (erase complete → device blank → USB drop),
      // which drops `shouldShow` to false and auto-dismisses the dialog.
      await controller.awaitDismissed();

      if (context.mounted) {
        _showEraseSuccessDialog(context, removedFromWallet);
      }
    } finally {
      if (_eraseController == controller) _eraseController = null;
      controller.dispose();
    }
  }

  void _showEraseSuccessDialog(BuildContext context, String? walletName) {
    final theme = Theme.of(context);
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        icon: Icon(
          Icons.check_circle_rounded,
          size: 48,
          color: theme.colorScheme.primary,
        ),
        title: Text('Device Erased'),
        content: walletName != null
            ? Text('The device has been removed from wallet "$walletName".')
            : Text('The device has been erased.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text('OK'),
          ),
        ],
      ),
    );
  }
}
