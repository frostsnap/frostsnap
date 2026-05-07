import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';

class CandidateReadyView extends StatelessWidget {
  final RecoverShare candidate;
  final bool addingToExisting;

  const CandidateReadyView({
    super.key,
    required this.candidate,
    required this.addingToExisting,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final deviceName = coord.getDeviceName(id: candidate.heldBy) ?? '<empty>';
    final message = addingToExisting
        ? "Key '$deviceName' is ready to be added to wallet '${candidate.heldShare.keyName}'."
        : "Key '$deviceName' is part of a wallet called '${candidate.heldShare.keyName}'.";

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.check_circle, size: 64, color: theme.colorScheme.primary),
        const SizedBox(height: 24),
        Text(
          'Key ready',
          style: theme.textTheme.headlineSmall,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 16),
        Text(message, textAlign: TextAlign.center),
      ],
    );
  }
}
