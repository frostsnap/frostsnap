import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
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
    final deviceIds = accessStructure.devices();
    final devices = deviceIds
        .map((id) => (id: id, name: coord.getDeviceName(id: id) ?? "??"))
        .toList();
    final deviceList = coord.deviceListState();
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
              ...devices.map(
                (device) {
                  final connectedDevice = deviceList.devices.firstWhere(
                    (d) => d.id == device.id,
                    orElse: () => null as dynamic,
                  );
                  final caseColor = connectedDevice is ConnectedDevice
                      ? connectedDevice.caseColor
                      : null;
                  final Color bgColor = caseColor?.toColor() ??
                      theme.colorScheme.surfaceContainer;
                  final Color textColor = caseColor?.onColor(context) ??
                      theme.colorScheme.onSurface;

                  return Chip(
                    label: Text(
                      device.name,
                      style: theme.textTheme.titleMedium?.copyWith(
                        color: textColor,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    backgroundColor: bgColor,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(8),
                      side: caseColor != null
                          ? BorderSide(
                              color: textColor.withValues(alpha: 0.2),
                              width: 1.5,
                            )
                          : BorderSide.none,
                    ),
                  );
                },
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
