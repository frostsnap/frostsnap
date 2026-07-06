import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';

/// Connect (or create) a wallet's coordination channel and flip it
/// into remote mode. Shared by the post-keygen and post-recovery
/// completion flows.
///
/// The channel secret derives from the AccessStructureId, so when
/// the channel already exists on the relays this joins it (original
/// creation metadata stands); otherwise the creation event is
/// stamped with [participants] — the pubkey → share-index
/// assignment the Members page treats as source of truth.
///
/// Retries until it succeeds, showing a persistent progress dialog.
/// Never falls back to local mode.
Future<void> setupCoordinationChannel(
  BuildContext context, {
  required AccessStructureRef asRef,
  required List<ChannelParticipant> participants,
}) async {
  final status = ValueNotifier<String>('Connecting…');
  // Show a persistent dialog that stays up for the entire setup.
  final dialogRoute = DialogRoute(
    context: context,
    barrierDismissible: false,
    builder: (ctx) => ValueListenableBuilder<String>(
      valueListenable: status,
      builder: (ctx, text, _) => AlertDialog(
        icon: const SizedBox(
          width: 32,
          height: 32,
          child: CircularProgressIndicator(strokeWidth: 3),
        ),
        title: const Text('Setting up signing channel…'),
        content: Text(text),
      ),
    ),
  );
  Navigator.of(context).push(dialogRoute);

  try {
    while (context.mounted) {
      try {
        status.value = 'Connecting to relay…';
        final encryptionKey = await SecureKeyProvider.getEncryptionKey();
        final params = coord.connectMaybeCreateChannel(
          accessStructureRef: asRef,
          encryptionKey: encryptionKey,
          participants: participants,
        );
        if (!context.mounted) return;
        final nostr = NostrContext.of(context);
        final client = await nostr.nostrClient;
        final identity = nostr.nostrSettings.currentIdentity();
        if (identity == null) {
          throw StateError('nostr identity not configured');
        }
        final handle = await client.connectToChannel(
          identity: identity,
          params: params,
        );
        try {
          status.value = 'Waiting for channel confirmation…';
          // listen-then-start: invoking firstWhere subscribes to the
          // stream synchronously, so the broadcast sink is attached
          // before we call handle.start() and the runner emits.
          final waitForConfirmation = handle
              .events()
              .watch()
              .firstWhere((event) => event is ChannelEvent_ChannelState)
              .timeout(const Duration(seconds: 30));
          await handle.start();
          await waitForConfirmation;
          status.value = 'Enabling remote mode…';
          if (!context.mounted) return;
          await NostrContext.of(context).nostrSettings.setCoordinationUiEnabled(
            accessStructureRef: asRef,
            enabled: true,
          );
        } finally {
          handle.close();
        }
        return;
      } catch (e) {
        debugPrint('Channel setup attempt failed: $e — retrying');
        status.value = 'Retrying… ($e)';
        await Future.delayed(const Duration(seconds: 3));
      }
    }
  } finally {
    status.dispose();
    if (context.mounted && dialogRoute.isActive) {
      Navigator.of(context).removeRoute(dialogRoute);
    }
  }
}
