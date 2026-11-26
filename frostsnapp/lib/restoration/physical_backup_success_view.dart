import 'package:flutter/material.dart';
import 'package:frostsnap/device_identity_art.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/dialog_content_with_actions.dart';
import 'package:frostsnap/src/rust/api.dart';

class PhysicalBackupSuccessView extends StatelessWidget with TitledWidget {
  final VoidCallback onClose;
  final DeviceId deviceId;
  final String deviceName;

  const PhysicalBackupSuccessView({
    super.key,
    required this.onClose,
    required this.deviceId,
    required this.deviceName,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return DialogContentWithActions(
      key: const ValueKey('physicalBackupSuccess'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.check_circle, size: 64, color: Colors.green),
          const SizedBox(height: 24),
          DeviceNameWithArt.forId(
            deviceId,
            name: Text(deviceName, style: theme.textTheme.titleMedium),
          ),
          const SizedBox(height: 12),
          Text(
            'Physical backup restored successfully on to $deviceName!',
            style: theme.textTheme.headlineSmall,
            textAlign: TextAlign.center,
          ),
        ],
      ),
      actions: [
        FilledButton.icon(
          icon: const Icon(Icons.arrow_forward),
          label: const Text('Close'),
          onPressed: onClose,
        ),
      ],
    );
  }

  @override
  String get titleText => '';
}
