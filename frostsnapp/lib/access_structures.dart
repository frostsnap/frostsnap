import 'package:flutter/material.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/restoration.dart';

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
    final devices =
        accessStructure
            .devices()
            .map((id) => coord.getDeviceName(id: id) ?? "??")
            .toList();
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
                (device) => Chip(
                  label: Text(
                    device,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  backgroundColor: theme.colorScheme.surfaceContainer,
                ),
              ),
              IconButton.filledTonal(
                onPressed: () async {
                  await startAddKeyToAcessStructureDialog(
                    context,
                    accessStructure.accessStructureRef(),
                  );
                },
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

Future<RestorationId?> startAddKeyToAcessStructureDialog(
  BuildContext context,
  AccessStructureRef accessStructureRef,
) async {
  final restorationId = await showDialog(
    context: context,
    builder: (context) => WalletRecoveryFlow(existing: accessStructureRef),
  );
  coord.cancelProtocol();
  return restorationId;
}
