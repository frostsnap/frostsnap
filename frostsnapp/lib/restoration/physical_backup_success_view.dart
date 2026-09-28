import 'package:flutter/material.dart';

class PhysicalBackupSuccessView extends StatelessWidget {
  final String deviceName;

  const PhysicalBackupSuccessView({super.key, required this.deviceName});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.check_circle, size: 64, color: Colors.green),
        const SizedBox(height: 24),
        Text(
          'Physical backup restored successfully on to $deviceName!',
          style: theme.textTheme.headlineSmall,
          textAlign: TextAlign.center,
        ),
      ],
    );
  }
}
