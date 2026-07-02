import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/network_advanced_options.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/recovery/remote_recovery_lobby_page.dart';
import 'package:frostsnap/restoration/device_discovery.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';

/// The remote-recovery ceremony as ONE stepped dialog: the leader's
/// create form hands off to the lobby inside the same
/// `MaybeFullscreenDialog`; joiners enter directly at the lobby step
/// (via [dispatchJoin]). Pops the recovered [AccessStructureRef]
/// when this participant's persist lands.
class RemoteRecoveryPage extends StatefulWidget {
  final Coordinator coord;
  final NostrClient nostrClient;

  /// Joiner entry: lobby handle + encryption key already acquired by
  /// [dispatchJoin]. Null for the leader, who starts at the create
  /// form and acquires them on submit.
  final RemoteRecoveryLobbyHandle? initialHandle;
  final SymmetricKey? initialEncryptionKey;

  const RemoteRecoveryPage.create({
    super.key,
    required this.coord,
    required this.nostrClient,
  }) : initialHandle = null,
       initialEncryptionKey = null;

  const RemoteRecoveryPage.joined({
    super.key,
    required this.coord,
    required this.nostrClient,
    required RemoteRecoveryLobbyHandle handle,
    required SymmetricKey encryptionKey,
  }) : initialHandle = handle,
       initialEncryptionKey = encryptionKey;

  @override
  State<RemoteRecoveryPage> createState() => _RemoteRecoveryPageState();

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
  /// shows the ceremony starting at the lobby step. Returns the popped
  /// [AccessStructureRef] on success, or null if the user cancelled
  /// mid-flow (unmounted, identity setup cancelled, lobby closed
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
    return MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: RemoteRecoveryPage.joined(
        coord: coord,
        nostrClient: nostrClient,
        handle: handle,
        encryptionKey: encryptionKey,
      ),
    );
  }
}

class _RemoteRecoveryPageState extends State<RemoteRecoveryPage> {
  // Create-step state.
  bool _creating = false;
  String? _createError;

  // Lobby-step state — live once [_handle] is set (immediately for
  // joiners, after the create form submits for the leader).
  RemoteRecoveryLobbyHandle? _handle;
  SymmetricKey? _encryptionKey;
  bool _isLeader = true;
  PublicKey? _myPubkey;
  StreamSubscription<RecoveryLobbyState>? _sub;
  RecoveryLobbyState? _state;
  bool _finishing = false;
  bool _persisting = false;
  String? _error;
  AccessStructureRef? _recoveredRef;
  bool _verificationFailed = false;

  @override
  void initState() {
    super.initState();
    final handle = widget.initialHandle;
    if (handle != null) {
      _attachHandle(handle, widget.initialEncryptionKey!, isLeader: false);
    }
  }

  /// Wire the lobby-step machinery to a live handle. Per the
  /// `nostr_recovery_transport` plan the handle is already started
  /// (create/join return only after the bridge's first
  /// `StateChanged` seeds the broadcast) — we just subscribe via
  /// `handle.subState().watch()`; no `start()`, no `close()`.
  void _attachHandle(
    RemoteRecoveryLobbyHandle handle,
    SymmetricKey encryptionKey, {
    required bool isLeader,
  }) {
    _handle = handle;
    _encryptionKey = encryptionKey;
    _isLeader = isLeader;
    _myPubkey = handle.myPubkey();
    _sub = handle.subState().watch().listen(_onState);
    // Announce ourselves so peers (and our own participant list)
    // see us before we've posted a key share. Best-effort — a
    // failed publish shouldn't block the lobby.
    unawaited(
      handle.announcePresence().catchError((Object e) {
        debugPrint('announcePresence failed: $e');
      }),
    );
    // Side-channel: FinishVerificationFailed is not exposed on the
    // state broadcast (the transport surfaces it via awaitFinished()
    // returning Err). Fire-and-forget so we surface the error banner
    // without blocking the state pipeline.
    unawaited(_watchFinished(handle));
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  Future<void> _submitCreate(CreateLobbyResult result) async {
    setState(() {
      _creating = true;
      _createError = null;
    });
    try {
      final nostr = NostrContext.of(context);
      final ensured = await nostr.ensureIdentity(context);
      if (ensured == null || !mounted) return;
      final identity = nostr.nostrSettings.currentIdentity();
      if (identity == null || !mounted) return;
      final handle = await RemoteRecoveryPage.dispatchCreate(
        client: widget.nostrClient,
        identity: identity,
        result: result,
      );
      if (!mounted) return;
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      if (!mounted) return;
      setState(() => _attachHandle(handle, encryptionKey, isLeader: true));
    } catch (e) {
      if (!mounted) return;
      setState(() => _createError = '$e');
    } finally {
      if (mounted) setState(() => _creating = false);
    }
  }

  /// Watches the `awaitFinished()` future. On success we don't need
  /// to do anything — the state broadcast already carries the
  /// `finished` field and the `_onState` handler kicks off persist.
  /// On error the transport is signalling FinishVerificationFailed
  /// (the leader picked a subset that doesn't reconstruct); flip
  /// the flag so the Recover button locks and the banner appears.
  Future<void> _watchFinished(RemoteRecoveryLobbyHandle handle) async {
    try {
      await handle.awaitFinished();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _verificationFailed = true;
        _error =
            "The leader's Finish message doesn't match — protocol bug or malicious leader. Aborting.";
      });
    }
  }

  void _onState(RecoveryLobbyState state) {
    if (!mounted) return;
    setState(() => _state = state);
    if (state.finished != null &&
        _recoveredRef == null &&
        !_persisting &&
        _error == null) {
      unawaited(_persist());
    }
  }

  Future<void> _persist() async {
    setState(() => _persisting = true);
    try {
      final asref = await _handle!.persistRecovered(
        coord: widget.coord,
        encryptionKey: _encryptionKey!,
      );
      if (!mounted) return;
      setState(() => _recoveredRef = asref);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        Navigator.of(context).pop(asref);
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _persisting = false);
    }
  }

  Future<void> _finish() async {
    final winning = _state?.currentRecovery?.winningShareRefs;
    if (winning == null || winning.isEmpty) return;
    setState(() {
      _finishing = true;
      _error = null;
    });
    try {
      await _handle!.finish(shareRefs: winning);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    } finally {
      if (mounted) setState(() => _finishing = false);
    }
  }

  Future<void> _cancel() async {
    try {
      await _handle!.cancel();
    } catch (e) {
      if (mounted) setState(() => _error = '$e');
    }
  }

  /// Joiner exit: publish Leave so peers see this participant go,
  /// then close the ceremony. Mirrors the keygen lobby's
  /// leaveLobby-then-pop footer semantics.
  Future<void> _leave() async {
    await _handle!.leave();
    if (!mounted) return;
    Navigator.of(context).pop();
  }

  /// Load-share flow: the exact same device-discovery + recovery
  /// flow local recovery uses (glowy plug-in prompt, blank-device
  /// backup entry, device-with-share confirmation), run with
  /// [RemoteLobbyContext] so it completes with a [RemoteShareResult]
  /// instead of writing coordinator state.
  Future<void> _loadShare() async {
    final result = await MaybeFullscreenDialog.show<RemoteShareResult>(
      context: context,
      barrierDismissible: true,
      child: const RecoveryFlowWithDiscovery(
        recoveryContext: RecoveryContext.remoteLobby(),
      ),
    );
    if (result == null || !mounted) return;
    try {
      final post = sharePostFromRemoteResult(
        result,
        deviceNameOf: (id) => widget.coord.getDeviceName(id: id),
      );
      await _handle!.postShare(post: post);
      if (!mounted) return;
      setState(() => _error = null);
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = '$e');
    }
  }

  /// A live lobby must be exited through the footer (Cancel lobby /
  /// Leave lobby) so peers hear about it — system back is blocked,
  /// matching the keygen lobby. Terminal states (cancelled /
  /// finished) have nothing left to announce, so Close and back
  /// both work.
  bool get _lobbyLive {
    if (_handle == null) return false;
    final s = _state;
    if (s != null && (s.cancelled || s.finished != null)) return false;
    return true;
  }

  @override
  Widget build(BuildContext context) {
    final handle = _handle;
    return PopScope(
      canPop: !_creating && !_finishing && !_persisting && !_lobbyLive,
      child: SafeArea(
        // One dialog, two steps: the create form hands off to the
        // lobby in place (cross-fade) instead of stacking a second
        // dialog.
        child: AnimatedSwitcher(
          duration: Durations.medium2,
          child: handle == null
              ? KeyedSubtree(
                  key: const ValueKey('createStep'),
                  child: CreateLobbyForm(
                    busy: _creating,
                    connectError: _createError,
                    onSubmit: (result) => unawaited(_submitCreate(result)),
                  ),
                )
              : KeyedSubtree(
                  key: const ValueKey('lobbyStep'),
                  child: RecoveryLobbyView(
                    state: _state,
                    isLeader: _isLeader,
                    myPubkey: _myPubkey!,
                    inviteLink: handle.inviteLink(),
                    finishing: _finishing,
                    persisting: _persisting,
                    error: _error,
                    recoveredRef: _recoveredRef,
                    verificationFailed: _verificationFailed,
                    onFinish: _finish,
                    onCancel: _cancel,
                    onLeave: _leave,
                    onLoadShare: _loadShare,
                  ),
                ),
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

/// The leader's create-lobby form as a ceremony dialog step. Pure —
/// validation + collected values surface through [onSubmit] so
/// widget tests can drive it without a live `NostrClient`.
class CreateLobbyForm extends StatefulWidget {
  const CreateLobbyForm({
    super.key,
    required this.onSubmit,
    this.busy = false,
    this.connectError,
  });

  final void Function(CreateLobbyResult) onSubmit;
  final bool busy;

  /// Failure from the lobby-create call, rendered on the name field.
  final String? connectError;

  @override
  State<CreateLobbyForm> createState() => _CreateLobbyFormState();
}

class _CreateLobbyFormState extends State<CreateLobbyForm> {
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
    if (widget.busy) return;
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
    setState(() => _err = null);
    widget.onSubmit(
      CreateLobbyResult(keyName: name, thresholdHint: hint, network: _network),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MultiStepDialogScaffold(
      stepKey: 'createRecoveryLobby',
      title: const Text('Start a recovery lobby'),
      subtitle:
          'Name the wallet being recovered, then invite the other '
          'share holders.',
      showClose: true,
      body: SliverToBoxAdapter(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _keyName,
              autofocus: true,
              decoration: InputDecoration(
                border: const OutlineInputBorder(),
                labelText: 'Wallet name',
                helperText: 'The name of the wallet being recovered',
                errorText: _err ?? widget.connectError,
                errorMaxLines: 2,
              ),
              textCapitalization: TextCapitalization.words,
              onSubmitted: (_) => _submit(),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _threshold,
              keyboardType: TextInputType.number,
              inputFormatters: [FilteringTextInputFormatter.digitsOnly],
              decoration: const InputDecoration(
                border: OutlineInputBorder(),
                labelText: 'Threshold hint (optional)',
                helperText: 'How many shares are needed to recover',
              ),
              onSubmitted: (_) => _submit(),
            ),
            NetworkAdvancedOptions(
              selected: _network,
              onChanged: (n) => setState(() => _network = n),
            ),
          ],
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          onPressed: widget.busy ? null : _submit,
          icon: widget.busy
              ? const SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.arrow_forward_rounded),
          iconAlignment: IconAlignment.end,
          label: const Text('Create'),
        ),
      ),
    );
  }
}
