import 'dart:io';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_backup.dart';
import 'package:frostsnap/device_action_backup_check.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_manager.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/theme.dart';

class BackupChecklist extends StatefulWidget {
  final BackupManager backupManager;
  final AccessStructure accessStructure;
  final ScrollController? scrollController;
  final bool showAppBar;

  const BackupChecklist({
    super.key,
    required this.backupManager,
    required this.accessStructure,
    this.scrollController,
    this.showAppBar = false,
  });

  @override
  State<BackupChecklist> createState() => _BackupChecklistState();
}

class _BackupChecklistState extends State<BackupChecklist> {
  late final DeviceActionBackupController _backupDialogController;
  late final DeviceActionBackupCheckController _checkDialogController;

  @override
  void initState() {
    super.initState();
    _backupDialogController = DeviceActionBackupController(
      accessStructure: widget.accessStructure,
      backupManager: widget.backupManager,
    );
    _checkDialogController = DeviceActionBackupCheckController(
      accessStructure: widget.accessStructure,
    );
  }

  @override
  void dispose() {
    _backupDialogController.dispose();
    _checkDialogController.dispose();
    super.dispose();
  }

  Future<bool> showBackupInvalidDialog(BuildContext context) async {
    final tryAgain = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (BuildContext context) => AlertDialog(
        title: Text('Backup check failed'),
        content: ConstrainedBox(
          constraints: BoxConstraints(maxWidth: 480),
          child: Text(
            'This can happen if the device gets disconnected, or your backup is invalid/inputted incorrectly.',
          ),
        ),
        actionsAlignment: MainAxisAlignment.spaceBetween,
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text('Exit'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text('Try again'),
          ),
        ],
      ),
    );
    return tryAgain ?? false;
  }

  Future<void> showBackupOkayDialog(BuildContext context) async {
    return await showDialog<void>(
      context: context,
      barrierDismissible: true,
      builder: (BuildContext context) => AlertDialog(
        title: Text('Backup is valid'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text('Done'),
          ),
        ],
      ),
    );
  }

  void maybeShowThatWasQuickDialog(
    BuildContext context,
    DeviceId deviceId,
  ) async {
    final manager = FrostsnapContext.of(context)!.backupManager;
    final walletCtx = WalletContext.of(context)!;

    final shouldWarn = manager.shouldQuickBackupWarn(
      keyId: walletCtx.wallet.keyId(),
      deviceId: deviceId,
    );
    if (shouldWarn) {
      final result = await showDialog<bool>(
        context: context,
        builder: (BuildContext context) => AlertDialog(
          title: Text('That was quick!'),
          content: const Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            spacing: 12,
            children: [
              Text('A backup of a device was performed recently.'),
              Text('Make sure your devices are secured in separate locations!'),
            ],
          ),
          actionsAlignment: MainAxisAlignment.spaceBetween,
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(false),
              child: Text('Back'),
            ),
            TextButton(
              onPressed: () => Navigator.of(context).pop(true),
              child: Text('Show Backup'),
            ),
          ],
        ),
      );
      if (context.mounted && result == true) {
        await _backupDialogController.show(context, deviceId);
      }
    } else {
      await _backupDialogController.show(context, deviceId);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;
    final frostKey = walletCtx.wallet.frostKey()!;
    final accessStructure = frostKey.accessStructures().first;
    final backupStream = walletCtx.backupStream;

    // final appBar = SliverAppBar(
    //   title: const Text('Backup Checklist'),
    //   titleTextStyle: theme.textTheme.titleMedium,
    //   centerTitle: true,
    //   backgroundColor: theme.colorScheme.surfaceContainerLow,
    //   pinned: false,
    //   stretch: true,
    //   forceMaterialTransparency: true,
    //   automaticallyImplyLeading: false,
    //   leading: IconButton(
    //     onPressed: () => Navigator.pop(context),
    //     icon: Icon(Icons.close),
    //   ),
    // );

    final topBar = TopBarSliver(
      title: Text('Backup checklist'),
      leading: IconButton(
        icon: Icon(Icons.arrow_back),
        onPressed: () => Navigator.pop(context),
      ),
      showClose: false,
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
            final completedDevices = allDevices
                .where(
                  (deviceId) =>
                      backupRun.devices.any(
                        (d) => deviceIdEquals(d.$1, deviceId) && d.$2 != null,
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
                  leading: Icon(
                    isCompleted ? Icons.check_circle : Icons.circle_outlined,
                    color: isCompleted
                        ? theme.colorScheme.primary
                        : theme.colorScheme.onSurfaceVariant,
                  ),
                  trailing: StreamBuilder(
                    stream: GlobalStreams.deviceListSubject,
                    builder: (context, deviceListSnapshot) {
                      final connectedDevice = deviceListSnapshot.data?.state
                          .getDevice(id: deviceId);
                      return Row(
                        mainAxisSize: MainAxisSize.min,
                        spacing: 8,
                        children: [
                          if (connectedDevice == null) Text('Disconnected'),
                          if (isCompleted && connectedDevice != null)
                            FilledButton(
                              style: FilledButton.styleFrom(
                                backgroundColor:
                                    theme.colorScheme.surfaceContainerLow,
                                foregroundColor:
                                    theme.colorScheme.onSurfaceVariant,
                              ),
                              onPressed: () async {
                                while (true) {
                                  final isBackupValid =
                                      await _checkDialogController.show(
                                        context,
                                        deviceId,
                                      );
                                  if (isBackupValid == null) /* cancelled */
                                    return;
                                  if (isBackupValid) {
                                    await showBackupOkayDialog(context);
                                    return;
                                  }

                                  final tryAgain =
                                      await showBackupInvalidDialog(context);
                                  if (!tryAgain) return;
                                }
                              },
                              child: const Text('Check'),
                            ),
                          if (connectedDevice != null)
                            FilledButton(
                              style: isCompleted
                                  ? FilledButton.styleFrom(
                                      backgroundColor:
                                          theme.colorScheme.surfaceContainerLow,
                                      foregroundColor:
                                          theme.colorScheme.onSurfaceVariant,
                                    )
                                  : FilledButton.styleFrom(
                                      backgroundColor:
                                          theme.colorScheme.primary,
                                    ),
                              onPressed: () => maybeShowThatWasQuickDialog(
                                context,
                                deviceId,
                              ),
                              child: isCompleted
                                  ? const Text('Show')
                                  : Text("I'm here"),
                            ),
                        ],
                      );
                    },
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
                    onPressed: completedDevices.length == allDevices.length
                        ? () => Navigator.popUntil(context, (r) => r.isFirst)
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
      controller: widget.scrollController,
      shrinkWrap: true,
      physics: ClampingScrollPhysics(),
      slivers: [
        topBar,
        infoColumn,
        devicesColumn,
        SliverSafeArea(sliver: SliverToBoxAdapter()),
      ],
    );
  }
}
