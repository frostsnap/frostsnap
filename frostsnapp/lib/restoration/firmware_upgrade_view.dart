import 'package:flutter/material.dart';

class FirmwareUpgradeView extends StatelessWidget {
  const FirmwareUpgradeView({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(
          Icons.system_update_alt_rounded,
          size: 64,
          color: theme.colorScheme.primary,
        ),
        const SizedBox(height: 24),
        Text(
          'Firmware Update Required',
          style: theme.textTheme.headlineSmall,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 16),
        const Text(
          'This device needs a firmware update before it can be used for '
          'wallet restoration.',
          textAlign: TextAlign.center,
        ),
      ],
    );
  }
}
