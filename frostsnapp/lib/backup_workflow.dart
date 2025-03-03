import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/show_backup.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class BackupChecklist extends StatelessWidget {
  final AccessStructure accessStructure;
  final bool showAppBar;
  const BackupChecklist({
    super.key,
    required this.accessStructure,
    this.showAppBar = false,
  });

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

  showThatWasQuickDialog(BuildContext context, DeviceId deviceId) async {
    final manager = FrostsnapContext.of(context)!.backupManager;
    final walletCtx = WalletContext.of(context)!;

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
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('A backup of a device was performed recently.'),
                  SizedBox(height: 12),
                  Text(
                    'Make sure your devices are secured in separate locations!',
                  ),
                ],
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(false),
                  child: Text('Back'),
                ),
                FilledButton(
                  onPressed: () => Navigator.of(context).pop(true),
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
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;
    final backupStream = walletCtx.backupStream;

    final appBar = SliverAppBar(
      title: const Text('Backup Checklist'),
      titleTextStyle: theme.textTheme.titleMedium,
      centerTitle: true,
      backgroundColor: theme.colorScheme.surfaceContainerLow,
      pinned: false,
      stretch: true,
      forceMaterialTransparency: true,
      automaticallyImplyLeading: false,
      leading: IconButton(
        onPressed: () => Navigator.pop(context),
        icon: Icon(Icons.close),
      ),
    );

    final toBringList =
        [
              'Your Frostsnap device',
              Platform.isAndroid
                  ? 'This phone'
                  : Platform.isIOS
                  ? 'This phone'
                  : 'This laptop',
              'A backup card',
              'A pencil',
            ]
            .map(
              (item) => ListTile(
                dense: true,
                leading: Icon(Icons.check),
                title: Text(item),
              ),
            )
            .toList();

    final infoColumn = SliverToBoxAdapter(
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
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
            Card(
              color: theme.colorScheme.surfaceContainerHighest,
              margin: EdgeInsets.all(0.0),
              child: Column(children: toBringList),
            ),
            const SizedBox(height: 32),
            Text(
              'Travel to separate locations to secure each device:',
              style: theme.textTheme.bodyLarge?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
            //const SizedBox(height: 24),
          ],
        ),
      ),
    );

    final devicesColumn = SliverToBoxAdapter(
      child: Padding(
        padding: const EdgeInsets.all(16.0),
        child: StreamBuilder(
          stream: backupStream,
          builder: (context, snapshot) {
            if (!snapshot.hasData) {
              return const Center(
                child: Padding(
                  padding: EdgeInsets.all(16.0),
                  child: Center(child: LinearProgressIndicator()),
                ),
              );
            }

            final backupRun = snapshot.data!;
            final allDevices = accessStructure.devices();
            final completedDevices =
                allDevices
                    .where(
                      (deviceId) => backupRun.devices.any(
                        (d) => deviceIdEquals(d.$1, deviceId) && d.$2 != null,
                      ),
                    )
                    .toList();

            final devicesList = allDevices.map((deviceId) {
              final deviceName = coord.getDeviceName(id: deviceId) ?? "";
              final isCompleted = completedDevices.any(
                (id) => deviceIdEquals(id, deviceId),
              );

              return Card(
                color: theme.colorScheme.surfaceContainerHighest,
                margin: EdgeInsets.symmetric(vertical: 8.0, horizontal: 4.0),
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
                            foregroundColor: theme.colorScheme.onSurfaceVariant,
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
                        onPressed:
                            () async =>
                                await showThatWasQuickDialog(context, deviceId),
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
              );
            });

            return Column(
              children: [
                ...devicesList,
                const SizedBox(height: 24),
                Center(
                  child: FilledButton(
                    onPressed:
                        completedDevices.length == allDevices.length
                            ? () => Navigator.pop(context)
                            : null,
                    child: const Text('Done'),
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );

    return CustomScrollView(
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [if (showAppBar) appBar, infoColumn, devicesColumn],
    );
  }
}
