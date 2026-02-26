import 'package:flutter/material.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/wallet_add.dart';

class AccessStructureListWidget extends StatelessWidget {
  final List<AccessStructure> accessStructures;

  const AccessStructureListWidget({super.key, required this.accessStructures});

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
      shrinkWrap: true,
      itemCount: accessStructures.length,
      itemBuilder: (context, i) {
        final accessStructure = accessStructures[i];
        return Center(
          child: AccessStructureWidget(accessStructure: accessStructure),
        );
      },
    );
  }
}

class AccessStructureWidget extends StatelessWidget {
  final AccessStructure accessStructure;

  const AccessStructureWidget({super.key, required this.accessStructure});

  @override
  Widget build(BuildContext context) {
    final deviceIds = accessStructure.devicesByShareIndex();
    final threshold = accessStructure.threshold();
    final theme = Theme.of(context);
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Container(
          margin: const EdgeInsets.only(top: 20),
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            border: Border.all(color: theme.colorScheme.secondary),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Wrap(
            spacing: 8,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              ...deviceIds.map(
                (deviceId) => _DeviceChip(
                  deviceId: deviceId,
                  accessStructure: accessStructure,
                ),
              ),
              IconButton.filledTonal(
                onPressed: () => WalletAddColumn.showAddKeyDialog(
                  context,
                  accessStructure.accessStructureRef(),
                ),
                icon: const Icon(Icons.add),
              ),
            ],
          ),
        ),
        Positioned(
          top: 4,
          left: 16,
          child: Container(
            color: theme.scaffoldBackgroundColor,
            padding: const EdgeInsets.symmetric(horizontal: 4),
            child: Text(
              "any $threshold of",
              style: Theme.of(context).textTheme.titleLarge,
            ),
          ),
        ),
      ],
    );
  }
}

class _DeviceChip extends StatelessWidget {
  final DeviceId deviceId;
  final AccessStructure accessStructure;

  const _DeviceChip({required this.deviceId, required this.accessStructure});

  @override
  Widget build(BuildContext context) {
    final deviceName = coord.getDeviceName(id: deviceId) ?? "??";
    final shareIndex = accessStructure.getDeviceShortShareIndex(
      deviceId: deviceId,
    );
    final theme = Theme.of(context);

    return Chip(
      label: DeviceWithShareIndex(
        shareIndex: shareIndex,
        deviceName: deviceName,
      ),
      backgroundColor: theme.colorScheme.surfaceContainer,
      deleteIcon: const Icon(Icons.close, size: 18),
      onDeleted: () => _showDeleteDialog(context, deviceName),
    );
  }

  void _showDeleteDialog(BuildContext context, String deviceName) {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Remove Device?'),
        content: Text(
          'Remove "$deviceName" from this access structure?\n\n'
          'The device will keep its key share, but you will have to add it '
          'back to the wallet to use it again.',
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
