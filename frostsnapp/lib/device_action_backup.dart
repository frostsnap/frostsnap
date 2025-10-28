import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_manager.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

class DeviceActionBackupController with ChangeNotifier {
  final BackupManager backupManager;
  final AccessStructure accessStructure;

  late final FullscreenActionDialogController<bool> _dialogController;
  final StreamController<void> _cancelButtonController =
      StreamController.broadcast();

  // Currently active device.
  DeviceId? get activeDeviceId => _dialogController.actionsNeeded.firstOrNull;

  String? get walletName => coord
      .getFrostKey(keyId: accessStructure.accessStructureRef().keyId)
      ?.keyName();

  DeviceActionBackupController({
    required this.accessStructure,
    required this.backupManager,
  }) {
    _dialogController = FullscreenActionDialogController(
      title: 'Display Backup on Device',
      body: (context) {
        final deviceId = activeDeviceId;
        final deviceIndex = accessStructure.getDeviceShortShareIndex(
          deviceId: deviceId!,
        )!; // critical that we do not display the wrong value here

        return Card(
          margin: EdgeInsets.zero,
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: InfoRow.toColumn(context, [
              InfoRow('For key', '#$deviceIndex'),
              InfoRow('Of wallet', walletName ?? ''),
            ]),
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

  Future<bool> show(BuildContext context, DeviceId id) async {
    final exists = accessStructure.devices().any((v) => deviceIdEquals(v, id));
    if (!exists) return false;
    final connected =
        (await GlobalStreams.deviceListSubject.first).state.getDevice(id: id) !=
        null;
    if (!connected) return false;

    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    await _dialogController.clearAllActionsNeeded();
    final _ = _dialogController.addActionNeeded(context, id)!;

    final isComplete = await Stream<bool>.fromFutures([
      coord
          .displayBackup(
            id: id,
            accessStructureRef: accessStructure.accessStructureRef(),
            encryptionKey: encryptionKey,
          )
          .first,
      _cancelButtonController.stream.first.then((_) => false),
      GlobalStreams.deviceListChangeStream
          .firstWhere(
            (change) =>
                deviceIdEquals(change.device.id, id) &&
                change.kind == DeviceListChangeKind.removed,
          )
          .then((_) => false),
    ]).first.catchError((_) => false);

    if (isComplete) {
      final keyId = accessStructure.masterAppkey().keyId();
      await backupManager.markBackupComplete(deviceId: id, keyId: keyId);
    }
    await coord.cancelProtocol();
    await _dialogController.removeActionNeeded(id);
    return isComplete;
  }
}
