import 'package:flutter/material.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_identity_art.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/wallet_add.dart';

extension AccessStructureKeys on AccessStructure {
  /// How many distinct keys the wallet has. Devices holding a copy of the same key count once, since
  /// only distinct keys add to what can spend or be lost.
  int get keyCount => devices()
      .map((id) => getDeviceShortShareIndex(deviceId: id))
      .toSet()
      .length;
}

class AccessStructureListWidget extends StatelessWidget {
  final List<AccessStructure> accessStructures;
  final String walletName;

  const AccessStructureListWidget({
    super.key,
    required this.accessStructures,
    required this.walletName,
  });

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    spacing: 24,
    children: [
      for (final accessStructure in accessStructures)
        AccessStructureWidget(
          accessStructure: accessStructure,
          walletName: walletName,
        ),
    ],
  );
}

class AccessStructureWidget extends StatelessWidget {
  final AccessStructure accessStructure;
  final String walletName;

  const AccessStructureWidget({
    super.key,
    required this.accessStructure,
    required this.walletName,
  });

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final deviceIds = accessStructure.devicesByShareIndex();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KeySetSummary(
          threshold: accessStructure.threshold(),
          shareIndices: [
            for (final id in deviceIds)
              accessStructure.getDeviceShortShareIndex(deviceId: id),
          ],
          walletName: walletName,
        ),
        const SizedBox(height: 20),
        Card.filled(
          margin: EdgeInsets.zero,
          color: cs.surfaceContainerHigh,
          clipBehavior: Clip.antiAlias,
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(20)),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.all(8),
                child: DeviceTileGrid(
                  spacing: 8,
                  tiles: [
                    for (final deviceId in deviceIds)
                      _KeyTile(
                        deviceId: deviceId,
                        accessStructure: accessStructure,
                      ),
                  ],
                ),
              ),
              Divider(height: 1, color: cs.outlineVariant),
              ListTile(
                leading: CircleAvatar(
                  backgroundColor: cs.secondaryContainer,
                  foregroundColor: cs.onSecondaryContainer,
                  child: const Icon(Icons.add),
                ),
                title: const Text('Add a key'),
                subtitle: const Text(
                  'From a backup, or a device that holds one',
                ),
                onTap: () => WalletAddColumn.showAddKeyDialog(
                  context,
                  accessStructure.accessStructureRef(),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// What a wallet's keys allow: how many of them spend, and how many can be lost. [shareIndices] has
/// one entry per device, so devices holding a copy of the same key count once.
class KeySetSummary extends StatelessWidget {
  final int threshold;
  final List<int?> shareIndices;
  final String walletName;

  const KeySetSummary({
    super.key,
    required this.threshold,
    required this.shareIndices,
    required this.walletName,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final cs = theme.colorScheme;
    final total = shareIndices.toSet().length;
    final spare = total - threshold;
    final hasCopies = total < shareIndices.length;
    final emphasis = TextStyle(fontWeight: FontWeight.w500, color: cs.primary);
    final legendStyle = theme.textTheme.bodySmall?.copyWith(
      color: cs.onSurfaceVariant,
    );
    final noteStyle = theme.textTheme.bodyMedium?.copyWith(
      color: cs.onSurfaceVariant,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text.rich(
          TextSpan(
            children: [
              const TextSpan(text: 'Any '),
              TextSpan(text: '$threshold', style: emphasis),
              const TextSpan(text: ' of these '),
              TextSpan(text: '$total', style: emphasis),
              const TextSpan(text: ' keys can spend'),
            ],
          ),
          style: theme.textTheme.headlineMedium,
        ),
        const SizedBox(height: 12),
        ExcludeSemantics(
          child: Row(
            spacing: 6,
            children: [
              for (var i = 0; i < total; i++)
                Expanded(
                  child: Container(
                    height: 6,
                    decoration: BoxDecoration(
                      color: i < threshold
                          ? cs.primary
                          : cs.surfaceContainerHighest,
                      borderRadius: BorderRadius.circular(3),
                    ),
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 6),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text('$threshold needed to spend', style: legendStyle),
            Text('$spare spare', style: legendStyle),
          ],
        ),
        const SizedBox(height: 12),
        Text(switch (spare) {
          <= 0 => '$walletName needs every one of these keys to spend.',
          1 => '$walletName can still be spent if any one key is lost.',
          _ => '$walletName can still be spent if any $spare keys are lost.',
        }, style: noteStyle),
        if (hasCopies) ...[
          const SizedBox(height: 4),
          Text(
            'Devices showing the same number hold copies of one key.',
            style: noteStyle,
          ),
        ],
      ],
    );
  }
}

/// How many of the wallet's keys have a recorded backup, with a way to make more.
class BackupStatusCard extends StatelessWidget {
  final AccessStructure accessStructure;

  const BackupStatusCard({super.key, required this.accessStructure});

  @override
  Widget build(BuildContext context) {
    final walletCtx = WalletContext.of(context);
    if (walletCtx == null) return const SizedBox.shrink();
    final theme = Theme.of(context);
    final cs = theme.colorScheme;
    return StreamBuilder(
      stream: walletCtx.backupStream,
      builder: (context, snapshot) {
        final run = snapshot.data;
        if (run == null) return const SizedBox.shrink();
        // Devices that share a key back it up once, so count keys, not devices.
        final keys = run.devices.map((d) => d.shareIndex).toSet();
        final backedUp = run.devices
            .where((d) => d.isBackedUp)
            .map((d) => d.shareIndex)
            .toSet();
        return Card.filled(
          margin: EdgeInsets.zero,
          color: cs.surfaceContainerHigh,
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(20)),
          ),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 12, 8, 12),
            child: Row(
              spacing: 12,
              children: [
                Icon(Icons.shield, color: cs.primary),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '${backedUp.length} of ${keys.length} keys backed up',
                        style: theme.textTheme.titleMedium,
                      ),
                      Text(
                        'A backup restores its key if the device is lost.',
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: cs.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
                TextButton(
                  onPressed: () => MaybeFullscreenDialog.show(
                    context: context,
                    child: walletCtx.wrap(
                      BackupChecklist(
                        accessStructure: accessStructure,
                        showAppBar: true,
                      ),
                    ),
                  ),
                  child: const Text('Back up'),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _KeyTile extends StatelessWidget {
  final DeviceId deviceId;
  final AccessStructure accessStructure;

  const _KeyTile({required this.deviceId, required this.accessStructure});

  @override
  Widget build(BuildContext context) {
    final deviceName = coord.getDeviceName(id: deviceId) ?? "??";
    final cs = Theme.of(context).colorScheme;

    return MenuAnchor(
      style: const MenuStyle(alignment: AlignmentDirectional.topEnd),
      menuChildren: [
        MenuItemButton(
          leadingIcon: const Icon(Icons.remove_circle_outline),
          onPressed: () => _showDeleteDialog(context, deviceName),
          child: const Text('Remove from wallet'),
        ),
      ],
      builder: (context, controller, _) => DeviceTile(
        id: deviceId,
        name: deviceName,
        shareIndex: accessStructure.getDeviceShortShareIndex(
          deviceId: deviceId,
        ),
        color: cs.surfaceContainerHighest,
        corner: Icon(Icons.more_vert, size: 18, color: cs.outline),
        cornerTooltip: 'Key options',
        onCornerTap: () =>
            controller.isOpen ? controller.close() : controller.open(),
      ),
    );
  }

  void _showDeleteDialog(BuildContext context, String deviceName) {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Remove Device?'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          spacing: 16,
          children: [
            DeviceNameWithArt.forId(deviceId, name: Text(deviceName)),
            Text(
              'Remove "$deviceName" from this access structure?\n\n'
              'The device will keep its key share, but you will have to add it '
              'back to the wallet to use it again.',
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () async {
              Navigator.pop(context);
              try {
                await coord.deleteShare(
                  accessStructureRef: accessStructure.accessStructureRef(),
                  deviceId: deviceId,
                );
              } catch (e) {
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(
                      content: Text('Failed to remove device: $e'),
                      backgroundColor: Theme.of(context).colorScheme.error,
                    ),
                  );
                }
              }
            },
            style: TextButton.styleFrom(
              foregroundColor: Theme.of(context).colorScheme.error,
            ),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
  }
}

class AccessStructureSummary extends StatelessWidget {
  final int t;
  final int n;

  const AccessStructureSummary({super.key, required this.t, this.n = 0});

  @override
  Widget build(BuildContext context) {
    final nText = n < t ? "?" : n.toString();
    return Text("$t-of-$nText", style: Theme.of(context).textTheme.titleSmall!);
  }
}
