import 'dart:async';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';
import 'package:frostsnap/restoration/material_dialog_card.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

class PlugInBlankView extends StatefulWidget with TitledWidget {
  final Function(ConnectedDevice)? onBlankDeviceConnected;

  const PlugInBlankView({super.key, this.onBlankDeviceConnected});

  @override
  State<PlugInBlankView> createState() => _PlugInBlankViewState();

  @override
  String get titleText => 'Insert blank device';
}

class _PlugInBlankViewState extends State<PlugInBlankView> {
  StreamSubscription? _subscription;
  ConnectedDevice? _connectedDevice;

  late final FullscreenActionDialogController<void> _eraseController;

  @override
  void initState() {
    super.initState();
    _subscription = GlobalStreams.deviceListSubject.listen((update) async {
      ConnectedDevice? connectedDevice;
      for (final candidate in update.state.devices) {
        connectedDevice = candidate;
        if (connectedDevice.name == null) {
          break;
        }
      }
      setState(() {
        _connectedDevice = connectedDevice;
      });
      if (connectedDevice != null && connectedDevice.name == null) {
        widget.onBlankDeviceConnected?.call(connectedDevice);
      }

      if (connectedDevice != null) {
        final device = update.state.devices.firstWhereOrNull(
          (device) => deviceIdEquals(device.id, connectedDevice!.id),
        );
        if (device?.name == null) {
          await _eraseController.clearAllActionsNeeded();
        }
      } else {
        await _eraseController.clearAllActionsNeeded();
      }
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
                title: Text('This will wipe the key from the device.'),
                subtitle: Text(
                  'The device will be rendered blank.\nThis action can not be reverted, and the only way to restore this key is through loading of a backup.',
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
    );
  }

  void _onCancel() async {
    await _eraseController.clearAllActionsNeeded();
  }

  void showEraseDialog(BuildContext context, DeviceId id) async {
    _eraseController.addActionNeeded(context, id);
    await coord.wipeDeviceData(deviceId: id);
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _eraseController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final List<Widget> children;
    if (_connectedDevice != null && _connectedDevice!.name != null) {
      var name = _connectedDevice!.name!;
      children = [
        MaterialDialogCard(
          iconData: Icons.warning_rounded,
          title: Text('Device not blank'),
          content: Text(
            'This device already has data on it. To load a physical backup, it must be erased. Erasing will permanently delete all keys on "${name}".',
          ),
          actions: [
            FilledButton.icon(
              style: FilledButton.styleFrom(
                backgroundColor: theme.colorScheme.error,
                foregroundColor: theme.colorScheme.onError,
              ),
              icon: Icon(Icons.delete),
              label: Text('Erase "$name"'),
              onPressed: () {
                showEraseDialog(context, _connectedDevice!.id);
              },
            ),
          ],
        ),
      ];
    } else {
      children = [
        MaterialDialogCard(
          iconData: Icons.usb_rounded,
          title: Text('Waiting for device'),
          content: Text(
            'Plug in a blank Frostsnap device. This device will be used to load your key from backup.',
          ),
          actions: [CircularProgressIndicator()],
          actionsAlignment: MainAxisAlignment.center,
        ),
      ];
    }
    return Column(
      key: const ValueKey('plugInBlankPrompt'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: children,
    );
  }
}
