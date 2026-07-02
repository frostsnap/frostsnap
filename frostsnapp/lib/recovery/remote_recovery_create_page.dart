import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/recovery/remote_recovery_lobby_page.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/settings.dart' show BitcoinNetworkChooser;
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';

/// Leader-side entry point for the remote recovery flow. Collects
/// wallet name + optional threshold hint + network, opens a
/// recovery lobby via `NostrClient.createRemoteRecoveryLobby`, and
/// pushes the lobby page.
///
/// Joiners no longer land here — they arrive through
/// `wallet_add.dart`'s universal `JoinLinkPage`. The [dispatchJoin]
/// static exposed on this class is the seam the join dispatcher
/// calls into.
class RemoteRecoveryCreatePage extends StatefulWidget {
  final Coordinator coord;
  final NostrClient nostrClient;

  const RemoteRecoveryCreatePage({
    super.key,
    required this.coord,
    required this.nostrClient,
  });

  @override
  State<RemoteRecoveryCreatePage> createState() =>
      _RemoteRecoveryCreatePageState();

  /// The single call site that maps a leader's [CreateLobbyResult] onto
  /// [NostrClient.createRemoteRecoveryLobby]. Extracted (`@visibleForTesting`)
  /// so a regression that hard-codes `BitcoinNetwork.bitcoin` — bypassing
  /// [CreateLobbyResult.network] — is caught by a page-independent unit
  /// test rather than requiring the full lobby handle to stand up.
  @visibleForTesting
  static Future<RemoteRecoveryLobbyHandle> dispatchCreate({
    required NostrClient client,
    required NostrIdentity identity,
    required CreateLobbyResult result,
  }) {
    final secret = ChannelSecret.generate();
    return client.createRemoteRecoveryLobby(
      identity: identity,
      channelSecret: secret,
      keyName: result.keyName,
      purpose: keyPurposeBitcoin(network: result.network),
      thresholdHint: result.thresholdHint,
    );
  }

  /// Joiner entrypoint used by the universal `JoinLinkPage` dispatcher
  /// in `wallet_add.dart`. Handles the identity gate,
  /// `NostrClient.joinRemoteRecoveryLobby`, encryption-key fetch, and
  /// pushes [RemoteRecoveryLobbyPage] as joiner. Returns the popped
  /// [AccessStructureRef] on success, or null if the user cancelled
  /// mid-flow (unmounted, identity setup cancelled, lobby popped
  /// without recovery).
  static Future<AccessStructureRef?> dispatchJoin({
    required BuildContext context,
    required Coordinator coord,
    required NostrClient nostrClient,
    required String link,
  }) async {
    final nostr = NostrContext.of(context);
    final ensured = await nostr.ensureIdentity(context);
    if (ensured == null || !context.mounted) return null;
    final identity = nostr.nostrSettings.currentIdentity();
    if (identity == null || !context.mounted) return null;
    final secret = ChannelSecret.fromRecoveryLink(link: link);
    final handle = await nostrClient.joinRemoteRecoveryLobby(
      identity: identity,
      channelSecret: secret,
    );
    if (!context.mounted) return null;
    final encryptionKey = await SecureKeyProvider.getEncryptionKey();
    if (!context.mounted) return null;
    return Navigator.of(context).push<AccessStructureRef>(
      MaterialPageRoute(
        builder: (_) => RemoteRecoveryLobbyPage(
          handle: handle,
          isLeader: false,
          coord: coord,
          encryptionKey: encryptionKey,
        ),
      ),
    );
  }
}

class _RemoteRecoveryCreatePageState extends State<RemoteRecoveryCreatePage> {
  bool _busy = false;
  String? _error;

  Future<void> _createLobby() async {
    final result = await showDialog<CreateLobbyResult>(
      context: context,
      builder: (ctx) => const CreateLobbyDialog(),
    );
    if (result == null || !mounted) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final nostr = NostrContext.of(context);
      final ensured = await nostr.ensureIdentity(context);
      if (ensured == null || !mounted) return;
      final identity = nostr.nostrSettings.currentIdentity();
      if (identity == null || !mounted) return;
      final handle = await RemoteRecoveryCreatePage.dispatchCreate(
        client: widget.nostrClient,
        identity: identity,
        result: result,
      );
      if (!mounted) return;
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      if (!mounted) return;
      final asRef = await Navigator.of(context).push<AccessStructureRef>(
        MaterialPageRoute(
          builder: (_) => RemoteRecoveryLobbyPage(
            handle: handle,
            isLeader: true,
            coord: widget.coord,
            encryptionKey: encryptionKey,
          ),
        ),
      );
      if (asRef == null || !mounted) return;
      Navigator.of(context).pop(asRef);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: const Text('Start recovery lobby')),
      body: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Invite participants to help recover a wallet over nostr. '
              'Share the invite link so others can join with their shares.',
              style: theme.textTheme.bodyLarge,
            ),
            const SizedBox(height: 32),
            FilledButton.icon(
              icon: const Icon(Icons.add),
              label: const Text('Create recovery lobby'),
              onPressed: _busy ? null : _createLobby,
            ),
            if (_error != null) ...[
              const SizedBox(height: 24),
              Text(_error!, style: TextStyle(color: theme.colorScheme.error)),
            ],
          ],
        ),
      ),
    );
  }
}

class CreateLobbyResult {
  final String keyName;
  final int? thresholdHint;
  final BitcoinNetwork network;

  const CreateLobbyResult({
    required this.keyName,
    required this.thresholdHint,
    required this.network,
  });
}

class CreateLobbyDialog extends StatefulWidget {
  const CreateLobbyDialog({super.key});

  @override
  State<CreateLobbyDialog> createState() => _CreateLobbyDialogState();
}

class _CreateLobbyDialogState extends State<CreateLobbyDialog> {
  final _keyName = TextEditingController();
  final _threshold = TextEditingController();
  BitcoinNetwork _network = BitcoinNetwork.bitcoin;
  String? _err;

  @override
  void dispose() {
    _keyName.dispose();
    _threshold.dispose();
    super.dispose();
  }

  void _submit() {
    final name = _keyName.text.trim();
    if (name.isEmpty) {
      setState(() => _err = 'Wallet name is required');
      return;
    }
    int? hint;
    final rawHint = _threshold.text.trim();
    if (rawHint.isNotEmpty) {
      final parsed = int.tryParse(rawHint);
      if (parsed == null || parsed < 1) {
        setState(() => _err = 'Threshold hint must be a positive integer');
        return;
      }
      hint = parsed;
    }
    Navigator.of(context).pop(
      CreateLobbyResult(keyName: name, thresholdHint: hint, network: _network),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Create recovery lobby'),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _keyName,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: 'Wallet name',
                helperText: 'The name of the wallet being recovered',
              ),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _threshold,
              keyboardType: TextInputType.number,
              inputFormatters: [FilteringTextInputFormatter.digitsOnly],
              decoration: const InputDecoration(
                labelText: 'Threshold hint (optional)',
                helperText: 'How many shares are needed to recover',
              ),
            ),
            BitcoinNetworkChooser(
              value: _network,
              onChanged: (n) => setState(() => _network = n),
            ),
            if (_err != null) ...[
              const SizedBox(height: 16),
              Text(
                _err!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Create')),
      ],
    );
  }
}
