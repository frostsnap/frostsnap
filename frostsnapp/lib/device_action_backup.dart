import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
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
  final StreamController<void> _backupRecordedButtonController =
      StreamController.broadcast();

  // Currently active device.
  DeviceId? get activeDeviceId => _dialogController.actionsNeeded.firstOrNull;

  String? get walletName => coord
      .getFrostKey(keyId: accessStructure.accessStructureRef().keyId)
      ?.keyName();

  bool _isShowingBackup = false;

  void _setIsShowing(bool v) {
    if (v == _isShowingBackup) return;
    _isShowingBackup = v;
    if (hasListeners) notifyListeners();
  }

  DeviceActionBackupController({
    required this.accessStructure,
    required this.backupManager,
  }) {
    _dialogController = FullscreenActionDialogController(
      title: 'Display Backup on Device',
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
        ListenableBuilder(
          listenable: this,
          builder: (context, _) {
            return _isShowingBackup
                ? OutlinedButton.icon(
                    label: Text('Mark Backup Recorded'),
                    onPressed: () {
                      if (_backupRecordedButtonController.isClosed) return;
                      _backupRecordedButtonController.add(null);
                    },
                  )
                : DeviceActionHint();
          },
        ),
      ],
      onDismissed: _onCancel,
    );
  }

  @override
  void dispose() async {
    await _cancelButtonController.close();
    await _backupRecordedButtonController.close();
    _dialogController.dispose();
    super.dispose();
  }

  void _onCancel() {
    if (_cancelButtonController.isClosed) return;
    _cancelButtonController.add(null);
  }

  Future<bool> show(BuildContext context, DeviceId id) async {
    _setIsShowing(false);
    final exists = accessStructure.devices().any((v) => deviceIdEquals(v, id));
    if (!exists) return false;
    final connected =
        (await GlobalStreams.deviceListSubject.first).state.getDevice(id: id) !=
        null;
    if (!connected) return false;

    await _dialogController.clearAllActionsNeeded();
    final _ = _dialogController.addActionNeeded(context, id)!;

    final isShowing = await Stream<bool>.fromFutures([
      coord
          .displayBackup(
            id: id,
            accessStructureRef: accessStructure.accessStructureRef(),
          )
          .first,
      _cancelButtonController.stream.first.then((_) => false),
    ]).first.catchError((_) => false);

    if (!isShowing) {
      await coord.cancelProtocol();
      await _dialogController.removeActionNeeded(id);
      return false;
    }

    _setIsShowing(true);

    final isComplete = await Stream<bool>.fromFutures([
      _cancelButtonController.stream.first.then((_) => false),
      _backupRecordedButtonController.stream.first.then((_) => true),
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
