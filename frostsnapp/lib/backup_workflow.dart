import 'dart:async';
import 'dart:io';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/contexts.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/show_backup.dart';
import 'package:frostsnapp/src/rust/api.dart';
import 'package:frostsnapp/src/rust/api/coordinator.dart';

class BackupChecklist extends StatelessWidget {
  final ScrollController? scrollController;
  final AccessStructure accessStructure;
  final bool showAppBar;
  const BackupChecklist({
    super.key,
    this.scrollController,
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
    final frostKey = walletCtx.wallet.frostKey()!;
    final accessStructure = frostKey.accessStructures().first;
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
              'The Frostsnap',
              Platform.isAndroid || Platform.isIOS
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
        child: DefaultTextStyle(
          style:
              theme.textTheme.bodyLarge?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ) ??
              TextStyle(),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Travel to the locations where you will store each Frostsnap.',
              ),
              const SizedBox(height: 16),
              Text('Make sure to bring:'),
              const SizedBox(height: 16),
              Card(
                color: theme.colorScheme.surfaceContainerHighest,
                margin: EdgeInsets.all(0.0),
                child: Column(children: toBringList),
              ),
              const SizedBox(height: 16),
              Row(
                crossAxisAlignment: CrossAxisAlignment.center,
                children: [
                  Icon(Icons.warning),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      "Anyone who can access any ${accessStructure.threshold()} of the ${accessStructure.devices().length} Frostsnaps in this wallet can take all the Bitcoin. Secure them carefully.",
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 16),
              Text(
                'When you arrive at each location press the button to show the backup:',
              ),
            ],
          ),
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
                      (deviceId) =>
                          backupRun.devices.any(
                            (d) =>
                                deviceIdEquals(d.$1, deviceId) && d.$2 != null,
                          ) ||
                          // if the device is not mentioned in the list assume it's completed
                          backupRun.devices.none(
                            (d) => deviceIdEquals(d.$1, deviceId),
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
                        child:
                            isCompleted ? const Text('Show') : Text("I'm here"),
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
      controller: scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [if (showAppBar) appBar, infoColumn, devicesColumn],
    );
  }
}
