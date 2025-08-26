import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/stream_ext.dart';

class DeviceActionBackupCheckController with ChangeNotifier {
  final AccessStructure accessStructure;

  late final FullscreenActionDialogController<bool> _dialogController;
  final StreamController<void> _cancelButtonController =
      StreamController.broadcast();

  // Currently active device.
  DeviceId? get activeDeviceId => _dialogController.actionsNeeded.firstOrNull;

  String? get walletName => coord
      .getFrostKey(keyId: accessStructure.accessStructureRef().keyId)
      ?.keyName();

  DeviceActionBackupCheckController({required this.accessStructure}) {
    _dialogController = FullscreenActionDialogController(
      title: 'Check Backup on Device',
      body: (context) {
        final deviceId = activeDeviceId;
        final deviceIndex = deviceId == null
            ? 0
            : 1 +
                  accessStructure.devices().indexWhere(
                    (id) => deviceIdEquals(id, deviceId),
                  );
        return Card(
          margin: EdgeInsets.zero,
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: InfoRow.toColumn(context, [
              InfoRow('For key', '#$deviceIndex'),
              InfoRow('Of wallet', walletName ?? ''),
            ]),
          ),
        );
      },
      actionButtons: [
        OutlinedButton(child: Text('Cancel'), onPressed: _onCancel),
        DeviceActionHint(label: 'Enter backup on device'),
      ],
      onDismissed: _onCancel,
    );
  }

  @override
  void dispose() async {
    await _cancelButtonController.close();
    _dialogController.dispose();
    super.dispose();
  }

  void _onCancel() {
    if (_cancelButtonController.isClosed) return;
    _cancelButtonController.add(null);
  }

  Future<bool?> show(BuildContext context, DeviceId id) async {
    final exists = accessStructure.devices().any((v) => deviceIdEquals(v, id));
    if (!exists) return false;
    final connected =
        (await GlobalStreams.deviceListSubject.first).state.getDevice(id: id) !=
        null;
    if (!connected) return false;

    final _ = _dialogController.addActionNeeded(context, id)!;

    final (phase, isCancelled) = await select([
      coord
          .tellDeviceToEnterPhysicalBackup(deviceId: id)
          .last
          .then((s) => (s.entered, true)),
      _cancelButtonController.stream.first.then((_) => (null, false)),
    ], catchError: (_) => (null, true));

    print('phase=$phase, isConnected=$isCancelled');

    if (phase == null) {
      await coord.cancelProtocol();
      await _dialogController.removeActionNeeded(id);
      if (!isCancelled) return null;
      return false;
    }

    final checkOk = coord.checkPhysicalBackup(
      accessStructureRef: accessStructure.accessStructureRef(),
      phase: phase,
    );

    await _dialogController.removeActionNeeded(id);
    return checkOk;
  }
}
