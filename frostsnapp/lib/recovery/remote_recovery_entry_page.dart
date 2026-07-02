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

/// Entry page for the remote recovery flow. Two paths:
///
/// - **Create**: caller supplies a wallet name (and optional
///   threshold hint) → `NostrClient.createRemoteRecoveryLobby` →
///   navigate to [RemoteRecoveryLobbyPage] as leader.
/// - **Join**: caller pastes a `frostsnap://recovery/<hex>` link →
///   `ChannelSecret.fromRecoveryLink` → `joinRemoteRecoveryLobby`
///   → navigate to [RemoteRecoveryLobbyPage] as joiner.
///
/// Both paths require a configured `NostrIdentity` (via
/// `NostrContext.ensureIdentity`); the entry page triggers the
/// nostr setup dialog if it's missing.
class RemoteRecoveryEntryPage extends StatefulWidget {
  final Coordinator coord;
  final NostrClient nostrClient;

  /// Deep-link entry: if non-null, the page auto-triggers the join flow
  /// with this link prefilled after first mount.
  final String? initialJoinLink;

  const RemoteRecoveryEntryPage({
    super.key,
    required this.coord,
    required this.nostrClient,
    this.initialJoinLink,
  });

  @override
  State<RemoteRecoveryEntryPage> createState() =>
      _RemoteRecoveryEntryPageState();

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
}

class _RemoteRecoveryEntryPageState extends State<RemoteRecoveryEntryPage> {
  bool _busy = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    if (widget.initialJoinLink != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        _joinLobby(prefilledLink: widget.initialJoinLink);
      });
    }
  }

  Future<void> _createLobby() async {
    final result = await showDialog<CreateLobbyResult>(
      context: context,
      builder: (ctx) => const CreateLobbyDialog(),
    );
    if (result == null || !mounted) return;
    await _connect(
      leader: true,
      identityGate: (identity) => RemoteRecoveryEntryPage.dispatchCreate(
        client: widget.nostrClient,
        identity: identity,
        result: result,
      ),
    );
  }

  Future<void> _joinLobby({String? prefilledLink}) async {
    final link =
        prefilledLink ??
        await showDialog<String>(
          context: context,
          builder: (ctx) => const _JoinLinkDialog(),
        );
    if (link == null || !mounted) return;
    await _connect(
      leader: false,
      identityGate: (identity) async {
        final secret = ChannelSecret.fromRecoveryLink(link: link);
        return widget.nostrClient.joinRemoteRecoveryLobby(
          identity: identity,
          channelSecret: secret,
        );
      },
    );
  }

  Future<void> _connect({
    required bool leader,
    required Future<RemoteRecoveryLobbyHandle> Function(NostrIdentity)
    identityGate,
  }) async {
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
      final handle = await identityGate(identity);
      if (!mounted) return;
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      if (!mounted) return;
      final asRef = await Navigator.of(context).push<AccessStructureRef>(
        MaterialPageRoute(
          builder: (_) => RemoteRecoveryLobbyPage(
            handle: handle,
            isLeader: leader,
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
      appBar: AppBar(title: const Text('Remote recovery')),
      body: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Recover a wallet with participants over nostr.',
              style: theme.textTheme.bodyLarge,
            ),
            const SizedBox(height: 32),
            FilledButton.icon(
              icon: const Icon(Icons.add),
              label: const Text('Create recovery lobby'),
              onPressed: _busy ? null : _createLobby,
            ),
            const SizedBox(height: 16),
            OutlinedButton.icon(
              icon: const Icon(Icons.login),
              label: const Text('Join with invite link'),
              onPressed: _busy ? null : _joinLobby,
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

class _JoinLinkDialog extends StatefulWidget {
  const _JoinLinkDialog();

  @override
  State<_JoinLinkDialog> createState() => _JoinLinkDialogState();
}

class _JoinLinkDialogState extends State<_JoinLinkDialog> {
  final _link = TextEditingController();
  String? _err;

  @override
  void dispose() {
    _link.dispose();
    super.dispose();
  }

  void _submit() {
    final raw = _link.text.trim();
    if (!raw.startsWith('frostsnap://recovery/')) {
      setState(() => _err = 'Must be a frostsnap://recovery/… link');
      return;
    }
    Navigator.of(context).pop(raw);
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Join recovery lobby'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          TextField(
            controller: _link,
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Invite link',
              hintText: 'frostsnap://recovery/…',
            ),
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
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Join')),
      ],
    );
  }
}
