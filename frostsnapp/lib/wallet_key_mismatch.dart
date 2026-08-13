import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/log.dart';

/// The classification of the app key against one existing wallet, with no UI.
/// The batch auto-exit uses this directly so it can finish the decryptable
/// wallets and show the recovery dialog once, rather than pausing on a modal per
/// dead wallet.
sealed class WalletKeyResolution {}

/// The key is present and decrypts this wallet.
class WalletKeyResolved extends WalletKeyResolution {
  final SymmetricKey key;
  WalletKeyResolved(this.key);
}

/// The wallet is known but the app key does not open it — a live key that does
/// not decrypt it (what clearing the lock screen leaves behind) or one the
/// platform reports permanently invalidated. The only outcome that warrants
/// advising recovery.
class WalletKeyNeedsRecovery extends WalletKeyResolution {}

/// The key could not be obtained for a transient or unrelated reason, or the
/// wallet is unknown. Non-destructive; surfaced as an ordinary error.
class WalletKeyUnavailable extends WalletKeyResolution {
  final String detail;
  WalletKeyUnavailable(this.detail);
}

Future<WalletKeyResolution> resolveWalletKey(
  AccessStructureRef accessStructureRef,
) async {
  final SymmetricKey key;
  try {
    key = await SecureKeyProvider.getEncryptionKey();
  } catch (e) {
    // getEncryptionKey heals a dead key in place, so a throw here is a genuine
    // platform failure, never an invalidated key. Recovery is driven instead by
    // a wrong-key ciphertext mismatch below.
    log(
      level: LogLevel.error,
      message:
          'resolveWalletKey: could not acquire key ($accessStructureRef): $e',
    );
    return WalletKeyUnavailable(
      e is PlatformException ? (e.message ?? e.code) : e.toString(),
    );
  }

  switch (coord.walletKeyStatus(
    accessStructureRef: accessStructureRef,
    encryptionKey: key,
  )) {
    case WalletKeyStatus.ok:
      return WalletKeyResolved(key);
    case WalletKeyStatus.wrongKey:
      return WalletKeyNeedsRecovery();
    case WalletKeyStatus.unknown:
      log(
        level: LogLevel.error,
        message: 'resolveWalletKey: no access structure ($accessStructureRef)',
      );
      return WalletKeyUnavailable('wallet not found');
  }
}

/// Acquires the app encryption key for a single operation on an existing wallet,
/// presenting the right message — the recovery dialog or an ordinary error — and
/// returning null when the caller must abort.
Future<SymmetricKey?> existingWalletKey({
  BuildContext? context,
  required AccessStructureRef accessStructureRef,
  required String action,
}) async {
  switch (await resolveWalletKey(accessStructureRef)) {
    case WalletKeyResolved(:final key):
      return key;
    case WalletKeyNeedsRecovery():
      await showWalletKeyMismatchDialog(context: context, action: action);
      return null;
    case WalletKeyUnavailable(:final detail):
      showOrdinaryKeyError(action, detail);
      return null;
  }
}

void showOrdinaryKeyError(String action, String detail) {
  rootScaffoldMessengerKey.currentState?.showSnackBar(
    SnackBar(content: Text("Couldn't $action: $detail")),
  );
}

Future<void>? _pendingDialog;

/// Concurrent callers share one dialog rather than stacking — several paths can
/// fail at once.
Future<void> showWalletKeyMismatchDialog({
  BuildContext? context,
  required String action,
}) {
  final existing = _pendingDialog;
  if (existing != null) return existing;

  final future = _resolveThenShow(context, action);
  _pendingDialog = future;
  future.whenComplete(() => _pendingDialog = null);
  return future;
}

/// A supplied context may be unmounted (post-await) or absent (a controller
/// reacting to the device stream before `runApp` mounts the navigator). In either
/// case fall back to the root navigator, waiting briefly for it to mount rather
/// than dropping the message — the silent path this dialog exists to close. With
/// no [WalletContext] the dialog shows its OK / "delete from settings" form.
Future<void> _resolveThenShow(BuildContext? context, String action) async {
  if (context != null && context.mounted) {
    await _show(context, action);
    return;
  }
  BuildContext? ctx = rootNavKey.currentContext;
  for (int i = 0; i < 100 && ctx == null; i++) {
    await Future.delayed(const Duration(milliseconds: 50));
    ctx = rootNavKey.currentContext;
  }
  if (ctx != null && ctx.mounted) {
    await _show(ctx, action);
  }
}

Future<void> _show(BuildContext context, String action) async {
  final walletCtx = WalletContext.of(context);
  final deleteWallet = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('Wallet needs recovery'),
      content: Text(
        "This wallet's data is encrypted, and your phone has lost the "
        'ability to unlock it in order to $action. This usually happens '
        'when the screen lock (PIN, pattern, or password) is changed or '
        'removed.\n\n'
        'To get this wallet working again, delete it from the app and '
        'restore it using your Frostsnap devices.'
        '${walletCtx == null ? "\n\nYou can delete it from the wallet's settings." : ""}',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(walletCtx == null ? 'OK' : 'Not now'),
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
