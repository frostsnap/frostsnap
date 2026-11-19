import 'package:flutter/material.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/theme.dart';

const _notCheckedText =
    "This wallet's backup isn't checked yet. Don't put money in it "
    "until your phone says 'Backup checked'.";

/// Whether the soft receive lock holds for [keyId]: a card key shows no
/// address and exports no descriptor until every card is checked (spec
/// L1.6.5).
bool cardLockHolds(KeyId keyId) =>
    coord.keyUsesCard(keyId: keyId) &&
    !coord.getBackupRun(keyId: keyId).isComplete;

/// For a page that would show an address or the descriptor: shows the lock's
/// dialog and returns true while the lock holds, so the caller shows nothing.
Future<bool> showCardLockIfHeld(
  BuildContext context,
  WalletContext walletCtx,
) async {
  if (!cardLockHolds(walletCtx.wallet.keyId())) return false;
  final frostKey = walletCtx.wallet.frostKey();
  if (frostKey != null) {
    await showCardNotCheckedDialog(context, frostKey.accessStructures()[0]);
  }
  return true;
}

/// The app's soft receive lock for a card key whose backup isn't checked
/// (design §1.1 item 8). Unlike `showSecureWalletDialog` there is no "Later":
/// both buttons leave without showing an address.
Future<void> showCardNotCheckedDialog(
  BuildContext context,
  AccessStructure accessStructure,
) async {
  final openChecklist = await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (dialogContext) => const CardNotCheckedDialog(),
  );
  if (openChecklist != true || !context.mounted) return;

  final superCtx = SuperWalletContext.of(context)!;
  await MaybeFullscreenDialog.show(
    context: context,
    child: superCtx.tryWrapInWalletContext(
      keyId: accessStructure.masterAppkey().keyId(),
      child: BackupChecklist(accessStructure: accessStructure),
    ),
  );
}

/// Pops `true` for [Open backup checklist] and `false` for [Cancel].
class CardNotCheckedDialog extends StatelessWidget {
  const CardNotCheckedDialog({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return BackdropFilter(
      filter: blurFilter,
      child: AlertDialog(
        constraints: dialogConstraints,
        icon: Icon(Icons.warning_rounded, color: cautionColor),
        title: const Text('Finish your backup card first'),
        content: Text(_notCheckedText, style: theme.textTheme.bodyMedium),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Open backup checklist'),
          ),
        ],
      ),
    );
  }
}

/// The lock in place of a settings page that would show the descriptor.
class CardNotCheckedNotice extends StatelessWidget {
  const CardNotCheckedNotice({super.key});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        spacing: 8,
        children: [
          Text(
            'Finish your backup card first',
            style: theme.textTheme.titleMedium,
          ),
          Text(_notCheckedText, style: theme.textTheme.bodyMedium),
        ],
      ),
    );
  }
}
