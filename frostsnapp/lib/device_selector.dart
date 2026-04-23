import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';

class DeviceItem {
  final DeviceId id;
  final String name;
  final int shareIndex;
  final bool enabled;
  final String? disabledReason;

  DeviceItem({
    required this.id,
    required this.name,
    required this.shareIndex,
    this.enabled = true,
    this.disabledReason,
  });

  static List<DeviceItem> fromAccessStructure(AccessStructure accessStruct) {
    return accessStruct.devices().map((id) {
      final name = coord.getDeviceName(id: id) ?? '<unknown>';
      final shareIndex = accessStruct.getDeviceShortShareIndex(deviceId: id) ?? 0;
      final nonces = coord.noncesAvailable(id: id);
      return DeviceItem(
        id: id,
        name: name,
        shareIndex: shareIndex,
        enabled: nonces > 0,
        disabledReason: nonces == 0 ? 'no nonces remaining' : null,
      );
    }).toList();
  }
}

class DeviceSelectorList extends StatelessWidget {
  final String title;
  final String? trailing;
  final List<DeviceItem> devices;
  final Set<DeviceId> selected;
  final ValueChanged<DeviceId> onToggle;

  const DeviceSelectorList({
    super.key,
    required this.title,
    this.trailing,
    required this.devices,
    required this.selected,
    required this.onToggle,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ListTile(
          dense: true,
          title: Text(title),
          trailing: trailing != null ? Text(trailing!) : null,
        ),
        ...devices.map((device) {
          final isSelected = selected.contains(device.id);
          return CheckboxListTile(
            value: isSelected,
            onChanged: device.enabled
                ? (_) => onToggle(device.id)
                : null,
            secondary: Icon(Icons.key),
            title: Text('#${device.shareIndex} ${device.name}'),
            subtitle: device.disabledReason != null
                ? Text(
                    device.disabledReason!,
                    style: TextStyle(color: theme.colorScheme.error),
                  )
                : null,
          );
        }),
      ],
    );
  }
}
