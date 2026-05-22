import 'package:flutter/material.dart';
import 'package:frostsnap/restoration/state.dart';

class StartRestorationInfoView extends StatelessWidget {
  final RecoveryContext recoveryContext;

  const StartRestorationInfoView({super.key, required this.recoveryContext});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final extra = switch (recoveryContext) {
      NewRestorationContext() =>
        ' Since this is the first key to be restored you will need to '
            'manually provide the wallet details.',
      _ => '',
    };
    return Column(
      children: [
        Icon(
          Icons.check_circle_outline,
          size: 64,
          color: theme.colorScheme.primary,
        ),
        const SizedBox(height: 24),
        Text(
          "You've plugged in a blank device to restore a physical backup onto.$extra",
          textAlign: TextAlign.center,
        ),
      ],
    );
  }
}
