import 'dart:async';

import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';

class DeviceActionBackupController with ChangeNotifier {
  final AccessStructure accessStructure;

  late final FullscreenActionDialogController<bool> _dialogController;

  // Currently active device.
  DeviceId? get activeDeviceId => _dialogController.actionsNeeded.firstOrNull;

  String? get walletName => coord
      .getFrostKey(keyId: accessStructure.accessStructureRef().keyId)
      ?.keyName();

  DeviceActionBackupController({required this.accessStructure}) {
    _dialogController = FullscreenActionDialogController(
      title: 'Record key backup',
      body: (context) {
        final theme = Theme.of(context);

        return Card(
          margin: EdgeInsets.zero,
          child: Padding(
            padding: const EdgeInsets.all(16.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'The device is displaying the key backup. Write down the:',
                  style: theme.textTheme.bodyLarge,
                ),
                const SizedBox(height: 16),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('1. ', style: theme.textTheme.bodyLarge),
                    Expanded(
                      child: Text(
                        'Key number',
                        style: theme.textTheme.bodyLarge,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('2. ', style: theme.textTheme.bodyLarge),
                    Expanded(
                      child: Text(
                        'All 25 words in order',
                        style: theme.textTheme.bodyLarge,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 16),
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: theme.colorScheme.errorContainer,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Icon(
                        Icons.warning_amber_rounded,
                        color: theme.colorScheme.error,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Text(
                          'This key backup is secret information. Anyone with access to ${accessStructure.threshold()} of the ${accessStructure.devices().length} keys can steal all your bitcoin.',
                          style: theme.textTheme.bodyMedium?.copyWith(
                            color: theme.colorScheme.onErrorContainer,
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
      actionButtons: [
        OutlinedButton(child: Text('Cancel'), onPressed: _onCancel),
        DeviceActionHint(label: "Write down backup", icon: Icons.edit_note),
      ],
      onDismissed: _onCancel,
    );
  }

  @override
  void dispose() async {
    _dialogController.dispose();
    super.dispose();
  }

  void _onCancel() {
    coord.cancelProtocol();
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

    bool isComplete = false;
    await for (final state in coord.displayBackup(
      id: id,
      accessStructureRef: accessStructure.accessStructureRef(),
      encryptionKey: encryptionKey,
    )) {
      if (state.confirmed) {
        isComplete = true;
        final accessStructureRef = accessStructure.accessStructureRef();
        final shareIndex = accessStructure.getDeviceShortShareIndex(
          deviceId: id,
        )!;
        await coord.markBackupComplete(
          accessStructureRef: accessStructureRef,
          shareIndex: shareIndex,
        );
      }

      if (state.closeDialog) {
        break;
      }
    }

    await _dialogController.removeActionNeeded(id);
    return isComplete;
  }
}
