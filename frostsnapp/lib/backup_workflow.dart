import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/show_backup.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class BackupChecklistPage extends StatelessWidget {
  final AccessStructure accessStructure;
  const BackupChecklistPage({super.key, required this.accessStructure});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final backgroundColor = ElevationOverlay.applySurfaceTint(
      theme.colorScheme.surface,
      theme.colorScheme.surfaceTint,
      0,
    );
    return Scaffold(
      backgroundColor: backgroundColor,
      appBar: FsAppBar(title: const Text('Security Checklist')),
      body: BackupChecklist(accessStructure: accessStructure),
    );
  }
}

class BackupChecklist extends StatelessWidget {
  final AccessStructure accessStructure;
  const BackupChecklist({super.key, required this.accessStructure});

  Future<void> _handleBackupDevice(
    BuildContext context,
    DeviceId deviceId,
  ) async {
    final manager = FrostsnapContext.of(context)!.backupManager;
    final keyId = accessStructure.accessStructureRef().keyId;
    final completed = await backupDeviceDialog(
      context,
      deviceId: deviceId,
      accessStructure: accessStructure,
    );
    if (completed) {
      await manager.markBackupComplete(deviceId: deviceId, keyId: keyId);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;
    final backupStream = walletCtx.backupStream;
    final manager = FrostsnapContext.of(context)!.backupManager;

    return StreamBuilder<BackupRun>(
      stream: backupStream,
      builder: (context, snapshot) {
        if (!snapshot.hasData) {
          return const Center(child: CircularProgressIndicator());
        }

        final backupRun = snapshot.data;
        final allDevices = accessStructure.devices();
        final completedDevices =
            backupRun == null
                ? <DeviceId>[]
                : allDevices
                    .where(
                      (deviceId) => backupRun.devices.any(
                        (d) => deviceIdEquals(d.$1, deviceId) && d.$2 != null,
                      ),
                    )
                    .toList();

        return ListView(
          padding: const EdgeInsets.all(16.0),
          children: [
            Text(
              'To secure this wallet, you will store your devices in several geographically separate locations.',
              style: theme.textTheme.titleMedium,
            ),
            const SizedBox(height: 12),
            Text(
              'At each location, you will create a backup to store alongside your device.',
              style: theme.textTheme.bodyLarge?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: 32),
            Text(
              'Make sure to bring:',
              style: theme.textTheme.titleMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: 16),
            Container(
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: theme.colorScheme.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(12),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final item in [
                    'Your Frostsnap device',
                    Platform.isAndroid
                        ? 'This phone'
                        : Platform.isIOS
                        ? 'This phone'
                        : 'This laptop',
                    'A backup card',
                    'A pencil',
                  ]) ...[
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8.0),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            '–  ',
                            style: theme.textTheme.bodyLarge?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                            ),
                          ),
                          Expanded(
                            child: Text(
                              item,
                              style: theme.textTheme.bodyLarge?.copyWith(
                                color: theme.colorScheme.onSurfaceVariant,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 32),
            Text(
              'Travel to separate locations to secure each device:',
              style: theme.textTheme.bodyLarge?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            const SizedBox(height: 24),
            // Device list with elevated cards
            ...allDevices.map((deviceId) {
              final deviceName = coord.getDeviceName(id: deviceId) ?? "";
              final isCompleted = completedDevices.any(
                (d) => deviceIdEquals(d, deviceId),
              );

              return Padding(
                padding: const EdgeInsets.only(bottom: 4.0),
                child: Card(
                  elevation: 2,
                  color: theme.colorScheme.surfaceContainerHighest,
                  child: ListTile(
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 16.0,
                      vertical: 8.0,
                    ),
                    title: Text(
                      deviceName,
                      style: theme.textTheme.titleMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (isCompleted)
                          FilledButton(
                            style: FilledButton.styleFrom(
                              backgroundColor:
                                  theme.colorScheme.surfaceContainerLow,
                              foregroundColor:
                                  theme.colorScheme.onSurfaceVariant,
                            ),
                            onPressed: () async {
                              await verifyBackup(
                                context,
                                deviceId,
                                accessStructure.accessStructureRef(),
                              );
                            },
                            child: const Text('Check'),
                          ),
                        const SizedBox(width: 8),
                        FilledButton(
                          style:
                              isCompleted
                                  ? FilledButton.styleFrom(
                                    backgroundColor:
                                        theme.colorScheme.surfaceContainerLow,
                                    foregroundColor:
                                        theme.colorScheme.onSurfaceVariant,
                                  )
                                  : FilledButton.styleFrom(
                                    backgroundColor: theme.colorScheme.primary,
                                  ),
                          onPressed: () async {
                            final shouldWarn = manager.shouldQuickBackupWarn(
                              keyId: walletCtx.wallet.keyId(),
                              deviceId: deviceId,
                            );
                            if (shouldWarn) {
                              final result = await showDialog<bool>(
                                context: context,
                                builder:
                                    (BuildContext context) => AlertDialog(
                                      title: Text('That was quick!'),
                                      content: const Column(
                                        mainAxisSize: MainAxisSize.min,
                                        crossAxisAlignment:
                                            CrossAxisAlignment.start,
                                        children: [
                                          Text(
                                            'A backup of a device was performed recently.',
                                          ),
                                          SizedBox(height: 12),
                                          Text(
                                            'Make sure your devices are secured in separate locations!',
                                          ),
                                        ],
                                      ),
                                      actions: [
                                        FilledButton(
                                          onPressed:
                                              () => Navigator.of(
                                                context,
                                              ).pop(false),
                                          child: Text('Back'),
                                        ),
                                        FilledButton(
                                          onPressed:
                                              () => Navigator.of(
                                                context,
                                              ).pop(true),
                                          child: Text('Show Backup'),
                                        ),
                                      ],
                                    ),
                              );
                              if (context.mounted && result == true) {
                                await _handleBackupDevice(context, deviceId);
                              }
                            } else {
                              await _handleBackupDevice(context, deviceId);
                            }
                          },
                          child: const Text('Backup'),
                        ),
                        const SizedBox(width: 16),
                        Icon(
                          isCompleted
                              ? Icons.check_circle
                              : Icons.circle_outlined,
                          color:
                              isCompleted
                                  ? theme.colorScheme.primary
                                  : theme.colorScheme.onSurfaceVariant,
                        ),
                      ],
                    ),
                  ),
                ),
              );
            }),
            if (completedDevices.length == allDevices.length) ...[
              const SizedBox(height: 24),
              Center(
                child: FilledButton(
                  onPressed: () {
                    Navigator.pop(context);
                  },
                  child: const Text('Done'),
                ),
              ),
            ],
          ],
        );
      },
    );
  }
}
