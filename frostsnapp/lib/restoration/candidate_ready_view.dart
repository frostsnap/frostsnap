import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';
import 'package:frostsnap/restoration/material_dialog_card.dart';
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
      buttonText = 'Start restoring';
    }

    return MaterialDialogCard(
      key: const ValueKey('candidateReady'),
      iconData: Icons.check_circle,
      title: Text(title),
      content: Text(message, textAlign: TextAlign.center),
      actions: [FilledButton(child: Text(buttonText), onPressed: onConfirm)],
    );
  }
}
