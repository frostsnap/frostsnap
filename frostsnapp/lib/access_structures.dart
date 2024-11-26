import 'package:flutter/material.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/theme.dart';

class AccessStructureListWidget extends StatelessWidget {
  final List<AccessStructureState> accessStructures;

  const AccessStructureListWidget({super.key, required this.accessStructures});

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
        shrinkWrap: true,
        itemCount: accessStructures.length,
        itemBuilder: (context, i) {
          final accessStructure = accessStructures[i];
          final widget = switch (accessStructure) {
            AccessStructureState_Recovering(:final field0) =>
              AccessStructureWidget(
                  devices: field0.gotSharesFrom
                      .map((device) => coord.getDeviceName(id: device) ?? "??")
                      .toList(),
                  threshold: field0.threshold),
            AccessStructureState_Complete(:final field0) =>
              AccessStructureWidget.fromAccessStructure(field0)
          };
          return Center(child: widget);
        });
  }
}

class AccessStructureWidget extends StatelessWidget {
  final List<String> devices;
  final int threshold;

  const AccessStructureWidget(
      {super.key, required this.devices, required this.threshold});

  static AccessStructureWidget fromAccessStructure(
      AccessStructure accessStructure) {
    return AccessStructureWidget(
      devices: accessStructure
          .devices()
          .map((id) => coord.getDeviceName(id: id) ?? "??")
          .toList(),
      threshold: accessStructure.threshold(),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      clipBehavior: Clip.none,
      children: [
        // Rectangle with border and chips inside
        Container(
          margin: const EdgeInsets.only(top: 20), // Space for the label
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
            border: Border.all(color: backgroundSecondaryColor),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Wrap(
            spacing: 8,
            runSpacing: 4,
            children: devices.map((device) {
              return Chip(
                label: Text(
                  device,
                  style: Theme.of(context).textTheme.bodySmall,
                ),
                backgroundColor: backgroundSecondaryColor,
              );
            }).toList(),
          ),
        ),

        // Text label that "breaks" the border
        Positioned(
          top: 10,
          left: 16,
          child: Container(
            color: backgroundPrimaryColor, // Match background
            padding: const EdgeInsets.symmetric(horizontal: 4),
            child: Text(
              threshold == devices.length
                  ? "all $threshold of"
                  : "any $threshold of",
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ),
        ),
      ],
    );
  }
}
