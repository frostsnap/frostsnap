import 'package:flutter/material.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/genuine_badge.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';

class DeviceItem {
  final DeviceId id;
  final String name;
  final int? shareIndex;
  final bool hasNonces;

  const DeviceItem({
    required this.id,
    required this.name,
    this.shareIndex,
    this.hasNonces = true,
  });

  String get title => shareIndex == null ? name : '#$shareIndex $name';

  static List<DeviceItem> fromAccessStructure(AccessStructure accessStruct) {
    return accessStruct
        .devices()
        .sortByKeyShareIndex(accessStruct, (id) => id)
        .map(
          (id) => DeviceItem(
            id: id,
            name: coord.getDeviceName(id: id) ?? '<unknown>',
            shareIndex: accessStruct.getDeviceShortShareIndex(deviceId: id),
            hasNonces: coord.noncesAvailable(id: id) > 0,
          ),
        )
        .toList();
  }
}

class DeviceSelectorList extends StatelessWidget {
  final String? title;
  final String? trailing;
  final List<DeviceItem> devices;
  final Set<DeviceId> selected;
  final bool canSelectMore;
  final void Function(DeviceId id, bool selected) onChanged;

  /// Overridable only because widget tests have no coordinator to follow genuine status through.
  final Widget Function(DeviceId id) badge;

  const DeviceSelectorList({
    super.key,
    this.title,
    this.trailing,
    required this.devices,
    required this.selected,
    this.canSelectMore = true,
    required this.onChanged,
    this.badge = GenuineBadge.forDeviceId,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final title = this.title;
    final trailing = this.trailing;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (title != null)
          ListTile(
            dense: true,
            title: Text(title),
            trailing: trailing != null ? Text(trailing) : null,
          ),
        ...devices.map((device) {
          final isSelected = selected.contains(device.id);
          final enabled = device.hasNonces && (canSelectMore || isSelected);
          return CheckboxListTile(
            value: isSelected,
            onChanged: enabled
                ? (checked) => onChanged(device.id, checked ?? false)
                : null,
            secondary: CaseAccent(
              id: device.id,
              builder: (context, accent) => Icon(Icons.key, color: accent),
            ),
            title: Row(
              spacing: 8,
              children: [
                Expanded(
                  child: Text(device.title, overflow: TextOverflow.ellipsis),
                ),
                badge(device.id),
              ],
            ),
            subtitle: device.hasNonces
                ? null
                : Text(
                    'no nonces remaining or too many signing sessions',
                    style: TextStyle(color: theme.colorScheme.error),
                  ),
          );
        }),
      ],
    );
  }
}
