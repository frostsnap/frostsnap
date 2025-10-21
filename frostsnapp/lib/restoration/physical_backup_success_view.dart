import 'package:flutter/material.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';

class PhysicalBackupSuccessView extends StatelessWidget with TitledWidget {
  final VoidCallback onClose;
  final String deviceName;

  const PhysicalBackupSuccessView({
    super.key,
    required this.onClose,
    required this.deviceName,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      key: const ValueKey('physicalBackupSuccess'),
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.check_circle, size: 48, color: Colors.green),
        const SizedBox(height: 16),
        Text(
          'Physical backup restored successfully on to $deviceName!',
          style: theme.textTheme.headlineMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 24),
        ElevatedButton.icon(
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
