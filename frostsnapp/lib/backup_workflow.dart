import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_gradient_card.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_backup.dart';
import 'package:frostsnap/device_action_backup_check.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';

class DeviceWithShareIndex extends StatelessWidget {
  final int? shareIndex;
  final String deviceName;

  const DeviceWithShareIndex({
    super.key,
    this.shareIndex,
    required this.deviceName,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final defaultTextStyle = DefaultTextStyle.of(context);

    if (shareIndex == null) {
      return Text(deviceName);
    }

    return Row(
      spacing: 4,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          "#$shareIndex",
          style: defaultTextStyle.style.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
            fontWeight: FontWeight.w500,
          ),
        ),
        Flexible(
          child: Text(
            deviceName,
            style: defaultTextStyle.style.copyWith(fontWeight: FontWeight.w500),
          ),
        ),
      ],
    );
  }
}

class BackupChecklist extends StatefulWidget {
  final AccessStructure accessStructure;
  final ScrollController? scrollController;
  final bool showAppBar;

  const BackupChecklist({
    super.key,
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

  void showBackupDialog(BuildContext context, DeviceId deviceId) async {
    await _backupDialogController.show(context, deviceId);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;
    final frostKey = walletCtx.wallet.frostKey()!;
    final accessStructure = frostKey.accessStructures().first;
    final backupStream = walletCtx.backupStream;

    final topBar = TopBarSliver(
      title: Text('Backup keys'),
      leading: IconButton(
        icon: Icon(Icons.arrow_back),
        onPressed: () => Navigator.pop(context),
      ),
      showClose: false,
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

            // Devices are already sorted by share index and contain all metadata
            final deviceInfoList = backupRun.devices;

            final completedDevices = deviceInfoList
                .where((d) => d.complete == true)
                .toList();
            final allComplete =
                completedDevices.length == deviceInfoList.length;
            final devicesLeftToBackup =
                deviceInfoList.length - completedDevices.length;

            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                // Warning about security
                Row(
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    Icon(Icons.warning),
                    SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        "Anyone who can access any ${accessStructure.threshold()} of the ${accessStructure.devices().length} backups for this wallet can take all the Bitcoin. Secure them carefully.",
                        style: theme.textTheme.bodyLarge?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 24),

                // Devices left to back up
                if (devicesLeftToBackup > 0)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 16.0),
                    child: Text(
                      devicesLeftToBackup == 1
                          ? '1 device left to back up'
                          : '$devicesLeftToBackup devices left to back up',
                      style: theme.textTheme.titleMedium,
                    ),
                  ),

                // Animated gradient prompt area
                StreamBuilder<DeviceListUpdate>(
                  stream: GlobalStreams.deviceListSubject,
                  builder: (context, deviceListSnapshot) {
                    final connectedDevices =
                        deviceListSnapshot.data?.state.devices ?? [];
                    final deviceCount = connectedDevices.length;

                    if (deviceCount > 1) {
                      // Multiple devices warning
                      return AnimatedGradientPrompt(
                        dense: false,
                        icon: Icon(
                          Icons.warning_amber_rounded,
                          color: theme.colorScheme.error,
                        ),
                        content: Text(
                          'Multiple devices detected. Please disconnect all but one device.',
                        ),
                      );
                    } else if (deviceCount == 1) {
                      // Single device - show buttons
                      final connectedDevice = connectedDevices.first;
                      final deviceInfo = deviceInfoList.firstWhereOrNull(
                        (d) => deviceIdEquals(d.deviceId, connectedDevice.id),
                      );

                      if (deviceInfo == null) {
                        return AnimatedGradientPrompt(
                          dense: false,
                          icon: Icon(Icons.info_rounded),
                          content: Text(
                            'Unknown device connected. Please check your device.',
                          ),
                        );
                      }

                      return AnimatedGradientPrompt(
                        dense: false,
                        icon: Icon(Icons.usb_rounded),
                        content: Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            Flexible(
                              child: Row(
                                spacing: 4,
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Flexible(
                                    child: DeviceWithShareIndex(
                                      shareIndex: deviceInfo.shareIndex,
                                      deviceName: deviceInfo.deviceName,
                                    ),
                                  ),
                                  Text(' detected'),
                                ],
                              ),
                            ),
                            FilledButton(
                              onPressed: () => showBackupDialog(
                                context,
                                deviceInfo.deviceId,
                              ),
                              child: Text(
                                'Display Backup #${deviceInfo.shareIndex}',
                              ),
                            ),
                          ],
                        ),
                      );
                    } else {
                      // No device
                      return AnimatedGradientPrompt(
                        dense: false,
                        icon: Icon(Icons.usb_rounded),
                        content: Text('Plug in device to back it up'),
                      );
                    }
                  },
                ),

                const SizedBox(height: 24),

                // Scrollable checklist
                ...deviceInfoList.map((device) {
                  // Three states: true (complete), false (needs backup), null (unknown)
                  final IconData icon;
                  final Color color;

                  if (device.complete == true) {
                    icon = Icons.check_circle;
                    color = theme.colorScheme.primary;
                  } else if (device.complete == false) {
                    icon = Icons.circle_outlined;
                    color = theme.colorScheme.onSurfaceVariant;
                  } else {
                    // null - unknown state
                    icon = Icons.help_outline;
                    color = theme.colorScheme.onSurfaceVariant;
                  }

                  return Card(
                    color: theme.colorScheme.surfaceContainerHighest,
                    margin: EdgeInsets.symmetric(vertical: 4.0),
                    child: ListTile(
                      dense: true,
                      leading: Icon(icon, color: color),
                      title: DeviceWithShareIndex(
                        shareIndex: device.shareIndex,
                        deviceName: device.deviceName,
                      ),
                    ),
                  );
                }),
                const SizedBox(height: 24),
                Center(
                  child: FilledButton(
                    onPressed: allComplete
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
        devicesColumn,
        SliverSafeArea(sliver: SliverToBoxAdapter()),
      ],
    );
  }
}
