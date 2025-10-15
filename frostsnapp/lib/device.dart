import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
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

  late final FullscreenActionDialogController<void> _eraseController;

  @override
  void initState() {
    super.initState();
    _sub = GlobalStreams.deviceListSubject.listen((
      DeviceListUpdate update,
    ) async {
      final device = update.state.devices.firstWhereOrNull(
        (device) => deviceIdEquals(device.id, widget.deviceId),
      );
      if (device?.name == null) await _eraseController.clearAllActionsNeeded();
      final _deviceWasSome = _device != null;
      final updateIsNone = device == null;
      if (_deviceWasSome && updateIsNone) {
        Navigator.pop(context);
        return;
      }
      setState(() {
        _gotFirstData = true;
        _device = device;
      });
    });
    _eraseController = FullscreenActionDialogController(
      title: 'Erase Device',
      body: (context) {
        final theme = Theme.of(context);
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
      actionButtons: [
        OutlinedButton(child: Text('Cancel'), onPressed: _onCancel),
        DeviceActionHint(),
      ],
      onDismissed: _onCancel,
    );
  }

  void _onCancel() async {
    final id = _device?.id;
    if (id != null) await coord.sendCancel(id: id);
    await _eraseController.clearAllActionsNeeded();
  }

  @override
  void dispose() {
    _sub.cancel();
    _eraseController.dispose();
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
    return Padding(
      padding: EdgeInsets.symmetric(vertical: 40),
      child: Center(heightFactor: 2.1, child: CircularProgressIndicator()),
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
    final needsUpgrade = device.needsFirmwareUpgrade();
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
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Nonces'),
        subtitle: Text('$noncesAvailable'),
        leading: Icon(Icons.numbers_rounded),
        onTap: () =>
            copyAction(context, 'Remaining nonces', '$noncesAvailable'),
      ),
      ListTile(
        contentPadding: EdgeInsets.symmetric(horizontal: 16),
        title: Text('Erase device'),
        subtitle: Text('Delete everything from this device'),
        leading: Icon(Icons.delete_forever_rounded),
        trailing: TextButton(
          onPressed: () => showEraseDialog(context, device.id),
          child: Text('Erase'),
          style: TextButton.styleFrom(foregroundColor: theme.colorScheme.error),
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
          title: Text('Firmware'),
          subtitle: Text(
            device.firmwareDigest,
            style: monospaceTextStyle,
            overflow: TextOverflow.ellipsis,
          ),
          trailing: needsUpgrade
              ? TextButton.icon(
                  onPressed: () async => await widget.firmwareUpgrade(context),
                  label: Text('Upgrade'),
                )
              : Card.outlined(
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      vertical: 4.0,
                      horizontal: 8.0,
                    ),
                    child: Text('Latest'),
                  ),
                ),
          onTap: () =>
              copyAction(context, "Device firmware", device.firmwareDigest),
        ),
        if (!isEmpty) ...advancedRows,
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
    _eraseController.addActionNeeded(context, id);
    await coord.wipeDeviceData(deviceId: id);
  }
}
