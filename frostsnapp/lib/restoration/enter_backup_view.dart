import 'package:flutter/material.dart';

/// Painted under the `FullscreenActionDialogController` overlay so the
/// brief moment when the overlay dismisses doesn't show a blank body.
class EnterBackupView extends StatelessWidget {
  final String? deviceName;

  const EnterBackupView({super.key, this.deviceName});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final onWho = deviceName == null ? 'your device' : deviceName!;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(
          Icons.keyboard_alt_rounded,
          size: 64,
          color: theme.colorScheme.primary,
        ),
        const SizedBox(height: 24),
        Text(
          'Enter your seed words',
          style: theme.textTheme.headlineSmall,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 16),
        Text(
          'Use $onWho to enter the key number and 25 seed words from your '
          'physical backup. The app will continue automatically once you '
          "finish.",
          textAlign: TextAlign.center,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
      ],
    );
  }
}
