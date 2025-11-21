import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_backup.dart';
import 'package:frostsnap/device_action_backup_check.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_run.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/theme.dart';
import 'package:glowy_borders/glowy_borders.dart';

class BackupConfirmationDialogContent extends StatelessWidget {
  final int threshold;
  final int totalDevices;
  final String walletName;
  final String deviceName;
  final VoidCallback onCancel;
  final VoidCallback onConfirm;

  const BackupConfirmationDialogContent({
    super.key,
    required this.threshold,
    required this.totalDevices,
    required this.walletName,
    required this.deviceName,
    required this.onCancel,
    required this.onConfirm,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            'Write down the device information on your backup sheet. If you lose this you will still be able to recover the wallet but it is helpful to have.',
            style: theme.textTheme.bodyMedium,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 24),
          Center(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxWidth: 300),
              child: Container(
                padding: const EdgeInsets.all(16.0),
                decoration: BoxDecoration(
                  border: Border.all(color: theme.colorScheme.outline),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Text('Threshold:', style: theme.textTheme.bodyLarge),
                        const SizedBox(width: 8),
                        Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 8,
                          ),
                          decoration: BoxDecoration(
                            border: Border.all(
                              color: theme.colorScheme.outline,
                            ),
                            borderRadius: BorderRadius.circular(4),
                          ),
                          child: Text(
                            '$threshold',
                            style: theme.textTheme.titleLarge?.copyWith(
                              color: theme.colorScheme.primary,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        Text('of', style: theme.textTheme.bodyLarge),
                        const SizedBox(width: 8),
                        Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 8,
                          ),
                          decoration: BoxDecoration(
                            border: Border.all(
                              color: theme.colorScheme.outline,
                            ),
                            borderRadius: BorderRadius.circular(4),
                          ),
                          child: Text(
                            '$totalDevices',
                            style: theme.textTheme.titleLarge?.copyWith(
                              color: theme.colorScheme.primary,
                              fontWeight: FontWeight.bold,
                            ),
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 16),
                    Center(
                      child: ConstrainedBox(
                        constraints: BoxConstraints(maxWidth: 400),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.center,
                          children: [
                            Text(
                              walletName,
                              style: theme.textTheme.titleLarge?.copyWith(
                                color: theme.colorScheme.primary,
                                fontWeight: FontWeight.bold,
                              ),
                              textAlign: TextAlign.center,
                            ),
                            const SizedBox(height: 2),
                            Container(
                              height: 1,
                              width: double.infinity,
                              color: theme.colorScheme.outline,
                            ),
                            const SizedBox(height: 2),
                            Text(
                              'Wallet Name',
                              style: theme.textTheme.bodySmall?.copyWith(
                                color: theme.colorScheme.onSurfaceVariant,
                              ),
                              textAlign: TextAlign.center,
                            ),
                          ],
                        ),
                      ),
                    ),
                    const SizedBox(height: 24),
                    Center(
                      child: ConstrainedBox(
                        constraints: BoxConstraints(maxWidth: 400),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.center,
                          children: [
                            Text(
                              deviceName,
                              style: theme.textTheme.titleLarge?.copyWith(
                                color: theme.colorScheme.primary,
                                fontWeight: FontWeight.bold,
                              ),
                              textAlign: TextAlign.center,
                            ),
                            const SizedBox(height: 2),
                            Container(
                              height: 1,
                              width: double.infinity,
                              color: theme.colorScheme.outline,
                            ),
                            const SizedBox(height: 2),
                            Text(
                              'Device Name',
                              style: theme.textTheme.bodySmall?.copyWith(
                                color: theme.colorScheme.onSurfaceVariant,
                              ),
                              textAlign: TextAlign.center,
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          const SizedBox(height: 24),
          Row(
            mainAxisAlignment: MainAxisAlignment.end,
            spacing: 8,
            children: [
              TextButton(onPressed: onCancel, child: Text('Cancel')),
              FilledButton(
                onPressed: onConfirm,
                child: Text('Show secret backup'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

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
    final confirmed = await showBackupConfirmationDialog(context, deviceId);
    if (confirmed == true) {
      await _backupDialogController.show(context, deviceId);
    }
  }

  Future<bool?> showBackupConfirmationDialog(
    BuildContext context,
    DeviceId deviceId,
  ) async {
    final walletCtx = WalletContext.of(context)!;
    final accessStructure = widget.accessStructure;
    final deviceName = coord.getDeviceName(id: deviceId) ?? 'Unknown Device';
    final walletName = walletCtx.wallet.frostKey()?.keyName() ?? '';

    return await showBottomSheetOrDialog<bool>(
      context,
      title: Text('Record backup information'),
      builder: (context, scrollController) {
        return BackupConfirmationDialogContent(
          threshold: accessStructure.threshold(),
          totalDevices: accessStructure.devices().length,
          walletName: walletName,
          deviceName: deviceName,
          onCancel: () => Navigator.pop(context, false),
          onConfirm: () => Navigator.pop(context, true),
        );
      },
    );
  }

  void showCheckDialog(BuildContext context, DeviceId deviceId) async {
    final result = await _checkDialogController.show(context, deviceId);
    if (result == true) {
      await showBackupOkayDialog(context);
    } else if (result == false) {
      final tryAgain = await showBackupInvalidDialog(context);
      if (tryAgain) {
        showCheckDialog(context, deviceId);
      }
    }
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

            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Connect the devices one at a time to back up their keys.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 16),
                // Explanatory text
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: theme.colorScheme.surfaceContainerHighest,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Icon(
                        Icons.info_outline,
                        color: theme.colorScheme.primary,
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Text(
                          "Each backup is an unencrypted copy of the device's key, allowing you to recover your wallet if the device is lost or damaged.",
                          style: theme.textTheme.bodyMedium,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 12),
                // Warning about security
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: theme.colorScheme.surfaceContainerHighest,
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
                          "Anyone who can access any ${accessStructure.threshold()} of the ${accessStructure.devices().length} keys for this wallet can take all the bitcoin. Secure them carefully.",
                          style: theme.textTheme.bodyMedium,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(height: 24),

                // Combined gradient border with device status and progress
                StreamBuilder<DeviceListUpdate>(
                  stream: GlobalStreams.deviceListSubject,
                  builder: (context, deviceListSnapshot) {
                    final connectedDevices =
                        deviceListSnapshot.data?.state.devices ?? [];
                    final deviceCount = connectedDevices.length;

                    Widget deviceStatusContent;
                    IconData statusIcon;
                    Color? statusIconColor;

                    if (deviceCount > 1) {
                      // Multiple devices warning
                      statusIcon = Icons.warning_sharp;
                      statusIconColor = theme.colorScheme.primary;
                      deviceStatusContent = Text(
                        'Multiple devices connected. Connect only one device at a time.',
                      );
                    } else if (deviceCount == 1) {
                      // Single device - show buttons
                      final connectedDevice = connectedDevices.first;
                      final connectedDeviceId = connectedDevice.id;

                      // Find the share index for the connected device
                      final shareIndex = accessStructure
                          .getDeviceShortShareIndex(
                            deviceId: connectedDeviceId,
                          );

                      // Find backup info for this share index
                      final deviceInfo = shareIndex != null
                          ? deviceInfoList.firstWhereOrNull(
                              (d) => d.shareIndex == shareIndex,
                            )
                          : null;

                      if (deviceInfo == null) {
                        statusIcon = Icons.info_rounded;
                        statusIconColor = null;
                        deviceStatusContent = Text(
                          'Unknown device connected. Please check your device.',
                        );
                      } else {
                        statusIcon = Icons.usb_rounded;
                        statusIconColor = theme.colorScheme.primary;
                        deviceStatusContent = Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            Flexible(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  DeviceWithShareIndex(
                                    shareIndex: deviceInfo.shareIndex,
                                    deviceName:
                                        coord.getDeviceName(
                                          id: deviceInfo.deviceId,
                                        ) ??
                                        '',
                                  ),
                                  Text(
                                    'connected',
                                    style: theme.textTheme.bodySmall?.copyWith(
                                      color: theme.colorScheme.onSurfaceVariant,
                                    ),
                                  ),
                                ],
                              ),
                            ),
                            if (deviceInfo.complete != false)
                              Row(
                                spacing: 8,
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  FilledButton(
                                    onPressed: () => showBackupDialog(
                                      context,
                                      connectedDeviceId,
                                    ),
                                    child: Text('Backup'),
                                  ),
                                  FilledButton(
                                    onPressed: () => showCheckDialog(
                                      context,
                                      connectedDeviceId,
                                    ),
                                    child: Text('Check'),
                                  ),
                                ],
                              )
                            else
                              FilledButton(
                                onPressed: () => showBackupDialog(
                                  context,
                                  connectedDeviceId,
                                ),
                                child: Text('Backup'),
                              ),
                          ],
                        );
                      }
                    } else {
                      // No device
                      statusIcon = Icons.usb_rounded;
                      statusIconColor = null;
                      deviceStatusContent = Text(
                        'Plug in device to back it up',
                      );
                    }

                    return AnimatedGradientBorder(
                      stretchAlongAxis: true,
                      borderSize: 1.0,
                      glowSize: 5.0,
                      animationTime: 6,
                      borderRadius: BorderRadius.circular(12.0),
                      gradientColors: [
                        theme.colorScheme.outlineVariant,
                        theme.colorScheme.primary,
                        theme.colorScheme.secondary,
                        theme.colorScheme.tertiary,
                      ],
                      child:
                          (Widget child) {
                            return Card.filled(
                              margin: EdgeInsets.all(0.0),
                              color: theme.colorScheme.surfaceContainerHigh,
                              child: child,
                            );
                          }(
                            Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: [
                                // Device status section
                                Padding(
                                  padding: const EdgeInsets.all(16.0),
                                  child: ConstrainedBox(
                                    constraints: BoxConstraints(minHeight: 48),
                                    child: Row(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.center,
                                      children: [
                                        Icon(
                                          statusIcon,
                                          color: statusIconColor,
                                        ),
                                        const SizedBox(width: 12),
                                        Expanded(child: deviceStatusContent),
                                      ],
                                    ),
                                  ),
                                ),
                                Divider(height: 1),
                                // Device checklist with progress counter
                                Stack(
                                  children: [
                                    Column(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.stretch,
                                      children: () {
                                        // Group devices by share index
                                        final devicesByShareIndex =
                                            <int, List<BackupDevice>>{};
                                        for (final device in deviceInfoList) {
                                          devicesByShareIndex
                                              .putIfAbsent(
                                                device.shareIndex,
                                                () => [],
                                              )
                                              .add(device);
                                        }

                                        // Create list tiles for each share index
                                        return devicesByShareIndex.entries.map((
                                          entry,
                                        ) {
                                          final shareIndex = entry.key;
                                          final devices = entry.value;

                                          // All devices with the same share index have the same completion status
                                          final complete =
                                              devices.first.complete;

                                          IconData icon;
                                          Color color;

                                          if (complete == false) {
                                            icon = Icons.circle_outlined;
                                            color = theme
                                                .colorScheme
                                                .onSurfaceVariant;
                                          } else {
                                            // complete == true or complete == null (treat null as backed up)
                                            icon = Icons.check_circle;
                                            color = theme.colorScheme.primary;
                                          }

                                          return ListTile(
                                            dense: true,
                                            leading: Icon(icon, color: color),
                                            title: Row(
                                              spacing: 4,
                                              crossAxisAlignment:
                                                  CrossAxisAlignment.center,
                                              mainAxisSize: MainAxisSize.min,
                                              children: [
                                                Text(
                                                  "#$shareIndex",
                                                  style: theme
                                                      .textTheme
                                                      .bodyMedium
                                                      ?.copyWith(
                                                        color: theme
                                                            .colorScheme
                                                            .onSurfaceVariant,
                                                        fontWeight:
                                                            FontWeight.w500,
                                                      ),
                                                ),
                                                Flexible(
                                                  child: Padding(
                                                    padding:
                                                        const EdgeInsets.only(
                                                          right: 80,
                                                        ),
                                                    child: Text(
                                                      devices
                                                          .map(
                                                            (d) =>
                                                                coord.getDeviceName(
                                                                  id: d
                                                                      .deviceId,
                                                                ) ??
                                                                '',
                                                          )
                                                          .join(', '),
                                                      style: theme
                                                          .textTheme
                                                          .bodyMedium
                                                          ?.copyWith(
                                                            fontWeight:
                                                                FontWeight.w500,
                                                          ),
                                                      softWrap: true,
                                                    ),
                                                  ),
                                                ),
                                              ],
                                            ),
                                          );
                                        }).toList();
                                      }(),
                                    ),
                                    Positioned(
                                      top: 12,
                                      right: 16,
                                      child: Text(
                                        '${completedDevices.length}/${deviceInfoList.length} complete',
                                        style: theme.textTheme.bodySmall
                                            ?.copyWith(
                                              color: theme
                                                  .colorScheme
                                                  .onSurfaceVariant,
                                            ),
                                      ),
                                    ),
                                  ],
                                ),
                              ],
                            ),
                          ),
                    );
                  },
                ),
                const SizedBox(height: 24),
                Center(
                  child: allComplete
                      ? FilledButton(
                          onPressed: () =>
                              Navigator.popUntil(context, (r) => r.isFirst),
                          child: const Text('Done'),
                        )
                      : TextButton(
                          onPressed: () => Navigator.pop(context),
                          child: const Text('Finish later'),
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
