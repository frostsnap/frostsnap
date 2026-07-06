import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/nostr_chat/channel_setup.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/restoration/enter_threshold_view.dart';
import 'package:frostsnap/restoration/enter_wallet_name_view.dart';
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

enum _CreateStep { walletName, threshold }

class _RemoteRecoveryPageState extends State<RemoteRecoveryPage> {
  // Create-phase state: two steps reusing local recovery's views
  // (wallet name + network, then the threshold "I'm not sure" /
  // "I know" selector) before the lobby handle exists.
  bool _creating = false;
  String? _createError;
  _CreateStep _createStep = _CreateStep.walletName;
  bool _createForward = true;
  String? _pendingName;
  BitcoinNetwork? _pendingNetwork;
  bool _nameCanSubmit = false;
  final _nameKey = GlobalKey<EnterWalletNameViewState>();
  final _thresholdKey = GlobalKey<EnterThresholdViewState>();

  // Lobby-step state — live once [_handle] is set (immediately for
  // joiners, after the create form submits for the leader).
  RemoteRecoveryLobbyHandle? _handle;
  SymmetricKey? _encryptionKey;
  bool _isLeader = true;
  PublicKey? _myPubkey;
  StreamSubscription<RecoveryLobbySnapshot>? _sub;
  RecoveryLobbySnapshot? _snapshot;
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

  void _submitWalletName(String walletName, BitcoinNetwork network) {
    setState(() {
      _pendingName = walletName;
      _pendingNetwork = network;
      _createForward = true;
      _createStep = _CreateStep.threshold;
    });
  }

  void _submitThreshold(int? threshold) {
    final name = _pendingName;
    final network = _pendingNetwork;
    if (name == null || network == null) return;
    unawaited(
      _submitCreate(
        CreateLobbyResult(
          keyName: name,
          thresholdHint: threshold,
          network: network,
        ),
      ),
    );
  }

  void _createStepBack() {
    setState(() {
      _createForward = false;
      _createStep = _CreateStep.walletName;
    });
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

  void _onState(RecoveryLobbySnapshot snapshot) {
    if (!mounted) return;
    setState(() => _snapshot = snapshot);
    if (snapshot.state.finished != null &&
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
      // A wallet recovered over nostr IS a remote wallet: connect
      // its coordination channel (rejoining the original — the
      // channel secret derives from the AccessStructureId — or
      // creating it stamped with the recovered share assignment)
      // and enable the remote shell before handing the asRef back.
      await setupCoordinationChannel(
        context,
        asRef: asref,
        participants: _handle!.channelParticipants(),
      );
      if (!mounted) return;
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
    final winning = _snapshot?.state.currentRecovery?.winningShareRefs;
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
    final s = _snapshot?.state;
    if (s != null && (s.cancelled || s.finished != null)) return false;
    return true;
  }

  MultiStepDialogScaffold _buildCreateStep(BuildContext context) {
    final theme = Theme.of(context);
    final errorText = _createError == null
        ? null
        : Padding(
            padding: const EdgeInsets.only(top: 16),
            child: Text(
              _createError!,
              style: TextStyle(color: theme.colorScheme.error),
            ),
          );
    switch (_createStep) {
      case _CreateStep.walletName:
        return MultiStepDialogScaffold(
          stepKey: 'recoveryWalletName',
          title: const Text('Wallet name'),
          showClose: true,
          forward: _createForward,
          reverseDuration: Duration.zero,
          body: SliverToBoxAdapter(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                EnterWalletNameView(
                  key: _nameKey,
                  initialWalletName: _pendingName,
                  initialBitcoinNetwork: _pendingNetwork,
                  intro:
                      'Enter the name of the wallet being recovered. '
                      "It's written on physical backups — if it's missing "
                      'or unreadable, choose another name.',
                  onChanged: (canSubmit) {
                    if (canSubmit != _nameCanSubmit) {
                      setState(() => _nameCanSubmit = canSubmit);
                    }
                  },
                  onSubmit: _submitWalletName,
                ),
                if (errorText != null) errorText,
              ],
            ),
          ),
          footer: Align(
            alignment: Alignment.centerRight,
            child: FilledButton(
              onPressed: _nameCanSubmit
                  ? () => _nameKey.currentState?.submit()
                  : null,
              child: const Text('Continue'),
            ),
          ),
        );
      case _CreateStep.threshold:
        return MultiStepDialogScaffold(
          stepKey: 'recoveryThreshold',
          title: const Text('Wallet Threshold (Optional)'),
          leading: IconButton(
            icon: const Icon(Icons.arrow_back_rounded),
            onPressed: _creating ? null : _createStepBack,
            tooltip: 'Back',
          ),
          forward: _createForward,
          reverseDuration: Duration.zero,
          body: SliverToBoxAdapter(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                EnterThresholdView(
                  key: _thresholdKey,
                  onSubmit: _submitThreshold,
                ),
                if (errorText != null) errorText,
              ],
            ),
          ),
          footer: Align(
            alignment: Alignment.centerRight,
            child: FilledButton.icon(
              onPressed: _creating
                  ? null
                  : () => _thresholdKey.currentState?.submit(),
              icon: _creating
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

  @override
  Widget build(BuildContext context) {
    final handle = _handle;
    final onThresholdStep =
        handle == null && _createStep == _CreateStep.threshold;
    return PopScope(
      canPop:
          !_creating &&
          !_finishing &&
          !_persisting &&
          !_lobbyLive &&
          !onThresholdStep,
      onPopInvokedWithResult: (didPop, _) {
        // System back on the threshold step steps back to the name
        // step instead of dismissing the ceremony.
        if (!didPop && onThresholdStep && !_creating) {
          _createStepBack();
        }
      },
      child: SafeArea(
        // One dialog, two steps: the create form hands off to the
        // lobby in place (cross-fade) instead of stacking a second
        // dialog.
        child: AnimatedSwitcher(
          duration: Durations.medium2,
          child: handle == null
              ? KeyedSubtree(
                  key: const ValueKey('createStep'),
                  child: _buildCreateStep(context),
                )
              : KeyedSubtree(
                  key: const ValueKey('lobbyStep'),
                  child: RecoveryLobbyView(
                    snapshot: _snapshot,
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
