import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api.dart';

/// Get the app's encryption key for an operation that must DECRYPT the existing
/// wallet at [accessStructureRef], after verifying it actually can.
///
/// Returns null — and shows the delete-and-recover dialog — when the key is
/// unavailable (no screen lock, so it can't be produced) or can't decrypt this
/// wallet (wrong key). A compact, deliberately brittle guard: the
/// canDecryptWallet probe is a pre-check that can drift from the operation, but
/// the wallet-locking migration removes the whole per-key encryption concern,
/// so it isn't worth threading typed errors through the stack for.
Future<SymmetricKey?> existingWalletKey({
  BuildContext? context,
  required AccessStructureRef accessStructureRef,
  required String action,
  @visibleForTesting Future<SymmetricKey> Function()? getKey,
  @visibleForTesting bool Function(SymmetricKey key)? canDecrypt,
  @visibleForTesting Future<void> Function()? showRecovery,
}) async {
  getKey ??= SecureKeyProvider.getExistingEncryptionKey;
  canDecrypt ??= (key) => coord.canDecryptWallet(
    accessStructureRef: accessStructureRef,
    encryptionKey: key,
  );
  showRecovery ??= () =>
      showWalletKeyMismatchDialog(context: context, action: action);

  // The empty-key fallback (see AndroidSecureKeyProvider) isn't persisted, so
  // a wallet established under it must keep working: probe the empty key
  // before giving up.
  SymmetricKey? emptyKeyIfDecrypts() {
    final emptyKey = SecureKeyProvider.emptyKey;
    return canDecrypt!(emptyKey) ? emptyKey : null;
  }

  final SymmetricKey key;
  try {
    key = await getKey();
  } on WalletKeyUnavailable {
    final emptyKey = emptyKeyIfDecrypts();
    if (emptyKey != null) return emptyKey;
    // Await so a null return means the dialog has been shown AND dismissed —
    // callers that pop their own flow afterwards then can't race it away.
    await showRecovery();
    return null;
  }
  if (!canDecrypt(key)) {
    final emptyKey = emptyKeyIfDecrypts();
    if (emptyKey != null) return emptyKey;
    await showRecovery();
    return null;
  }
  return key;
}

Future<void>? _pendingDialog;

/// Tells the user the app's encryption key can't unlock this wallet's data
/// (so it can't [action], e.g. "sign this transaction") and that the wallet
/// must be deleted and recovered from its devices, routing into the existing
/// delete flow when a [WalletContext] is in scope.
///
/// Falls back to the root navigator when the caller has no [BuildContext]
/// (e.g. controllers reacting to stream events). Concurrent callers share a
/// single dialog.
Future<void> showWalletKeyMismatchDialog({
  BuildContext? context,
  required String action,
}) {
  final existing = _pendingDialog;
  if (existing != null) return existing;

  final ctx = context ?? rootNavKey.currentContext;
  if (ctx == null || !ctx.mounted) return Future.value();

  final future = _show(ctx, action);
  _pendingDialog = future;
  future.whenComplete(() => _pendingDialog = null);
  return future;
}

Future<void> _show(BuildContext context, String action) async {
  final walletCtx = WalletContext.of(context);
  final deleteWallet = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('Wallet needs recovery'),
      content: Text(
        "This phone's encryption key can't unlock this wallet's data, "
        "so it can't $action.\n\n"
        'This usually means the wallet was protected by a key this phone '
        'no longer has. To keep using the wallet, delete it from this app '
        'and recover it from its Frostsnap devices.'
        '${walletCtx == null ? "\n\nYou can delete it from the wallet's settings." : ''}',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Not now'),
        ),
        if (walletCtx != null)
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Delete wallet'),
          ),
      ],
    ),
  );

  if (deleteWallet == true && walletCtx != null && context.mounted) {
    await MaybeFullscreenDialog.show(
      context: context,
      child: walletCtx.wrap(DeleteWalletPage()),
    );
  }
}
