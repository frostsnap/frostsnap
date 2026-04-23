import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/stream_ext.dart';

class DeviceActionBackupCheckController with ChangeNotifier {
  final AccessStructure accessStructure;

  FullscreenActionDialogController<CheckBackupState>? _dialogController;

  DeviceId? get activeDeviceId => _dialogController?.actionsNeeded.firstOrNull;

  DeviceActionBackupCheckController({required this.accessStructure});

  @override
  void dispose() {
    _dialogController?.dispose();
    super.dispose();
  }

  Future<CheckBackupState?> show(BuildContext context, DeviceId id) async {
    final exists = accessStructure.devices().any((v) => deviceIdEquals(v, id));
    if (!exists) return null;
    final connected =
        (await GlobalStreams.deviceListSubject.first).state.getDevice(id: id) !=
        null;
    if (!connected) return null;

    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    final shareIndex = accessStructure.getDeviceShareIndex(deviceId: id);
    if (shareIndex == null) return null;

    late final FullscreenActionDialogController<CheckBackupState> controller;
    controller = FullscreenActionDialogController<CheckBackupState>(
      context: context,
      devices: [id],
      title: 'Check Backup on Device',
      body: (context) {
        final theme = Theme.of(context);
        return Text(
          'Check your physical backup by entering it on the device screen.',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        );
      },
      actionButtons: [
        OutlinedButton(
          child: Text('Cancel'),
          onPressed: () => controller.clearAllActionsNeeded(),
        ),
        DeviceActionHint(label: 'Complete quiz on device'),
      ],
    );
    _dialogController = controller;

    try {
      final (state, isCancelled) = await select([
        coord
            .tellDeviceToCheckBackup(
              deviceId: id,
              accessStructureRef: accessStructure.accessStructureRef(),
              shareIndex: shareIndex,
              encryptionKey: encryptionKey,
            )
            .last
            .then((s) => (s, true)),
        controller.awaitDismissed().then((_) => (null, false)),
      ], catchError: (_) => (null, true));

      if (state == null) {
        await coord.cancelProtocol();
        await controller.removeActionNeeded(id);
        if (!isCancelled) return null;
        return null;
      }

      await controller.removeActionNeeded(id);
      return state;
    } finally {
      if (_dialogController == controller) _dialogController = null;
      controller.dispose();
    }
  }
}
