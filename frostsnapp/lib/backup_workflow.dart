import 'package:collection/collection.dart';
import 'package:dynamic_color/dynamic_color.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_run.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/verifiable_card/alarm_view.dart';
import 'package:frostsnap/verifiable_card/card_flow.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';
import 'package:frostsnap/verifiable_card/card_platform.dart';
import 'package:frostsnap/wallet_key_mismatch.dart';
import 'package:glowy_borders/glowy_borders.dart';

extension BackupDeviceX on BackupDevice {
  /// A share counts as backed up when it has been explicitly marked
  /// complete, OR when there is no tracking record at all (legacy wallets
  /// created before backup-run tracking existed). Only `complete == false`
  /// means a backup run is in progress and this share still needs recording.
  /// A card share is backed up only once its card is checked: the receive
  /// lock and the banner wait for PASSED (design §3.10).
  bool get isBackedUp => switch (cardStatus) {
    null => complete != false,
    final status => status == CardBackupStatus.checked,
  };

  /// Both halves of the card are recorded, or the share was marked by hand:
  /// the backup run's step, which a card's check comes after.
  bool get isRecorded => complete == true;

  /// No card status: a key this install didn't create as a card key and
  /// holds no card record of. One restored from its cards has one card per
  /// share, so it offers no backup and no check (design §0, §5.4); a 25-word
  /// key from before cards is one too (spec PK-O24). Owner B15: on an install
  /// that didn't create the wallet, a card written here unverified has a
  /// status, and is never backed up in the checked sense.
  bool get restoredFromCard => cardStatus == null;
}

extension BackupRunX on BackupRun {
  bool get isComplete => devices.every((d) => d.isBackedUp);
}

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
  // Every new key is a card, so the 25-word display and check from the
  // app aren't offered: [Backup] and [Check] open the card flow.

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

  /// A card flow is opening or open, so a second tap on [Backup] doesn't
  /// start another one for the same device.
  bool _cardFlowOpen = false;

  /// A card key's [Backup] and [Check]: the split card flow on [deviceId],
  /// which also runs a later check of a PASSED card (design §3.9). [Backup]
  /// shows the standard "Record backup information" sheet first.
  void showCardFlow(
    BuildContext context,
    DeviceId deviceId, {
    bool infoSheet = false,
  }) async {
    if (!cardFlowsSupported) {
      showMessageSnackbar(context, cardFlowsUnsupportedMessage);
      return;
    }
    if (_cardFlowOpen) return;
    _cardFlowOpen = true;
    try {
      if (infoSheet) {
        final confirmed = await showBackupConfirmationDialog(context, deviceId);
        if (confirmed != true || !context.mounted) return;
      }
      final walletCtx = WalletContext.of(context)!;
      final accessStructureRef = widget.accessStructure.accessStructureRef();
      final encryptionKey = await existingWalletKey(
        context: context,
        accessStructureRef: accessStructureRef,
        action: 'make this backup card',
      );
      if (encryptionKey == null || !context.mounted) return;
      await MaybeFullscreenDialog.show(
        context: context,
        child: walletCtx.wrap(
          VerifiableCardFlow(
            driver: CoordCardFlowDriver(
              deviceId: deviceId,
              accessStructureRef: accessStructureRef,
              encryptionKey: encryptionKey,
            ),
          ),
        ),
      );
    } finally {
      _cardFlowOpen = false;
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final walletCtx = WalletContext.of(context)!;
    final frostKey = walletCtx.wallet.frostKey()!;
    final accessStructure = frostKey.accessStructures().first;
    final backupStream = walletCtx.backupStream;
    final usesCard = coord.keyUsesCard(keyId: frostKey.keyId());

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

            final deviceInfoList = backupRun.devices;

            final allComplete = backupRun.isComplete;

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
                          usesCard
                              ? "Each key's backup is a 64-word card: words 1–32 from the device and words 33–64 from your phone."
                              : unverifiedChecklistLine,
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

                // Device checklist
                ...() {
                  // Group devices by share index
                  final devicesByShareIndex = <int, List<BackupDevice>>{};
                  for (final device in deviceInfoList) {
                    devicesByShareIndex
                        .putIfAbsent(device.shareIndex, () => [])
                        .add(device);
                  }

                  return devicesByShareIndex.entries.map((entry) {
                    final shareIndex = entry.key;
                    final devices = entry.value;
                    // A failed card offers no [Backup]: its row repeats the
                    // alarm's next step instead (owner B10, app review M1),
                    // so it's never a dead end.
                    final nextStep =
                        devices.first.cardStatus == CardBackupStatus.failed
                        ? devices.first.cardNextStep
                        : null;

                    return ListTile(
                      dense: true,
                      // Owner B12: a check after N has no Stop, here or in
                      // the flow.
                      subtitle: nextStep != null
                          ? Text(
                              nextStepLine(nextStep),
                              style: theme.textTheme.bodySmall?.copyWith(
                                color: theme.colorScheme.error,
                              ),
                            )
                          : null,
                      title: Row(
                        spacing: 4,
                        crossAxisAlignment: CrossAxisAlignment.center,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            "Key #$shareIndex",
                            style: theme.textTheme.bodyMedium?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant,
                              fontWeight: FontWeight.w500,
                            ),
                          ),
                          Flexible(
                            child: Text(
                              devices
                                  .map(
                                    (d) =>
                                        coord.getDeviceName(id: d.deviceId) ??
                                        '',
                                  )
                                  .join(', '),
                              style: theme.textTheme.bodyMedium?.copyWith(
                                fontWeight: FontWeight.w500,
                              ),
                              softWrap: true,
                            ),
                          ),
                        ],
                      ),
                      trailing: _BackupStatusTrailing(
                        isRecorded: devices.first.isRecorded,
                        shareIndex: shareIndex,
                        accessStructure: accessStructure,
                        cardStatus: devices.first.cardStatus,
                        createdHere: usesCard,
                      ),
                    );
                  }).toList();
                }(),

                const SizedBox(height: 16),

                // Device connection call-to-action
                StreamBuilder<DeviceListUpdate>(
                  stream: GlobalStreams.deviceListSubject,
                  builder: (context, deviceListSnapshot) {
                    final connectedDevices =
                        deviceListSnapshot.data?.state.devices ?? [];
                    final deviceCount = connectedDevices.length;

                    Widget statusContent;
                    IconData statusIcon;
                    Color? statusIconColor;

                    if (deviceCount > 1) {
                      statusIcon = Icons.warning_sharp;
                      statusIconColor = theme.colorScheme.primary;
                      statusContent = Text(
                        'Multiple devices connected. Connect only one device at a time.',
                      );
                    } else if (deviceCount == 1) {
                      final connectedDevice = connectedDevices.first;
                      final connectedDeviceId = connectedDevice.id;

                      final shareIndex = accessStructure
                          .getDeviceShortShareIndex(
                            deviceId: connectedDeviceId,
                          );

                      final deviceInfo = shareIndex != null
                          ? deviceInfoList.firstWhereOrNull(
                              (d) => d.shareIndex == shareIndex,
                            )
                          : null;

                      if (deviceInfo == null) {
                        statusIcon = Icons.info_rounded;
                        statusIconColor = null;
                        statusContent = Text(
                          'Unknown device connected. Please check your device.',
                        );
                      } else {
                        statusIcon = Icons.usb_rounded;
                        statusIconColor = theme.colorScheme.primary;
                        statusContent = Wrap(
                          spacing: 16,
                          runSpacing: 8,
                          crossAxisAlignment: WrapCrossAlignment.center,
                          children: [
                            Column(
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
                                  'Connected',
                                  style: theme.textTheme.bodySmall?.copyWith(
                                    color: theme.colorScheme.onSurfaceVariant,
                                  ),
                                ),
                              ],
                            ),
                            // Every new key is a card: [Backup]
                            // until it passes, then [Check], which repeats
                            // the quizzes (design §3.9). A failed card offers
                            // neither. Owner B15: on an install that didn't
                            // create the wallet, [Backup] writes the card
                            // unverified, if the device has one to write (its
                            // own answer); a card written here offers nothing
                            // more.
                            ...switch (deviceInfo.cardStatus) {
                              null when !usesCard => [
                                FilledButton(
                                  onPressed: () =>
                                      showCardFlow(context, connectedDeviceId),
                                  child: Text('Backup'),
                                ),
                              ],
                              CardBackupStatus.noCard ||
                              CardBackupStatus.notChecked => [
                                FilledButton(
                                  onPressed: () => showCardFlow(
                                    context,
                                    connectedDeviceId,
                                    infoSheet: true,
                                  ),
                                  child: Text('Backup'),
                                ),
                              ],
                              CardBackupStatus.checked => [
                                FilledButton(
                                  onPressed: () =>
                                      showCardFlow(context, connectedDeviceId),
                                  child: Text('Check'),
                                ),
                              ],
                              CardBackupStatus.failed ||
                              CardBackupStatus.unverified ||
                              null => [],
                            },
                          ],
                        );
                      }
                    } else {
                      statusIcon = Icons.usb_rounded;
                      statusIconColor = null;
                      statusContent = Text(
                        'Plug in a device to record or check a backup',
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
                      child: Card.filled(
                        margin: EdgeInsets.all(0.0),
                        color: theme.colorScheme.surfaceContainerHigh,
                        child: Padding(
                          padding: const EdgeInsets.all(16.0),
                          child: ConstrainedBox(
                            constraints: BoxConstraints(minHeight: 48),
                            child: Row(
                              crossAxisAlignment: CrossAxisAlignment.center,
                              children: [
                                Icon(statusIcon, color: statusIconColor),
                                const SizedBox(width: 12),
                                Expanded(child: statusContent),
                              ],
                            ),
                          ),
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

class _BackupStatusTrailing extends StatelessWidget {
  final bool isRecorded;
  final int shareIndex;
  final AccessStructure accessStructure;

  /// A card key's share, whose PASSED and FAILED come from the card check.
  /// Marking it backed up by hand only records it, never checks it. Null for a
  /// key with no card status (see [restoredFromCard]).
  final CardBackupStatus? cardStatus;

  /// Owner B15: this install created the wallet, so its cards are checked
  /// here. Otherwise a card written here is unverified.
  final bool createdHere;

  const _BackupStatusTrailing({
    required this.isRecorded,
    required this.shareIndex,
    required this.accessStructure,
    this.cardStatus,
    this.createdHere = true,
  });

  Widget _buildCardStatus(BuildContext context, CardBackupStatus status) {
    final theme = Theme.of(context);
    final (text, color, icon) = switch (status) {
      CardBackupStatus.noCard || CardBackupStatus.notChecked when isRecorded =>
        ('Recorded, not checked', cautionColor, Icons.pending_outlined),
      CardBackupStatus.noCard => (
        'Not backed up',
        theme.colorScheme.onSurfaceVariant,
        Icons.radio_button_unchecked,
      ),
      // Owner B15: an unverified card being written on this install.
      CardBackupStatus.notChecked when !createdHere => (
        'Not written yet: continue',
        cautionColor,
        Icons.pending_outlined,
      ),
      // NotChecked always has a pinned seal, so the flow can continue.
      CardBackupStatus.notChecked => (
        'Not checked yet: continue',
        cautionColor,
        Icons.pending_outlined,
      ),
      CardBackupStatus.checked => (
        'Backup checked',
        Colors.green.harmonizeWith(theme.colorScheme.primary),
        Icons.check_circle,
      ),
      CardBackupStatus.failed => (
        'Backup failed',
        theme.colorScheme.error,
        Icons.error_rounded,
      ),
      // Owner B15: written on this install, which didn't create the wallet.
      CardBackupStatus.unverified => (
        unverifiedRow,
        cautionColor,
        Icons.gpp_maybe_outlined,
      ),
    };
    final iconSlot = Padding(
      padding: const EdgeInsets.all(8),
      child: Icon(icon, size: 18, color: color),
    );
    // "Mark as backed up" records the halves, never PASSED (design §5.4).
    final markable =
        !isRecorded &&
        (status == CardBackupStatus.noCard ||
            status == CardBackupStatus.notChecked);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(text, style: theme.textTheme.bodyMedium?.copyWith(color: color)),
        if (markable) _markButton(context, iconSlot) else iconSlot,
      ],
    );
  }

  Widget _markButton(BuildContext context, Widget iconSlot) => Tooltip(
    message: 'Mark as backed up',
    child: InkWell(
      customBorder: const CircleBorder(),
      onTap: () async {
        final confirmed = await _confirmMarkBackupComplete(context, shareIndex);
        if (!confirmed) return;
        if (!context.mounted) return;
        await coord.markBackupComplete(
          accessStructureRef: accessStructure.accessStructureRef(),
          shareIndex: shareIndex,
        );
      },
      child: iconSlot,
    ),
  );

  @override
  Widget build(BuildContext context) {
    final cardStatus = this.cardStatus;
    if (cardStatus != null) return _buildCardStatus(context, cardStatus);

    final theme = Theme.of(context);
    // An install that didn't create the wallet, with no card written here
    // (owner B15): a key restored from its cards has nothing to do, and a
    // device with a card to write offers it when connected.
    return Text(
      'No card written here',
      style: theme.textTheme.bodyMedium?.copyWith(
        color: theme.colorScheme.onSurfaceVariant,
      ),
    );
  }
}

Future<bool> _confirmMarkBackupComplete(
  BuildContext context,
  int shareIndex,
) async {
  final result = await showDialog<bool>(
    context: context,
    builder: (dialogContext) => AlertDialog(
      title: const Text('Mark as backed up?'),
      content: Text(
        'Only do this if you have already recorded the backup for Key #$shareIndex somewhere safe.\n\nIf you mark a key as backed up without actually having the backup, you risk permanently losing access to this wallet.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(dialogContext).pop(false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.of(dialogContext).pop(true),
          child: const Text('I have the backup'),
        ),
      ],
    ),
  );
  return result == true;
}
