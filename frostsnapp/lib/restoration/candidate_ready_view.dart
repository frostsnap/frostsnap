import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/dialog_content_with_actions.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';

class CandidateReadyView extends StatelessWidget with TitledWidget {
  final RecoverShare candidate;
  final RestorationId? continuing;
  final AccessStructureRef? existing;
  final VoidCallback onConfirm;

  const CandidateReadyView({
    super.key,
    required this.candidate,
    this.continuing,
    this.existing,
    required this.onConfirm,
  });

  @override
  String get titleText => 'Restore with existing key';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final deviceName = coord.getDeviceName(id: candidate.heldBy) ?? '<empty>';

    String title;
    String message;
    String buttonText;

    title = 'Key ready';

    if (continuing != null || existing != null) {
      message =
          "Key '$deviceName' is ready to be added to wallet '${candidate.heldShare.keyName}'.";
      buttonText = 'Add to wallet';
    } else {
      message =
          "Key '$deviceName' is part of a wallet called '${candidate.heldShare.keyName}'.";
      buttonText = 'Restore';
    }

    return DialogContentWithActions(
      key: const ValueKey('candidateReady'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.check_circle, size: 64, color: theme.colorScheme.primary),
          const SizedBox(height: 24),
          Text(
            title,
            style: theme.textTheme.headlineSmall,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 16),
          Text(message, textAlign: TextAlign.center),
        ],
      ),
      actions: [FilledButton(onPressed: onConfirm, child: Text(buttonText))],
    );
  }
}
