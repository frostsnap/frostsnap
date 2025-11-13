import 'package:flutter/material.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/dialog_content_with_actions.dart';
import 'package:frostsnap/restoration/state.dart';

class StartRestorationInfoView extends StatelessWidget with TitledWidget {
  final VoidCallback onContinue;
  final RecoveryContext recoveryContext;

  const StartRestorationInfoView({
    super.key,
    required this.recoveryContext,
    required this.onContinue,
  });

  @override
  String get titleText => 'Found blank device for backup entry';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    var text =
        'You\'ve plugged in a blank device to restore a physical backup onto.';
    var button;
    switch (recoveryContext) {
      case NewRestorationContext():
        text +=
            ' Since this is the first key to be restored you will need to manually provide the wallet details.';
        button = "Begin restoration";
        break;
      case ContinuingRestorationContext():
        button = "Next";
        break;
      case AddingToWalletContext():
        button = "Next";
        break;
    }
    return DialogContentWithActions(
      key: const ValueKey('startRestorationInfo'),
      content: Column(
        children: [
          Icon(
            Icons.check_circle_outline,
            size: 64,
            color: theme.colorScheme.primary,
          ),
          const SizedBox(height: 24),
          Text(text, textAlign: TextAlign.center),
        ],
      ),
      actions: [FilledButton(onPressed: onContinue, child: Text(button))],
    );
  }
}
