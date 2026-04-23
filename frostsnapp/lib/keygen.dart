import 'package:flutter/material.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/theme.dart';

enum SecureWalletChoice {
  /// User chose "Later" — caller should proceed with whatever it was going
  /// to do (e.g. opening the receive sheet).
  later,

  /// User chose "Secure Wallet" — the backup checklist has been opened and
  /// the caller should not proceed.
  secure,

  /// User backed out (system back button). Caller should do nothing.
  cancelled,
}

/// Shows a reminder that backups are incomplete and offers to open the
/// backup checklist.
Future<SecureWalletChoice> showSecureWalletDialog(
  BuildContext context,
  AccessStructure accessStructure,
) async {
  final choice = await showDialog<SecureWalletChoice>(
    context: context,
    barrierDismissible: false,
    builder: (BuildContext dialogContext) {
      return BackdropFilter(
        filter: blurFilter,
        child: AlertDialog(
          title: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: const [
              Text(
                'Secure your wallet',
                style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
              ),
              Icon(Icons.checklist, size: 40),
            ],
          ),
          content: const Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Before receiving any Bitcoin, you should backup and distribute your Frostsnap devices.',
              ),
              SizedBox(height: 16),
              Text(
                'With each device you should:',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.directions_walk),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Travel to the secure location where you will store it.',
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.edit),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Record the backup on the provided backup card (~5 mins).',
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.lock),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Safely store your Frostsnap device and its backup.',
                    ),
                  ),
                ],
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () =>
                  Navigator.of(dialogContext).pop(SecureWalletChoice.later),
              child: const Text('Later'),
            ),
            FilledButton(
              onPressed: () =>
                  Navigator.of(dialogContext).pop(SecureWalletChoice.secure),
              child: const Text('Secure Wallet'),
            ),
          ],
        ),
      );
    },
  );

  if (choice != SecureWalletChoice.secure) {
    return choice ?? SecureWalletChoice.cancelled;
  }

  if (!context.mounted) return SecureWalletChoice.cancelled;
  final superCtx = SuperWalletContext.of(context)!;
  await MaybeFullscreenDialog.show(
    context: context,
    child: superCtx.tryWrapInWalletContext(
      keyId: accessStructure.masterAppkey().keyId(),
      child: BackupChecklist(accessStructure: accessStructure),
    ),
  );
  return SecureWalletChoice.secure;
}
