import 'dart:async';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/async_action_button.dart';
import 'package:frostsnap/camera/camera.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_setup_step.dart';
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/hex.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/network_advanced_options.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/nostr_chat/setup_dialog.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/keygen.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/keygen_run.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_keygen.dart';
import 'package:frostsnap/threshold_selector.dart';
import 'package:frostsnap/wallet_create.dart' show LargeCircularProgressIndicator;
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:sliver_tools/sliver_tools.dart';

// =============================================================================
// Steps
// =============================================================================

enum OrgKeygenStep {
  walletType,
  sessionRole,
  joinSession,
  nameWallet,
}

enum LobbyAndKeygenStep { lobby, review, acceptKeygen }

enum OrgKeygenRole { host, participant }

/// Result popped by `OrgKeygenPage` when the user picks a wallet type on
/// the first step. Organisation continues inside the page — the page
/// pops with `null` at that point only if the user backs out.
enum WalletTypeChoice { personal }

// =============================================================================
// Controller
// =============================================================================

/// Pre-lobby controller. Collects the user's wallet-type / role / name /
/// join-link choices and produces a [RemoteLobbyHandle] when they submit.
/// Once the handle is in hand, [OrgKeygenPage] navigates (via
/// `pushReplacement`) to [LobbyAndKeygenPage] which owns its own
/// [LobbyAndKeygenController] with a non-null handle. There is deliberately
/// no "connecting" step here: the only on-screen acknowledgement of the
/// pending lobby creation is the loading state on the submit button.
class OrgKeygenController extends ChangeNotifier {
  OrgKeygenController({required this.nostrClient});

  final NostrClient nostrClient;

  OrgKeygenStep _step = OrgKeygenStep.walletType;
  OrgKeygenStep get step => _step;

  OrgKeygenRole _role = OrgKeygenRole.host;
  OrgKeygenRole get role => _role;
  bool get isHost => _role == OrgKeygenRole.host;

  final nameController = TextEditingController();
  final joinLinkController = TextEditingController();

  String get walletName => nameController.text.trim();
  bool get nameValid => walletName.isNotEmpty && walletName.length <= 15;
  bool get joinLinkValid =>
      joinLinkController.text.trim().startsWith('frostsnap://keygen/');

  /// Host-side network selection (developer-mode only). Defaults to
  /// mainnet; feeds into `key_purpose_bitcoin(network)` when creating
  /// the lobby.
  BitcoinNetwork _network = BitcoinNetwork.bitcoin;
  BitcoinNetwork get network => _network;
  void setNetwork(BitcoinNetwork n) {
    _network = n;
    notifyListeners();
  }

  /// Surfaces a `createRemoteLobby` / `joinRemoteLobby` failure on the
  /// originating step (nameWallet / joinSession). Cleared on the next
  /// submit.
  String? _connectError;
  String? get connectError => _connectError;

  /// True while a `createRemoteLobby` / `joinRemoteLobby` await is in
  /// flight. Drives the submit button's spinner.
  bool _connecting = false;
  bool get connecting => _connecting;

  // --- step transitions ---

  void chosePersonal(BuildContext context) {
    Navigator.of(context).pop(WalletTypeChoice.personal);
  }

  void choseOrganisation() {
    _step = OrgKeygenStep.sessionRole;
    notifyListeners();
  }

  void chooseCreateSession() {
    _role = OrgKeygenRole.host;
    _step = OrgKeygenStep.nameWallet;
    notifyListeners();
  }

  void chooseJoinSession() {
    _role = OrgKeygenRole.participant;
    _step = OrgKeygenStep.joinSession;
    notifyListeners();
  }

  /// Awaits `createRemoteLobby`. Returns the handle on success; sets
  /// `connectError` and returns null on failure. The caller is expected
  /// to navigate to [LobbyAndKeygenPage] when this returns non-null.
  Future<RemoteLobbyHandle?> openLobbyAsHost() async {
    if (!nameValid) return null;
    _connecting = true;
    _connectError = null;
    notifyListeners();
    try {
      final nsec = await _loadNsec();
      final secret = ChannelSecret.generate();
      final handle = await nostrClient.createRemoteLobby(
        channelSecret: secret,
        nsec: nsec,
        keyName: walletName,
        purpose: keyPurposeBitcoin(network: _network),
      );
      return handle;
    } catch (e) {
      _connectError = '$e';
      return null;
    } finally {
      _connecting = false;
      notifyListeners();
    }
  }

  /// Joiner counterpart to [openLobbyAsHost].
  Future<RemoteLobbyHandle?> openLobbyAsJoiner() async {
    if (!joinLinkValid) return null;
    _connecting = true;
    _connectError = null;
    notifyListeners();
    try {
      final secret = ChannelSecret.fromKeygenLink(
        link: joinLinkController.text.trim(),
      );
      final nsec = await _loadNsec();
      final handle = await nostrClient.joinRemoteLobby(
        channelSecret: secret,
        nsec: nsec,
      );
      return handle;
    } catch (e) {
      _connectError = '$e';
      return null;
    } finally {
      _connecting = false;
      notifyListeners();
    }
  }

  Future<String> _loadNsec() async {
    throw UnimplementedError(
      '_loadNsec must be overridden by the owning page where NostrContext is accessible',
    );
  }

  void back(BuildContext context) {
    switch (_step) {
      case OrgKeygenStep.walletType:
        Navigator.of(context).pop();
        return;
      case OrgKeygenStep.sessionRole:
        _step = OrgKeygenStep.walletType;
      case OrgKeygenStep.joinSession:
        _step = OrgKeygenStep.sessionRole;
      case OrgKeygenStep.nameWallet:
        _step = OrgKeygenStep.sessionRole;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    nameController.dispose();
    joinLinkController.dispose();
    super.dispose();
  }
}

/// Owns the in-flight lobby session: the [RemoteLobbyHandle] (final,
/// non-null), the live [LobbyState] stream, the [DeviceSetupController]
/// for the "Add devices" dialog, and the post-StartKeygen substep
/// transitions.
///
/// Constructed by [LobbyAndKeygenPage] from a handle that has already
/// been successfully created — there is no construction path that leaves
/// `handle` unset, so the "lobby handle is gone" error class is
/// structurally impossible.
class LobbyAndKeygenController extends ChangeNotifier {
  LobbyAndKeygenController({
    required this.handle,
    required this.isHost,
    required this.walletName,
    required Future<String> Function() loadNsec,
  }) : _loadNsec = loadNsec,
       _myPubkey = handle.myPubkey() {
    deviceSetup.addListener(notifyListeners);
    final broadcastSub = handle.subState();
    _stateBroadcastSub = broadcastSub;
    _stateSub = broadcastSub.start().listen((state) {
      _state = state;
      notifyListeners();
    });
  }

  final RemoteLobbyHandle handle;
  final bool isHost;
  final String walletName;
  // ignore: unused_field
  final Future<String> Function() _loadNsec;

  final PublicKey _myPubkey;
  PublicKey get myPubkey => _myPubkey;

  LobbyAndKeygenStep _step = LobbyAndKeygenStep.lobby;
  LobbyAndKeygenStep get step => _step;

  LobbyState? _state;
  LobbyState? get lobbyState => _state;
  StreamSubscription<LobbyState>? _stateSub;
  // Held alive so Dart-side GC doesn't collect the opaque subscription
  // handle and trigger Rust's `_stop()`. See the analogous comment on
  // `_AcceptKeygenWaitingViewState._kgBroadcastSub`.
  LobbyStateBroadcastSubscription? _stateBroadcastSub;

  /// Shared with `_DeviceSetupDialog` so typed names + upgrade state
  /// survive across dialog close / reopen. Owned by this controller —
  /// the dialog merely subscribes for rebuilds.
  final DeviceSetupController deviceSetup = DeviceSetupController();

  /// Host-side local threshold choice. Never published on its own —
  /// only put on the wire as part of `StartKeygen`.
  int? _pendingThreshold;

  /// Host-only: pubkeys (as hex) that the host has deselected from the
  /// keygen. The toggle UI lives on Ready participant rows; the
  /// excluded set drops out of `selected` when `startKeygen` is called.
  /// Self-exclusion is rejected.
  final Set<String> _excludedHex = {};

  bool isExcluded(PublicKey pk) => _excludedHex.contains(pk.toHex());

  void setIncluded(PublicKey pk, bool included) {
    if (pk == _myPubkey) return;
    final hex = pk.toHex();
    final changed = included ? _excludedHex.remove(hex) : _excludedHex.add(hex);
    if (changed) notifyListeners();
  }

  /// Devices counted across the *included* Ready participants. Drives
  /// the threshold slider's domain and the "Continue with N devices"
  /// label, so that excluding a participant updates the UI immediately.
  int get totalDevices {
    final s = _state;
    if (s == null) return 0;
    return s.participants.values.fold(0, (sum, p) {
      if (_excludedHex.contains(p.pubkey.toHex())) return sum;
      return sum + p.devices.length;
    });
  }

  int get recommendedThreshold {
    final total = totalDevices;
    if (total <= 1) return 1;
    return max((total * 2 / 3).ceil(), 1).clamp(1, total);
  }

  int get displayThreshold => _pendingThreshold ?? recommendedThreshold;

  void setPendingThreshold(int v) {
    _pendingThreshold = v;
    notifyListeners();
  }

  /// Whether the local user has already marked themselves Ready.
  bool get meIsReady {
    final s = _state;
    if (s == null) return false;
    return s.participants.values.any(
      (p) => p.pubkey == _myPubkey && p.status == ParticipantStatus.ready,
    );
  }

  /// Throws on failure so the caller (the device-setup dialog) can keep
  /// itself open and surface the error. No more "handle is gone" — the
  /// handle is final and non-null by construction.
  Future<void> markReady(List<({DeviceId id, String name})> devices) async {
    final regs = devices
        .map((d) => DeviceRegistration(
              deviceId: d.id,
              name: d.name,
              kind: DeviceKind.frostsnap,
            ))
        .toList();
    await handle.markReady(devices: regs);
  }

  /// Host-only. Enter the review screen locally — nothing is published
  /// until the host taps "Generate keys".
  void goToReview() {
    final s = _state;
    if (s == null || !s.allReady()) return;
    _pendingThreshold ??= recommendedThreshold;
    _step = LobbyAndKeygenStep.review;
    notifyListeners();
  }

  void goToAcceptKeygen() {
    if (_step == LobbyAndKeygenStep.acceptKeygen) return;
    _step = LobbyAndKeygenStep.acceptKeygen;
    notifyListeners();
  }

  /// Host-only. Publish `StartKeygen`.
  Future<void> startKeygen() async {
    final s = _state;
    if (s == null) throw StateError('no lobby state yet');
    final threshold = _pendingThreshold ?? recommendedThreshold;
    final selected = <SelectedCoordinator>[];
    for (final p in s.participants.values) {
      if (p.status != ParticipantStatus.ready) continue;
      if (_excludedHex.contains(p.pubkey.toHex())) continue;
      final regId = p.registerEventId;
      if (regId == null) continue;
      selected.add(SelectedCoordinator(pubkey: p.pubkey, registerEventId: regId));
    }
    if (selected.isEmpty) {
      throw StateError('no Ready participants to include');
    }
    await handle.startKeygen(threshold: threshold, selected: selected);
  }

  /// Selected joiners only. Publish `AckKeygen`.
  Future<void> ackKeygen() async {
    final s = _state;
    if (s == null || s.keygen == null) {
      throw StateError('no pending keygen to ack');
    }
    await handle.ackKeygen(startKeygenEventId: s.keygen!.keygenEventId);
  }

  /// Host-only. Publish `CancelLobby` — the page pops on the resulting
  /// `state.cancelled = true` transition (host-locally + relay-echoed
  /// for joiners).
  Future<void> cancelLobby() => handle.cancel();

  /// Joiner-side. Publish `Leave` and await relay OK so other
  /// participants reliably see us drop.
  Future<void> leaveLobby() => handle.leave();

  /// Within-page back: only review→lobby is reversible. Returns true if
  /// handled, false if the caller should pop the page itself.
  bool back() {
    switch (_step) {
      case LobbyAndKeygenStep.lobby:
      case LobbyAndKeygenStep.acceptKeygen:
        return false;
      case LobbyAndKeygenStep.review:
        _step = LobbyAndKeygenStep.lobby;
        notifyListeners();
        return true;
    }
  }

  // --- Keygen ceremony state ---
  //
  // Once `state.keygen.allAcked()` lands, we hand off to
  // `coord.startRemoteKeygen` and stream `KeyGenState` updates here.
  // Lifted onto the controller (rather than the accept-view's State)
  // so the FullscreenActionDialog body can react via
  // `ListenableBuilder(listenable: ctrl, ...)` — same pattern as the
  // local-keygen flow in `wallet_create.dart`.

  RemoteKeygenSessionHandle? _keygenSession;
  RemoteKeygenSessionHandle? get keygenSession => _keygenSession;

  KeyGenState? _keygenState;
  KeyGenState? get keygenState => _keygenState;

  StreamSubscription<KeyGenState>? _keygenStateSub;
  // Held alive so Dart-side GC doesn't collect the opaque subscription
  // and trigger Rust's `_stop()`.
  KeyGenStateBroadcastSubscription? _keygenBroadcastSub;

  bool _keygenStarting = false;

  /// Idempotent: kick off the ceremony once. Called by the accept-view
  /// when AllAcked + I'm in `acked`.
  Future<void> startKeygenCeremony() async {
    if (_keygenStarting || _keygenSession != null) return;
    _keygenStarting = true;
    try {
      final args = await handle.awaitKeygenReady();
      final session = await coord.startRemoteKeygen(args: args);
      _keygenSession = session;
      final broadcastSub = session.subState();
      _keygenBroadcastSub = broadcastSub;
      _keygenStateSub = broadcastSub.start().listen((state) {
        _keygenState = state;
        notifyListeners();
      });
      notifyListeners();
    } finally {
      _keygenStarting = false;
    }
  }

  @override
  void dispose() {
    _keygenStateSub?.cancel();
    _keygenBroadcastSub?.stop();
    _keygenSession?.cancel();
    _stateSub?.cancel();
    _stateBroadcastSub?.stop();
    deviceSetup.removeListener(notifyListeners);
    deviceSetup.dispose();
    super.dispose();
  }
}

// =============================================================================
// Page
// =============================================================================

class OrgKeygenPage extends StatefulWidget {
  const OrgKeygenPage({super.key, required this.nostrClient});

  final NostrClient nostrClient;

  @override
  State<OrgKeygenPage> createState() => _OrgKeygenPageState();
}

class _OrgKeygenPageState extends State<OrgKeygenPage> {
  late final _ConcreteController _ctrl;

  @override
  void initState() {
    super.initState();
    _ctrl = _ConcreteController(
      nostrClient: widget.nostrClient,
      nostrContextLookup: () => NostrContext.of(context),
    );
    _ctrl.addListener(_onUpdate);
  }

  void _onUpdate() {
    if (mounted) setState(() {});
  }

  Future<void> _submitName() async {
    final handle = await _ctrl.openLobbyAsHost();
    if (handle == null || !mounted) return;
    final settings = NostrContext.of(context).nostrSettings;
    // The lobby+keygen flow is shown over this dialog — same scaffolding
    // (`MaybeFullscreenDialog`), no back button. When it closes (success
    // or cancel), we pop our own dialog so the wizard exits cleanly.
    await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: LobbyAndKeygenPage(
        handle: handle,
        isHost: true,
        walletName: _ctrl.walletName,
        loadNsec: () async => settings.getNsec(),
      ),
    );
    if (mounted) Navigator.of(context).pop();
  }

  Future<void> _submitJoinLink() async {
    final handle = await _ctrl.openLobbyAsJoiner();
    if (handle == null || !mounted) return;
    final settings = NostrContext.of(context).nostrSettings;
    await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: LobbyAndKeygenPage(
        handle: handle,
        isHost: false,
        walletName: '', // joiner learns it via state.keyName
        loadNsec: () async => settings.getNsec(),
      ),
    );
    if (mounted) Navigator.of(context).pop();
  }

  @override
  void dispose() {
    _ctrl.removeListener(_onUpdate);
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _ctrl.back(context);
      },
      child: SafeArea(
        child: AnimatedSwitcher(
          duration: const Duration(milliseconds: 320),
          switchInCurve: Curves.easeOutCubic,
          switchOutCurve: Curves.easeInCubic,
          transitionBuilder: (child, animation) {
            // Outgoing step (animation reversed by AnimatedSwitcher) slides
            // out to the left; incoming slides in from the right. The fade
            // softens the cross-over so brief layout differences don't pop.
            final offset = Tween<Offset>(
              begin: const Offset(1.0, 0.0),
              end: Offset.zero,
            ).animate(animation);
            return SlideTransition(
              position: offset,
              child: FadeTransition(opacity: animation, child: child),
            );
          },
          layoutBuilder: (currentChild, previousChildren) {
            // Default stacks centered; we want top-aligned + stretched so
            // step layouts (which start with a header at the top) line up.
            //
            // `Positioned.fill` for the outgoing children: Stack sizes
            // itself to the currentChild's intrinsic size only (positioned
            // children are excluded from Stack sizing). Without this the
            // Stack sizes to the larger of in/outgoing, so when the
            // incoming step is shorter the dialog visibly shrinks once
            // the outgoing finishes animating away.
            return Stack(
              alignment: Alignment.topCenter,
              children: <Widget>[
                for (final child in previousChildren)
                  Positioned.fill(child: child),
                if (currentChild != null) currentChild,
              ],
            );
          },
          child: KeyedSubtree(
            key: ValueKey(_ctrl.step),
            child: _buildStep(context),
          ),
        ),
      ),
    );
  }

  Widget _buildStep(BuildContext context) {
    switch (_ctrl.step) {
      case OrgKeygenStep.walletType:
        return _WalletTypeView(ctrl: _ctrl);
      case OrgKeygenStep.sessionRole:
        return _SessionRoleView(ctrl: _ctrl);
      case OrgKeygenStep.joinSession:
        return _JoinSessionView(ctrl: _ctrl, onSubmit: _submitJoinLink);
      case OrgKeygenStep.nameWallet:
        return _NameView(ctrl: _ctrl, onSubmit: _submitName);
    }
  }
}

class _ConcreteController extends OrgKeygenController {
  _ConcreteController({required super.nostrClient, required this.nostrContextLookup});

  final NostrContext Function() nostrContextLookup;

  @override
  Future<String> _loadNsec() async {
    return nostrContextLookup().nostrSettings.getNsec();
  }
}

// =============================================================================
// Lobby + Keygen page (post-handle-acquisition)
// =============================================================================

/// The lobby/review/acceptKeygen flow. Pushed (via `pushReplacement`) by
/// [OrgKeygenPage] only after a [RemoteLobbyHandle] has been successfully
/// created. The handle is non-null in the controller — there is no path
/// that constructs this page without one.
///
/// Pops with `null` if the user cancels/leaves before finalize, or with
/// the resulting [AccessStructureRef] on a successful keygen.
class LobbyAndKeygenPage extends StatefulWidget {
  const LobbyAndKeygenPage({
    super.key,
    required this.handle,
    required this.isHost,
    required this.walletName,
    required this.loadNsec,
  });

  final RemoteLobbyHandle handle;
  final bool isHost;
  final String walletName;
  final Future<String> Function() loadNsec;

  @override
  State<LobbyAndKeygenPage> createState() => _LobbyAndKeygenPageState();
}

class _LobbyAndKeygenPageState extends State<LobbyAndKeygenPage> {
  late final LobbyAndKeygenController _ctrl;
  bool _reactedToCancel = false;
  bool _reactedToPendingKeygen = false;

  @override
  void initState() {
    super.initState();
    _ctrl = LobbyAndKeygenController(
      handle: widget.handle,
      isHost: widget.isHost,
      walletName: widget.walletName,
      loadNsec: widget.loadNsec,
    );
    _ctrl.addListener(_onUpdate);
    _ctrl.addListener(_watchForCancellation);
    _ctrl.addListener(_watchForPendingKeygen);
  }

  void _onUpdate() {
    if (mounted) setState(() {});
  }

  void _watchForCancellation() {
    if (_reactedToCancel) return;
    final state = _ctrl.lobbyState;
    if (state == null || !state.cancelled) return;
    _reactedToCancel = true;
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      if (!mounted) return;
      if (!_ctrl.isHost) {
        // Peer-initiated cancel: inform the user before kicking them
        // out of the page.
        await showDialog<void>(
          context: context,
          barrierDismissible: false,
          builder: (ctx) => AlertDialog(
            icon: const Icon(Icons.cancel_outlined),
            title: const Text('Lobby cancelled'),
            content: const Text(
              'The host cancelled this keygen session. '
              'You can start a new session or join a different invite.',
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(ctx).pop(),
                child: const Text('OK'),
              ),
            ],
          ),
        );
      }
      if (!mounted) return;
      Navigator.of(context).pop();
    });
  }

  void _watchForPendingKeygen() {
    if (_reactedToPendingKeygen) return;
    final state = _ctrl.lobbyState;
    final pending = state?.keygen;
    if (pending == null) return;
    if (!pending.includes(pubkey: _ctrl.myPubkey)) return;
    _reactedToPendingKeygen = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _ctrl.goToAcceptKeygen();
    });
  }

  Future<void> _declineKeygen() async {
    final confirm = await showDialog<bool>(
      context: context,
      builder: (ctx) {
        final theme = Theme.of(ctx);
        return AlertDialog(
          icon: Icon(Icons.cancel_outlined, color: theme.colorScheme.error),
          title: const Text('Decline this keygen?'),
          content: const Text(
            'Declining is final. If the host wants to try again they '
            'will have to start a new session.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(ctx).pop(false),
              child: const Text('Back'),
            ),
            FilledButton(
              style: FilledButton.styleFrom(
                backgroundColor: theme.colorScheme.error,
                foregroundColor: theme.colorScheme.onError,
              ),
              onPressed: () => Navigator.of(ctx).pop(true),
              child: const Text('Decline'),
            ),
          ],
        );
      },
    );
    if (confirm != true || !mounted) return;
    try {
      await _ctrl.leaveLobby();
    } catch (_) {
      // Best-effort: if the publish failed (no relay reachable), pop
      // the page directly so the user isn't stuck.
      if (mounted) Navigator.of(context).pop();
    }
  }

  @override
  void dispose() {
    _ctrl.removeListener(_onUpdate);
    _ctrl.removeListener(_watchForCancellation);
    _ctrl.removeListener(_watchForPendingKeygen);
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        // Within-page back only works for review→lobby. Anywhere else
        // the user must use the explicit Cancel/Leave footer button —
        // sneaking out via OS back skips the relay publish.
        _ctrl.back();
      },
      child: SafeArea(
        child: AnimatedSwitcher(
          duration: const Duration(milliseconds: 320),
          switchInCurve: Curves.easeOutCubic,
          switchOutCurve: Curves.easeInCubic,
          transitionBuilder: (child, animation) {
            final offset = Tween<Offset>(
              begin: const Offset(1.0, 0.0),
              end: Offset.zero,
            ).animate(animation);
            return SlideTransition(
              position: offset,
              child: FadeTransition(opacity: animation, child: child),
            );
          },
          layoutBuilder: (currentChild, previousChildren) {
            // See _OrgKeygenPageState.build for why we wrap previousChildren
            // in `Positioned.fill` instead of letting them sit naturally.
            return Stack(
              alignment: Alignment.topCenter,
              children: <Widget>[
                for (final child in previousChildren)
                  Positioned.fill(child: child),
                if (currentChild != null) currentChild,
              ],
            );
          },
          child: KeyedSubtree(
            key: ValueKey(_ctrl.step),
            child: _buildStep(context),
          ),
        ),
      ),
    );
  }

  Widget _buildStep(BuildContext context) {
    switch (_ctrl.step) {
      case LobbyAndKeygenStep.lobby:
        return _LobbyView(ctrl: _ctrl);
      case LobbyAndKeygenStep.review:
        return _ReviewView(ctrl: _ctrl);
      case LobbyAndKeygenStep.acceptKeygen:
        return _AcceptKeygenView(ctrl: _ctrl, onDecline: _declineKeygen);
    }
  }
}

// =============================================================================
// Step 1: wallet type
// =============================================================================

class _WalletTypeView extends StatelessWidget {
  const _WalletTypeView({required this.ctrl});
  final OrgKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: 'Who is this for?', onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
          child: Column(
            spacing: 12,
            children: [
              _ChoiceCard(
                icon: Icons.person_rounded,
                title: 'Just me',
                subtitle:
                    'A personal wallet. You visit your devices in person to sign.',
                onTap: () => ctrl.chosePersonal(context),
              ),
              _ChoiceCard(
                icon: Icons.groups_rounded,
                title: 'A group of us',
                subtitle:
                    'A shared wallet with other participants. You can each be in a different place.',
                emphasized: true,
                onTap: () async {
                  final ok = await ensureNostrIdentity(context);
                  if (!ok) return;
                  ctrl.choseOrganisation();
                },
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step 2: session role (start / join)
// =============================================================================

class _SessionRoleView extends StatelessWidget {
  const _SessionRoleView({required this.ctrl});
  final OrgKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: 'Start or join a session', onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
          child: Column(
            spacing: 12,
            children: [
              _ChoiceCard(
                icon: Icons.add_circle_outline_rounded,
                title: 'Start a new session',
                subtitle: 'Invite others to join a wallet you\'re creating.',
                emphasized: true,
                onTap: ctrl.chooseCreateSession,
              ),
              _ChoiceCard(
                icon: Icons.link_rounded,
                title: 'Join an existing session',
                subtitle: 'Accept an invite link from someone else.',
                onTap: ctrl.chooseJoinSession,
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step 3a: join session (participant)
// =============================================================================

class _JoinSessionView extends StatefulWidget {
  const _JoinSessionView({required this.ctrl, required this.onSubmit});
  final OrgKeygenController ctrl;
  final Future<void> Function() onSubmit;

  @override
  State<_JoinSessionView> createState() => _JoinSessionViewState();
}

class _JoinSessionViewState extends State<_JoinSessionView> {
  OrgKeygenController get ctrl => widget.ctrl;
  bool _attempted = false;
  bool _prefilled = false;
  final _focusNode = FocusNode();
  static const _prefix = 'frostsnap://keygen/';

  @override
  void initState() {
    super.initState();
    ctrl.joinLinkController.addListener(_onChanged);
    _focusNode.addListener(_onFocus);
  }

  @override
  void dispose() {
    ctrl.joinLinkController.removeListener(_onChanged);
    _focusNode.removeListener(_onFocus);
    _focusNode.dispose();
    super.dispose();
  }

  void _onChanged() {
    if (!mounted) return;
    // Always rebuild so the Join button's enabled state (which depends
    // on the text) tracks the controller. Clear `_attempted` on the
    // same tick so a prior "invalid link" error fades as the user
    // starts editing.
    setState(() {
      if (_attempted) _attempted = false;
    });
  }

  /// First time the field gains focus, drop the URL scheme in so the
  /// user only needs to paste/type the code.
  void _onFocus() {
    if (!_focusNode.hasFocus) return;
    if (_prefilled || ctrl.joinLinkController.text.isNotEmpty) return;
    _prefilled = true;
    ctrl.joinLinkController.text = _prefix;
    ctrl.joinLinkController.selection =
        TextSelection.collapsed(offset: _prefix.length);
  }

  void _trySubmit() {
    if (ctrl.joinLinkValid) {
      unawaited(widget.onSubmit());
    } else {
      setState(() => _attempted = true);
    }
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData('text/plain');
    if (data?.text != null) {
      ctrl.joinLinkController.text = data!.text!;
    }
  }

  Future<void> _scan() async {
    final scanned = await MaybeFullscreenDialog.show<String>(
      context: context,
      child: const QrStringScanner(title: 'Scan invite'),
    );
    if (!mounted || scanned == null) return;
    ctrl.joinLinkController.text = scanned.trim();
    // If what we scanned is a valid keygen link, submit immediately —
    // the user's intent for scanning is "I've got the invite, take me
    // there". If it's not valid, leave it in the field so the
    // errorText guides them.
    _trySubmit();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final errorText = (_attempted && !ctrl.joinLinkValid)
        ? 'Not a valid invite link'
        : ctrl.connectError;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: 'Join session', onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
          child: Card.outlined(
            color: theme.colorScheme.surfaceContainerHigh,
            margin: EdgeInsets.zero,
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  TextField(
                    autofocus: true,
                    focusNode: _focusNode,
                    controller: ctrl.joinLinkController,
                    decoration: InputDecoration(
                      filled: false,
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(8),
                        borderSide: BorderSide.none,
                      ),
                      hintText: _prefix,
                      errorText: errorText,
                      errorMaxLines: 2,
                    ),
                    onSubmitted: (_) => _trySubmit(),
                  ),
                  const SizedBox(height: 4),
                  Row(
                    children: [
                      TextButton.icon(
                        onPressed: _paste,
                        icon: const Icon(Icons.paste),
                        label: const Text('Paste'),
                      ),
                      TextButton.icon(
                        onPressed: _scan,
                        icon: const Icon(Icons.qr_code_scanner_rounded),
                        label: const Text('Scan'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Align(
            alignment: Alignment.centerRight,
            child: FilledButton.icon(
              icon: ctrl.connecting
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.arrow_forward_rounded),
              iconAlignment: IconAlignment.end,
              onPressed: (ctrl.connecting ||
                      ctrl.joinLinkController.text.trim().isEmpty)
                  ? null
                  : _trySubmit,
              label: const Text('Join'),
            ),
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step 3b: name wallet (host)
// =============================================================================

class _NameView extends StatelessWidget {
  const _NameView({required this.ctrl, required this.onSubmit});
  final OrgKeygenController ctrl;
  final Future<void> Function() onSubmit;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final devMode =
        SettingsContext.of(context)?.settings.isInDeveloperMode() ?? false;
    final canSubmit = ctrl.nameValid && !ctrl.connecting;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: 'Name this wallet', onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
          child: Text(
            'All wallet participants will see this name.',
            style: theme.textTheme.bodyMedium
                ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
          child: TextField(
            autofocus: true,
            controller: ctrl.nameController,
            decoration: InputDecoration(
              border: const OutlineInputBorder(),
              hintText: 'e.g. Acme Treasury',
              errorText: ctrl.connectError,
              errorMaxLines: 2,
            ),
            maxLength: 15,
            textCapitalization: TextCapitalization.words,
            onChanged: (_) => (ctrl as _ConcreteController).bump(),
            onSubmitted: (_) {
              if (canSubmit) unawaited(onSubmit());
            },
          ),
        ),
        if (devMode)
          NetworkAdvancedOptions(
            selected: ctrl.network,
            onChanged: ctrl.setNetwork,
          ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Align(
            alignment: Alignment.centerRight,
            child: FilledButton.icon(
              onPressed: canSubmit ? () => unawaited(onSubmit()) : null,
              icon: ctrl.connecting
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.arrow_forward_rounded),
              iconAlignment: IconAlignment.end,
              label: const Text('Next'),
            ),
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step 4: lobby
// =============================================================================

class _LobbyView extends StatelessWidget {
  const _LobbyView({required this.ctrl});
  final LobbyAndKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final state = ctrl.lobbyState;
    // Until the NIP-28 ChannelCreation event lands, the lobby has no
    // known initiator and rendering it would be misleading (host
    // missing, participant counts wrong). Show a spinner instead.
    final channelReady = state != null && state.initiator != null;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // No back button: the only valid exits from the lobby are the
        // explicit Cancel/Leave footer actions (which publish the
        // appropriate abort to the relay). Sneaking out via OS back
        // skips the publish and other participants would never know.
        _Header(title: state?.keyName ?? ctrl.walletName),
        if (channelReady)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
            child: Text(
              'Add your devices while you wait for others to join.',
              style: theme.textTheme.bodyMedium
                  ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
            ),
          ),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            shrinkWrap: true,
            children: [
              // Local participant saw a `StartKeygen` arrive but their
              // pubkey isn't in the selected set — surface a terminal
              // banner so the user can pop the page rather than sitting
              // on a stale lobby view. Inclusion is derived from
              // `pending_keygen.includes(myPubkey)` rather than a
              // separate latched flag on `LobbyState`.
              if (state != null &&
                  state.keygen != null &&
                  !state.keygen!.includes(pubkey: ctrl.myPubkey))
                Card.filled(
                  color: theme.colorScheme.surfaceContainerHighest,
                  child: ListTile(
                    leading: Icon(Icons.info_outline_rounded,
                        color: theme.colorScheme.onSurfaceVariant),
                    title: Text(
                      'This round started without you',
                      style: theme.textTheme.titleSmall,
                    ),
                    subtitle: Text(
                      'The host chose a different set of participants. '
                      'You can close this lobby — there\'s nothing more to do here.',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                    trailing: TextButton(
                      onPressed: () => Navigator.of(context).pop(),
                      child: const Text('Close'),
                    ),
                  ),
                ),
              // `state.cancelled == true` is now handled at the page
              // level by `_watchForCancellation` (dialog + pop), so
              // no inline banner is needed — the user will be looking
              // at the dialog within one frame of the state flip.
              if (!channelReady)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 48),
                  child: Column(
                    children: [
                      const CircularProgressIndicator(),
                      const SizedBox(height: 16),
                      Text('Connecting to relay…',
                          style: theme.textTheme.bodyMedium?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant)),
                    ],
                  ),
                )
              else ...[
                Row(
                  children: [
                    Expanded(
                        child: Text('Participants',
                            style: theme.textTheme.labelLarge)),
                    Text(
                      state.allReady()
                          ? 'All ready'
                          : '${state.participants.values.where((p) => p.status != ParticipantStatus.joining).length} of ${state.participants.length} ready',
                      style: theme.textTheme.labelLarge,
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                ..._participantRows(ctrl: ctrl, state: state, readOnly: false),
                const SizedBox(height: 12),
                if (ctrl.isHost)
                  _InviteTile(
                    onTap: () => _showInviteDialog(context, ctrl.handle),
                  ),
              ],
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            children: [
              // Host: red "Cancel lobby" action. Publishes `CancelLobby`
              // (awaits relay OK + local apply via `dispatch`), then the
              // page pops on the resulting `state.cancelled = true`
              // transition. Joiners see the same state change and get a
              // dialog + pop via `_watchForCancellation`.
              //
              // Joiner: red "Leave lobby" action. Publishes `Leave`,
              // then pops the page directly (Leave doesn't flip
              // `state.cancelled` for non-selected participants, so we
              // can't rely on the cancellation watcher).
              AsyncActionButton(
                onPressed: ctrl.isHost
                    ? ctrl.cancelLobby
                    : () async {
                        await ctrl.leaveLobby();
                        if (context.mounted) Navigator.of(context).pop();
                      },
                style: FilledButton.styleFrom(
                  backgroundColor: theme.colorScheme.error,
                  foregroundColor: theme.colorScheme.onError,
                ),
                child: Text(ctrl.isHost ? 'Cancel lobby' : 'Leave lobby'),
              ),
              const Spacer(),
              _LobbyPrimaryButton(ctrl: ctrl),
            ],
          ),
        ),
      ],
    );
  }
}

class _LobbyPrimaryButton extends StatelessWidget {
  const _LobbyPrimaryButton({required this.ctrl});
  final LobbyAndKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final state = ctrl.lobbyState;
    if (state == null || state.initiator == null) {
      return const FilledButton(onPressed: null, child: Text('Connecting…'));
    }
    if (!ctrl.meIsReady) {
      return FilledButton.icon(
        icon: const Icon(Icons.add_rounded),
        label: const Text('Add your devices'),
        onPressed: () => _showDeviceSetupDialog(context, ctrl),
      );
    }
    if (!state.allReady()) {
      return const FilledButton(onPressed: null, child: Text('Waiting for participants'));
    }
    if (ctrl.totalDevices < 2) {
      return const FilledButton(onPressed: null, child: Text('Need at least 2 devices total'));
    }
    if (!ctrl.isHost) {
      return const _WaitingForHostStatus();
    }
    return FilledButton.icon(
      icon: const Icon(Icons.arrow_forward_rounded),
      iconAlignment: IconAlignment.end,
      onPressed: ctrl.goToReview,
      label: Text('Continue with ${ctrl.totalDevices} devices'),
    );
  }
}

class _ParticipantRow extends StatefulWidget {
  const _ParticipantRow({
    required this.ctrl,
    required this.participant,
    required this.isMe,
    required this.isInitiator,
    required this.keyOffset,
    this.readOnly = false,
    this.trailingOverride,
  });

  final LobbyAndKeygenController ctrl;
  final ParticipantInfo participant;
  final bool isMe;
  /// Whether this participant is the host who created the lobby.
  /// Computed by the parent (`_participantRows`) by comparing
  /// `participant.pubkey` against `state.initiator`.
  final bool isInitiator;
  /// The key-number of this participant's first device in the global
  /// (per-lobby) numbering — computed by the parent so device rows can
  /// show "Key #N" consistently.
  final int keyOffset;
  /// In review/readonly mode, the trailing slot is a phase-aware label
  /// instead of an edit icon, and the row starts expanded.
  final bool readOnly;
  /// If provided, replaces the entire status portion of the trailing
  /// slot (e.g. the Ready/Joining pill or the ack-status indicator on
  /// the accept screen). The expand chevron is still appended after.
  final Widget? trailingOverride;

  @override
  State<_ParticipantRow> createState() => _ParticipantRowState();
}

class _ParticipantRowState extends State<_ParticipantRow> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final p = widget.participant;
    final isReady = p.status == ParticipantStatus.ready;
    final excluded = widget.ctrl.isExcluded(p.pubkey);
    // Host-only exclusion toggle: only meaningful for Ready participants
    // who aren't the host themselves. Lives at the start of the trailing
    // slot, before whichever status/action widgets the row renders.
    final showExclusionToggle =
        widget.ctrl.isHost && !widget.isMe && isReady;
    final exclusionToggle = showExclusionToggle
        ? Tooltip(
            message: excluded ? 'Include in keygen' : 'Exclude from keygen',
            child: Checkbox(
              value: !excluded,
              onChanged: (v) => widget.ctrl.setIncluded(p.pubkey, v ?? true),
              visualDensity: VisualDensity.compact,
            ),
          )
        : null;

    final Widget trailing;
    if (widget.trailingOverride != null) {
      trailing = Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 8,
        children: [
          widget.trailingOverride!,
          AnimatedRotation(
            turns: _expanded ? 0.5 : 0.0,
            duration: Durations.short3,
            child: Icon(Icons.keyboard_arrow_down_rounded,
                color: theme.colorScheme.onSurfaceVariant),
          ),
        ],
      );
    } else if (widget.readOnly) {
      trailing = Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 8,
        children: [
          _reviewStatusLabel(context, p),
          AnimatedRotation(
            turns: _expanded ? 0.5 : 0.0,
            duration: Durations.short3,
            child: Icon(Icons.keyboard_arrow_down_rounded,
                color: theme.colorScheme.onSurfaceVariant),
          ),
        ],
      );
    } else {
      // Trailing slot fills with whichever control applies — the edit
      // button (own row, ready) or the inclusion checkbox (host view of
      // another ready participant). They're mutually exclusive, and
      // sharing the same fixed 36x36 slot keeps every row's right edge
      // aligned regardless of which control renders.
      final Widget? trailingAction;
      if (widget.isMe && isReady) {
        trailingAction = IconButton(
          icon: const Icon(Icons.edit_rounded, size: 18),
          tooltip: 'Edit your devices',
          visualDensity: VisualDensity.compact,
          padding: EdgeInsets.zero,
          color: theme.colorScheme.onSurfaceVariant,
          onPressed: () => _showDeviceSetupDialog(context, widget.ctrl),
        );
      } else {
        trailingAction = exclusionToggle;
      }

      final statusLabel = switch (p.status) {
        ParticipantStatus.joining => Text(
            widget.isMe ? 'Waiting for you' : 'Joined',
            style: theme.textTheme.bodySmall
                ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
          ),
        ParticipantStatus.ready => Row(
            mainAxisSize: MainAxisSize.min,
            spacing: 4,
            children: [
              Text('Ready',
                  style: theme.textTheme.labelMedium
                      ?.copyWith(color: Colors.green)),
              const Icon(Icons.verified_rounded,
                  size: 18, color: Colors.green),
            ],
          ),
      };

      trailing = Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 4,
        children: [
          statusLabel,
          SizedBox(
            width: 36,
            height: 36,
            child: trailingAction ?? const SizedBox.shrink(),
          ),
        ],
      );
    }

    return AnimatedOpacity(
      duration: Durations.short3,
      opacity: excluded ? 0.55 : 1.0,
      child: Card.filled(
      margin: const EdgeInsets.symmetric(vertical: 3),
      color: theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            leading: Stack(
              clipBehavior: Clip.none,
              children: [
                CircleAvatar(
                  backgroundColor: widget.isMe
                      ? theme.colorScheme.surfaceContainerHighest
                      : theme.colorScheme.secondaryContainer,
                  child: Icon(
                    widget.isMe
                        ? Icons.person_rounded
                        : Icons.person_outline_rounded,
                    color: widget.isMe
                        ? theme.colorScheme.onSurfaceVariant
                        : theme.colorScheme.onSecondaryContainer,
                    size: 20,
                  ),
                ),
                if (widget.isInitiator)
                  Positioned(
                    right: -2,
                    bottom: -2,
                    child: Tooltip(
                      message: 'Host',
                      child: Container(
                        padding: const EdgeInsets.all(2),
                        decoration: BoxDecoration(
                          color: theme.colorScheme.surfaceContainerHigh,
                          shape: BoxShape.circle,
                        ),
                        child: const Icon(Icons.star_rounded,
                            size: 14, color: Color(0xFFFFC107)),
                      ),
                    ),
                  ),
              ],
            ),
            title: Text(
              widget.isMe ? 'You' : _shortPubkey(p.pubkey),
              style: theme.textTheme.titleSmall,
              overflow: TextOverflow.ellipsis,
            ),
            subtitle: Text(
              isReady
                  ? '${p.devices.length} ${p.devices.length == 1 ? "device" : "devices"}'
                  : '',
              style: theme.textTheme.bodySmall
                  ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
            ),
            trailing: trailing,
            onTap: (isReady && p.devices.isNotEmpty)
                ? () => setState(() => _expanded = !_expanded)
                : null,
          ),
          AnimatedCrossFade(
            duration: Durations.short4,
            crossFadeState: (isReady && _expanded)
                ? CrossFadeState.showSecond
                : CrossFadeState.showFirst,
            firstChild: const SizedBox(width: double.infinity, height: 0),
            secondChild: _DeviceList(
              devices: p.devices,
              keyOffset: widget.keyOffset,
            ),
          ),
        ],
      ),
      ),
    );
  }

  /// Status label for the (host-only) review step. Since threshold
  /// no longer has its own negotiation round-trip, any Ready
  /// participant is green "Ready"; anyone still Joining is muted.
  Widget _reviewStatusLabel(BuildContext context, ParticipantInfo p) {
    final theme = Theme.of(context);
    if (p.status == ParticipantStatus.ready) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 4,
        children: [
          Text('Ready',
              style: theme.textTheme.labelMedium
                  ?.copyWith(color: Colors.green)),
          const Icon(Icons.verified_rounded,
              size: 18, color: Colors.green),
        ],
      );
    }
    return Text('Joining',
        style: theme.textTheme.bodySmall
            ?.copyWith(color: theme.colorScheme.onSurfaceVariant));
  }
}

class _DeviceList extends StatelessWidget {
  const _DeviceList({required this.devices, required this.keyOffset});
  final List<DeviceRegistration> devices;
  final int keyOffset;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      width: double.infinity,
      color: theme.colorScheme.surfaceContainerHighest,
      padding: const EdgeInsets.fromLTRB(72, 4, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (int i = 0; i < devices.length; i++)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 4),
              child: Row(
                children: [
                  Icon(Icons.key,
                      size: 16, color: theme.colorScheme.onSurfaceVariant),
                  const SizedBox(width: 8),
                  Text('Key #${keyOffset + i}',
                      style: theme.textTheme.bodyMedium?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant)),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(devices[i].name,
                        style: theme.textTheme.bodyMedium,
                        overflow: TextOverflow.ellipsis),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

String _shortPubkey(PublicKey pk) {
  final hex = pk.toHex();
  return hex.substring(0, 8);
}

/// Build `_ParticipantRow` widgets with cumulative key-number offsets
/// so each participant's device list can render "Key #N" in a single
/// global numbering (Key #1 is the first device across all participants).
List<Widget> _participantRows({
  required LobbyAndKeygenController ctrl,
  required LobbyState state,
  required bool readOnly,
}) {
  final rows = <Widget>[];
  int keyNumber = 1;
  for (final p in state.participants.values) {
    final offset = keyNumber;
    keyNumber += p.devices.length;
    final isInitiator =
        state.initiator != null && state.initiator! == p.pubkey;
    rows.add(
      _ParticipantRow(
        ctrl: ctrl,
        participant: p,
        isMe: p.pubkey == ctrl.myPubkey,
        isInitiator: isInitiator,
        keyOffset: offset,
        readOnly: readOnly,
      ),
    );
  }
  return rows;
}

class _InviteTile extends StatelessWidget {
  const _InviteTile({required this.onTap});
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: Colors.transparent,
      borderRadius: BorderRadius.circular(12),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: onTap,
        child: CustomPaint(
          painter: _DashedBorderPainter(
            color: theme.colorScheme.outline,
            radius: 12,
          ),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 16),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(Icons.person_add_rounded,
                    size: 20, color: theme.colorScheme.primary),
                const SizedBox(width: 10),
                Text('Invite participants',
                    style: theme.textTheme.titleSmall?.copyWith(
                        color: theme.colorScheme.primary,
                        fontWeight: FontWeight.w600)),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _DashedBorderPainter extends CustomPainter {
  const _DashedBorderPainter({required this.color, this.radius = 12});

  final Color color;
  final double radius;

  static const double _dash = 6;
  static const double _gap = 4;
  static const double _stroke = 1.5;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..strokeWidth = _stroke
      ..style = PaintingStyle.stroke;

    final rrect = RRect.fromRectAndRadius(
      Offset.zero & size,
      Radius.circular(radius),
    );
    final path = Path()..addRRect(rrect);

    for (final metric in path.computeMetrics()) {
      double distance = 0;
      while (distance < metric.length) {
        final end = distance + _dash;
        canvas.drawPath(
          metric.extractPath(distance, end.clamp(0, metric.length)),
          paint,
        );
        distance = end + _gap;
      }
    }
  }

  @override
  bool shouldRepaint(covariant _DashedBorderPainter old) =>
      old.color != color || old.radius != radius;
}

void _showInviteDialog(BuildContext context, RemoteLobbyHandle handle) {
  MaybeFullscreenDialog.show<void>(
    context: context,
    barrierDismissible: true,
    child: _InviteDialog(inviteLink: handle.inviteLink()),
  );
}

class _InviteDialog extends StatelessWidget {
  const _InviteDialog({required this.inviteLink});
  final String inviteLink;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return FullscreenDialogScaffold(
      title: const Text('Invite participants'),
      body: SliverList.list(
        children: [
          Center(
            child: Container(
              width: 220,
              height: 220,
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.white,
                borderRadius: BorderRadius.circular(16),
              ),
              child: PrettyQrView.data(
                data: inviteLink,
                decoration: const PrettyQrDecoration(
                  shape: PrettyQrSmoothSymbol(),
                ),
              ),
            ),
          ),
          const SizedBox(height: 16),
          SelectableText(
            inviteLink,
            textAlign: TextAlign.center,
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
      footer: Row(
        spacing: 12,
        children: [
          Expanded(
            child: FilledButton.tonalIcon(
              icon: const Icon(Icons.copy_rounded, size: 18),
              label: const Text('Copy'),
              onPressed: () async {
                await Clipboard.setData(ClipboardData(text: inviteLink));
                if (context.mounted) {
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(content: Text('Copied')),
                  );
                }
              },
            ),
          ),
          Expanded(
            child: FilledButton.tonalIcon(
              icon: const Icon(Icons.share_rounded, size: 18),
              label: const Text('Share invite'),
              onPressed: () {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(
                    content: Text('Share not wired up yet'),
                    duration: Duration(seconds: 2),
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

// =============================================================================
// Device setup dialog
// =============================================================================

void _showDeviceSetupDialog(BuildContext context, LobbyAndKeygenController ctrl) {
  MaybeFullscreenDialog.show<void>(
    context: context,
    barrierDismissible: false,
    child: _DeviceSetupDialog(ctrl: ctrl),
  );
}

class _DeviceSetupDialog extends StatefulWidget {
  const _DeviceSetupDialog({required this.ctrl});
  final LobbyAndKeygenController ctrl;

  @override
  State<_DeviceSetupDialog> createState() => _DeviceSetupDialogState();
}

class _DeviceSetupDialogState extends State<_DeviceSetupDialog> {
  // The controller is owned by `OrgKeygenController` so typed names
  // and name previews survive dialog close/reopen. We only subscribe
  // here for rebuilds — ownership (including dispose) stays upstream.
  DeviceSetupController get _setup => widget.ctrl.deviceSetup;

  bool _submitting = false;
  String? _submitError;

  @override
  void initState() {
    super.initState();
    _setup.addListener(_onChanged);
  }

  @override
  void dispose() {
    _setup.removeListener(_onChanged);
    super.dispose();
  }

  void _onChanged() {
    if (mounted) setState(() {});
  }

  Future<void> _submit() async {
    setState(() {
      _submitting = true;
      _submitError = null;
    });
    try {
      // Snapshot synchronously before the async gap so a device
      // plug/unplug mid-`markReady` can't desync the list from the
      // names we just validated via `_setup.ready`.
      final devices = _setup.devices
          .map((d) => (id: d.id, name: _setup.deviceNames[d.id]!))
          .toList();
      await widget.ctrl.markReady(devices);
      if (!mounted) return;
      Navigator.of(context).pop();
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _submitting = false;
        _submitError = '$e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final count = _setup.connectedDeviceCount;

    final errorBanner = _submitError == null
        ? null
        : SliverToBoxAdapter(
            child: Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Card.filled(
                margin: EdgeInsets.zero,
                color: theme.colorScheme.errorContainer,
                child: ListTile(
                  leading: Icon(
                    Icons.error_outline,
                    color: theme.colorScheme.onErrorContainer,
                  ),
                  title: Text(
                    _submitError!,
                    style: TextStyle(
                      color: theme.colorScheme.onErrorContainer,
                    ),
                  ),
                ),
              ),
            ),
          );

    return FullscreenDialogScaffold(
      title: const Text('Add your devices'),
      subtitle: 'Each device you add will hold one key in the wallet.',
      leading: IconButton(
        icon: const Icon(Icons.arrow_back_rounded),
        onPressed: () => Navigator.of(context).pop(),
        tooltip: 'Back',
      ),
      body: MultiSliver(
        children: [
          if (errorBanner != null) errorBanner,
          DeviceSetupView(
            controller: _setup,
            onSubmitted: () {
              if (_setup.ready && !_submitting) _submit();
            },
          ),
        ],
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          onPressed: (_setup.ready && !_submitting) ? _submit : null,
          icon: _submitting
              ? const SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.arrow_forward_rounded),
          iconAlignment: IconAlignment.end,
          label: Text(
            'Continue with $count ${count == 1 ? "device" : "devices"}',
          ),
        ),
      ),
    );
  }
}

// =============================================================================
// Step 5: review
// =============================================================================

class _ReviewView extends StatelessWidget {
  const _ReviewView({required this.ctrl});
  final LobbyAndKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final state = ctrl.lobbyState;
    final total = ctrl.totalDevices;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: 'Choose threshold', onBack: () => ctrl.back()),
        const SizedBox(height: 12),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            shrinkWrap: true,
            children: [
              IgnorePointer(
                ignoring: !ctrl.isHost,
                child: Opacity(
                  opacity: ctrl.isHost ? 1.0 : 0.75,
                  child: ThresholdSelector(
                    threshold: ctrl.displayThreshold.clamp(1, max(total, 1)),
                    totalDevices: max(total, 1),
                    recommendedThreshold: ctrl.recommendedThreshold,
                    onChanged: (v) => ctrl.setPendingThreshold(v),
                  ),
                ),
              ),
              const SizedBox(height: 16),
              if (state != null)
                ..._participantRows(ctrl: ctrl, state: state, readOnly: true),
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Align(
            alignment: Alignment.centerRight,
            child: _ReviewPrimaryButton(ctrl: ctrl),
          ),
        ),
      ],
    );
  }
}

class _ReviewPrimaryButton extends StatelessWidget {
  const _ReviewPrimaryButton({required this.ctrl});
  final LobbyAndKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    // Host-only screen now: the review step is never reached by
    // joiners (they see the accept-modal triggered by
    // `state.keygen`). The only action here is terminal —
    // publishing `StartKeygen`.
    final state = ctrl.lobbyState;
    if (state == null) {
      return const FilledButton(onPressed: null, child: Text('Connecting…'));
    }
    if (!state.allReady()) {
      return const FilledButton(
        onPressed: null,
        child: Text('Waiting for participants'),
      );
    }
    return AsyncActionButton(
      onPressed: ctrl.startKeygen,
      child: const Text('Generate keys'),
    );
  }
}

// =============================================================================
// Step 6: accept keygen (joiner)
// =============================================================================

class _AcceptKeygenView extends StatelessWidget {
  const _AcceptKeygenView({
    required this.ctrl,
    required this.onDecline,
  });
  final LobbyAndKeygenController ctrl;

  /// Mode A's terminal "Decline" action — routes through a confirm
  /// dialog because the action is final.
  final Future<void> Function() onDecline;

  @override
  Widget build(BuildContext context) {
    final state = ctrl.lobbyState;
    final pending = state?.keygen;
    final me = ctrl.myPubkey;

    if (pending == null) {
      // Brief gap between step flip and the post-frame state arriving.
      return const Center(child: CircularProgressIndicator());
    }

    final iAmAcked = pending.acked.any((pk) => pk == me);

    return iAmAcked
        ? _AcceptKeygenWaitingView(
            ctrl: ctrl,
            pending: pending,
          )
        : _AcceptKeygenDecisionView(
            ctrl: ctrl,
            pending: pending,
            onDecline: onDecline,
          );
  }
}

/// Mode A: pre-ack. Wallet + threshold + network info, participant
/// list, Decline + Accept footer. The "declining is final" disclosure
/// surfaces in a confirm dialog when Decline is tapped.
class _AcceptKeygenDecisionView extends StatelessWidget {
  const _AcceptKeygenDecisionView({
    required this.ctrl,
    required this.pending,
    required this.onDecline,
  });
  final LobbyAndKeygenController ctrl;
  final ResolvedKeygen pending;
  final Future<void> Function() onDecline;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final state = ctrl.lobbyState;
    final keyName = state?.keyName ?? '';
    final purpose = state?.purpose;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _Header(title: 'Generate this key?'),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
            shrinkWrap: true,
            children: [
              _AcceptInfoRow(
                icon: Icons.account_balance_wallet_rounded,
                label: 'Wallet',
                value: keyName,
              ),
              const SizedBox(height: 12),
              _AcceptInfoRow(
                icon: Icons.security_rounded,
                label: 'Threshold',
                value:
                    '${pending.threshold} of ${pending.participants.length} required to spend',
              ),
              Builder(
                builder: (context) {
                  final network = purpose?.bitcoinNetwork();
                  if (network == null || network.isMainnet()) {
                    return const SizedBox.shrink();
                  }
                  return Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: _AcceptInfoRow(
                      icon: Icons.dns_rounded,
                      label: 'Network',
                      value: network.name(),
                      valueColor: theme.colorScheme.error,
                    ),
                  );
                },
              ),
              const SizedBox(height: 24),
              Text(
                'Participants',
                style: theme.textTheme.labelLarge,
              ),
              const SizedBox(height: 8),
              if (state != null)
                ..._ackParticipantRows(
                  ctrl: ctrl,
                  state: state,
                  pending: pending,
                ),
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            children: [
              TextButton(
                onPressed: () => onDecline(),
                style: TextButton.styleFrom(
                  foregroundColor: theme.colorScheme.error,
                ),
                child: const Text('Decline'),
              ),
              const Spacer(),
              AsyncActionButton(
                onPressed: ctrl.ackKeygen,
                icon: Icons.arrow_forward_rounded,
                child: const Text('Accept'),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// Mode B: post-ack. Compact N-of-M header, per-participant ack
/// status list, single "Cancel keygen" action.
///
/// Kicks off the ceremony in `initState` by awaiting
/// `lobby.awaitKeygenReady()` (which only resolves once `AllAcked` lands),
/// then calling `coord.startRemoteKeygen` to obtain a
/// [RemoteKeygenSessionHandle]. All subsequent control — subscribing to
/// state, confirming the code match, cancelling — goes through that handle.
/// Cancellation paths (user cancel, page-pop, code-match decline, local
/// device disconnect) all converge on a single Rust-side cleanup driven by
/// the spawned ceremony task.
class _AcceptKeygenWaitingView extends StatefulWidget {
  const _AcceptKeygenWaitingView({
    required this.ctrl,
    required this.pending,
  });
  final LobbyAndKeygenController ctrl;
  final ResolvedKeygen pending;

  @override
  State<_AcceptKeygenWaitingView> createState() =>
      _AcceptKeygenWaitingViewState();
}

/// Sub-phases of `_AcceptKeygenWaitingView` after the user has acked.
///
/// - `awaiting`: pre-allAcks; the FullscreenActionDialog is in charge of the
///   per-device ack progress overlay. Body shows the "Waiting on keygen"
///   shell.
/// - `verify`: allAcks landed and the fullscreen dialog has dismissed.
///   We now require the user to verify the security code with **every other
///   participant** out-of-band before finalizing — per-participant checklist.
///
/// `state.finished` causes the page to pop directly (same as the local
/// keygen flow) — no intermediate "wallet created" screen.
enum _AcceptPhase { awaiting, verify }

class _AcceptKeygenWaitingViewState extends State<_AcceptKeygenWaitingView> {
  /// The session handle the ceremony task lives behind. Held until either
  /// finalize succeeds, the user cancels, or the page is disposed — at
  /// which point the handle's `Drop` (or explicit `cancel`) tears down the
  /// run loop centrally.
  KeyGenState? get _kgState => widget.ctrl.keygenState;
  RemoteKeygenSessionHandle? get _session => widget.ctrl.keygenSession;

  /// Fullscreen dialog showing the security code + per-device ack progress.
  /// Constructed once we know our local devices (from `widget.pending`).
  /// Auto-dismisses when all `_localDevices` have been removed via
  /// `removeActionNeeded` as their session-acks land. Same shape as
  /// `wallet_create.dart` uses for local keygen.
  FullscreenActionDialogController? _fullscreenController;

  /// Cached so `LargeCircularProgressIndicator.progress` and the
  /// `removeActionNeeded` forwarding both look at the same set.
  late final List<DeviceId> _localDevices = _computeLocalDevices();

  /// Sub-phase. Drives `build` and the `_onCtrlChanged` transitions.
  /// Acts as the single latch — no more separate `_confirmShown` flag.
  _AcceptPhase _phase = _AcceptPhase.awaiting;

  /// Latched true once we've initiated a page-pop. Subsequent state
  /// changes (e.g. a finished+aborted race during teardown) are ignored —
  /// without this we'd hit `Navigator.pop` while the navigator is locked
  /// from the previous pop and Flutter asserts.
  bool _popped = false;

  /// Tracks which session_acks we've already forwarded to the fullscreen
  /// controller, since the state's `sessionAcks` list grows monotonically
  /// and we'd otherwise call `removeActionNeeded` for the same device on
  /// every rebuild.
  final Set<DeviceId> _ackedForwardedToDialog = <DeviceId>{};

  /// Pubkeys of *other* participants the user has ticked off as
  /// out-of-band-verified. Continue is enabled when this contains every
  /// non-self participant.
  final Set<PublicKey> _verified = <PublicKey>{};

  /// True while `confirmMatch` is awaiting. Disables Continue and shows
  /// a spinner. The page-pop happens via `state.finished` in
  /// `_onCtrlChanged`, not inline after the await — so a parallel
  /// `state.aborted` (e.g. local-device disconnect) can still pop.
  bool _confirming = false;

  /// In-flight transition to the `verify` phase. Set true while we're
  /// `await`ing the fullscreen dialog's dismissal animation, so a
  /// re-fired `_onCtrlChanged` mid-await doesn't kick off a second
  /// transition.
  bool _verifyTransitionInFlight = false;

  @override
  void initState() {
    super.initState();
    // The controller fires `notifyListeners` whenever its `keygenState`
    // updates (among other things). Hook the side effects — show the
    // fullscreen dialog, forward acks, fire the code-match dialog, pop
    // on finished/aborted — off that single listener.
    widget.ctrl.addListener(_onCtrlChanged);
    unawaited(widget.ctrl.startKeygenCeremony());
  }

  /// Compute the device IDs registered to *this* participant — the set the
  /// FullscreenActionDialog tracks for ack progress.
  List<DeviceId> _computeLocalDevices() {
    final me = widget.ctrl.myPubkey;
    for (final p in widget.pending.participants) {
      if (p.pubkey == me) {
        return p.devices.map((d) => d.deviceId).toList();
      }
    }
    return const [];
  }

  void _onCtrlChanged() {
    if (!mounted || _popped) return;
    final state = _kgState;
    if (state == null) return;

    // Lazily build the fullscreen dialog the first time we have a state
    // to display. Building in initState would risk pushing a route
    // mid-build; the controller fires AFTER startKeygenCeremony's first
    // emit so this runs out-of-build.
    _fullscreenController ??= _buildFullscreenController(context);

    // Forward each new ack to the fullscreen dialog so its progress
    // indicator advances and it auto-dismisses once every local device
    // has acked.
    final controller = _fullscreenController;
    if (controller != null) {
      for (final id in state.sessionAcks) {
        if (_ackedForwardedToDialog.add(id)) {
          unawaited(controller.removeActionNeeded(id));
        }
      }
    }

    if (state.aborted != null) {
      _popped = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Navigator.of(context).pop();
      });
      return;
    }

    if (state.finished != null) {
      _popped = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Navigator.of(context).pop(state.finished);
      });
      return;
    }

    // All our local devices have acked → wait for the fullscreen dialog
    // to dismiss, then transition to the verify-out-of-band sub-view.
    if (state.allAcks &&
        _phase == _AcceptPhase.awaiting &&
        !_verifyTransitionInFlight) {
      _verifyTransitionInFlight = true;
      unawaited(_transitionToVerify());
    }
  }

  Future<void> _transitionToVerify() async {
    await _fullscreenController?.awaitDismissed();
    if (!mounted || _popped) return;
    setState(() => _phase = _AcceptPhase.verify);
  }

  Future<void> _confirmAndFinalize() async {
    final session = _session;
    if (session == null || _confirming) return;
    setState(() => _confirming = true);
    try {
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      // Don't pop here — `state.finished` from the stream flips _phase
      // to `created` in _onCtrlChanged. The user then dismisses via Done.
      await session.confirmMatch(encryptionKey: encryptionKey);
    } catch (e) {
      if (!mounted) return;
      setState(() => _confirming = false);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Finalize failed: $e')),
      );
    }
  }

  /// Build the fullscreen "Security Check" dialog. The body and the
  /// progress button both use `ListenableBuilder` so they rebuild on
  /// every controller `notifyListeners` (which fires whenever
  /// `keygenState` updates) — same pattern as `wallet_create.dart`.
  FullscreenActionDialogController _buildFullscreenController(
    BuildContext context,
  ) {
    return FullscreenActionDialogController(
      context: context,
      devices: _localDevices,
      title: 'Security Check',
      body: (context) => ListenableBuilder(
        listenable: widget.ctrl,
        builder: (context, _) {
          final theme = Theme.of(context);
          final sessionHash = widget.ctrl.keygenState?.sessionHash;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.center,
            spacing: 12,
            children: [
              const Text(
                'Check that this code is identical and matches on every device',
                textAlign: TextAlign.center,
              ),
              Card.filled(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: AnimatedCrossFade(
                    firstChild: const Padding(
                      padding: EdgeInsets.all(8),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        spacing: 12,
                        children: [
                          CircularProgressIndicator(),
                          Text('This can take a few seconds...'),
                        ],
                      ),
                    ),
                    secondChild: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          '${widget.pending.threshold}-of-${widget.pending.participants.length}',
                          style: theme.textTheme.labelLarge,
                        ),
                        Text(
                          _formatChecksum(sessionHash?.field0),
                          style: theme.textTheme.headlineLarge?.copyWith(
                            fontFamily: monospaceTextStyle.fontFamily,
                          ),
                        ),
                      ],
                    ),
                    crossFadeState: sessionHash == null
                        ? CrossFadeState.showFirst
                        : CrossFadeState.showSecond,
                    duration: Durations.medium1,
                  ),
                ),
              ),
              Text(
                'The security check code confirms that all devices have behaved honestly during key generation.',
                textAlign: TextAlign.center,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ],
          );
        },
      ),
      actionButtons: [
        OutlinedButton(
          onPressed: () => _session?.cancel(),
          child: const Text('Cancel'),
        ),
        ListenableBuilder(
          listenable: widget.ctrl,
          builder: (context, _) {
            final theme = Theme.of(context);
            final state = widget.ctrl.keygenState;
            return Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 12,
              children: [
                Text(
                  'Confirm on device',
                  style: theme.textTheme.labelMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
                LargeCircularProgressIndicator(
                  size: 36,
                  progress: state == null
                      ? 0
                      : state.sessionAcks
                          .where(_localDevices.contains)
                          .length,
                  total: _localDevices.length,
                ),
              ],
            );
          },
        ),
      ],
    );
  }

  String _formatChecksum(List<int>? sessionHashBytes) {
    final bytes = sessionHashBytes != null && sessionHashBytes.length >= 4
        ? sessionHashBytes.sublist(0, 4)
        : <int>[0, 0, 0, 0];
    return toSpacedHex(Uint8List.fromList(bytes));
  }

  Future<void> _onCancelTapped() async {
    final confirm = await showDialog<bool>(
      context: context,
      builder: (ctx) {
        final theme = Theme.of(ctx);
        return AlertDialog(
          icon: const Icon(Icons.cancel_outlined),
          title: const Text('Cancel this keygen?'),
          content: const Text(
            'This stops the keygen on your side. Other participants will '
            'sit waiting until you tell them out-of-band.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(ctx).pop(false),
              child: const Text('Keep going'),
            ),
            FilledButton(
              style: FilledButton.styleFrom(
                backgroundColor: theme.colorScheme.error,
                foregroundColor: theme.colorScheme.onError,
              ),
              onPressed: () => Navigator.of(ctx).pop(true),
              child: const Text('Cancel keygen'),
            ),
          ],
        );
      },
    );
    if (confirm != true || !mounted) return;
    // Local-only teardown — no protocol message published. The ensuing
    // state.aborted pops the page via _onKgState.
    _session?.cancel();
  }

  @override
  void dispose() {
    widget.ctrl.removeListener(_onCtrlChanged);
    final fullscreen = _fullscreenController;
    _fullscreenController = null;
    fullscreen?.dispose();
    // Note: the `_keygenSession` and its stream subscription are owned
    // by `LobbyAndKeygenController`; they're cleaned up in its
    // `dispose`. We just unhook our listener here.
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return switch (_phase) {
      _AcceptPhase.awaiting => _buildAwaitingView(context),
      _AcceptPhase.verify => _buildVerifyView(context),
    };
  }

  Widget _buildAwaitingView(BuildContext context) {
    final theme = Theme.of(context);
    final state = widget.ctrl.lobbyState;
    final pending = widget.pending;
    final ackedCount = pending.acked.length;
    final total = pending.participants.length;
    final allAcked = ackedCount == total;
    final kg = _kgState;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _Header(title: 'Waiting on keygen'),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
          child: Text(
            _statusLine(allAcked, ackedCount, total, kg),
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: _ThresholdHero(
            threshold: pending.threshold,
            total: total,
          ),
        ),
        const SizedBox(height: 12),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            shrinkWrap: true,
            children: [
              if (state != null)
                ..._ackParticipantRows(
                  ctrl: widget.ctrl,
                  state: state,
                  pending: pending,
                ),
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            children: [
              TextButton(
                onPressed: _onCancelTapped,
                style: TextButton.styleFrom(
                  foregroundColor: theme.colorScheme.error,
                ),
                child: const Text('Cancel keygen'),
              ),
              const Spacer(),
              if (allAcked && (kg == null || !kg.allAcks))
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(
                        strokeWidth: 2,
                        color: theme.colorScheme.primary,
                      ),
                    ),
                    const SizedBox(width: 12),
                    Text(
                      _spinnerLabel(kg),
                      style: theme.textTheme.labelLarge,
                    ),
                  ],
                ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildVerifyView(BuildContext context) {
    final theme = Theme.of(context);
    final pending = widget.pending;
    final state = widget.ctrl.lobbyState;
    final me = widget.ctrl.myPubkey;
    final others = pending.participants.where((p) => p.pubkey != me).toList();
    final allChecked = _verified.length == others.length;
    final canContinue = allChecked && !_confirming;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const _Header(title: 'Verify the security code'),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
          child: Text(
            'Contact each other participant out-of-band — phone, video '
            'call, or in person. Confirm the code below matches what they '
            'see on their device. Tick each one off as you do.',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Card.filled(
            child: Padding(
              padding: const EdgeInsets.symmetric(
                vertical: 12,
                horizontal: 16,
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    '${pending.threshold}-of-${pending.participants.length}',
                    style: theme.textTheme.labelLarge,
                  ),
                  Text(
                    _formatChecksum(_kgState?.sessionHash?.field0),
                    style: theme.textTheme.headlineLarge?.copyWith(
                      fontFamily: monospaceTextStyle.fontFamily,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
        const SizedBox(height: 12),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            shrinkWrap: true,
            children: [
              if (state != null)
                ..._verifyChecklistRows(
                  ctrl: widget.ctrl,
                  state: state,
                  pending: pending,
                  verified: _verified,
                  enabled: !_confirming,
                  onToggle: (pk, ok) {
                    setState(() {
                      if (ok) {
                        _verified.add(pk);
                      } else {
                        _verified.remove(pk);
                      }
                    });
                  },
                ),
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            children: [
              TextButton(
                onPressed: _confirming ? null : _onCancelTapped,
                style: TextButton.styleFrom(
                  foregroundColor: theme.colorScheme.error,
                ),
                child: const Text('Cancel keygen'),
              ),
              const Spacer(),
              FilledButton.icon(
                onPressed: canContinue ? _confirmAndFinalize : null,
                icon: _confirming
                    ? const SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.check_rounded),
                label: const Text('Continue'),
              ),
            ],
          ),
        ),
      ],
    );
  }

  String _statusLine(
      bool allAcked, int ackedCount, int total, KeyGenState? kg) {
    if (!allAcked) return '$ackedCount of $total accepted';
    if (kg == null) return 'Everyone is in. Starting keygen…';
    if (kg.aborted != null) return 'Aborted';
    if (kg.finished != null) return 'Finalized.';
    if (kg.allAcks) return 'Confirm the code on this device.';
    if (kg.sessionHash != null) {
      return 'Verify the security code on every device, then confirm.';
    }
    if (!kg.allShares) {
      return 'Awaiting shares (${kg.gotShares.length}/${kg.devices.length})…';
    }
    return 'Verifying with all participants…';
  }

  String _spinnerLabel(KeyGenState? kg) {
    if (kg == null) return 'Starting keygen…';
    if (!kg.allShares) return 'Awaiting shares…';
    if (kg.sessionHash == null) return 'Verifying…';
    return 'Awaiting acks…';
  }
}

/// Small ack-status pill — `Accepted ✓` (primary) or `Waiting` with
/// a spinner. Passed as `_ParticipantRow.trailingOverride` on the
/// accept screens so the lobby's accordion-style row is reused.
class _AckStatusPill extends StatelessWidget {
  const _AckStatusPill({required this.isAcked});
  final bool isAcked;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (isAcked) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.check_circle_rounded,
              size: 18, color: theme.colorScheme.primary),
          const SizedBox(width: 6),
          Text(
            'Accepted',
            style: theme.textTheme.labelMedium
                ?.copyWith(color: theme.colorScheme.primary),
          ),
        ],
      );
    }
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        SizedBox(
          width: 14,
          height: 14,
          child: CircularProgressIndicator(
            strokeWidth: 2,
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const SizedBox(width: 8),
        Text(
          'Waiting',
          style: theme.textTheme.labelMedium
              ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
        ),
      ],
    );
  }
}

/// Build the accept-screen participant accordion rows. Each
/// `SelectedParticipant` is looked up in `state.participants` (where
/// they must exist — they registered before being selected) so the
/// lobby's `_ParticipantRow` widget can reuse its existing
/// device-list expansion.
List<Widget> _ackParticipantRows({
  required LobbyAndKeygenController ctrl,
  required LobbyState state,
  required ResolvedKeygen pending,
}) {
  final me = ctrl.myPubkey;
  final initiator = state.initiator;
  final ackedSet = pending.acked.toSet();
  final rows = <Widget>[];
  int keyNumber = 1;
  for (final selected in pending.participants) {
    final info = state.participants[selected.pubkey];
    if (info == null) continue;
    final offset = keyNumber;
    keyNumber += info.devices.length;
    rows.add(
      _ParticipantRow(
        ctrl: ctrl,
        participant: info,
        isMe: info.pubkey == me,
        isInitiator: initiator != null && initiator == info.pubkey,
        keyOffset: offset,
        readOnly: true,
        trailingOverride:
            _AckStatusPill(isAcked: ackedSet.contains(info.pubkey)),
      ),
    );
  }
  return rows;
}

/// Build the verify-out-of-band checklist rows. One per *other*
/// participant (self is filtered out — you don't verify with yourself).
/// The trailing slot is a `Checkbox` rather than the ack pill.
List<Widget> _verifyChecklistRows({
  required LobbyAndKeygenController ctrl,
  required LobbyState state,
  required ResolvedKeygen pending,
  required Set<PublicKey> verified,
  required bool enabled,
  required void Function(PublicKey, bool) onToggle,
}) {
  final me = ctrl.myPubkey;
  final initiator = state.initiator;
  final rows = <Widget>[];
  int keyNumber = 1;
  for (final selected in pending.participants) {
    final info = state.participants[selected.pubkey];
    if (info == null) {
      keyNumber += selected.devices.length;
      continue;
    }
    final offset = keyNumber;
    keyNumber += info.devices.length;
    if (info.pubkey == me) continue;
    final isVerified = verified.contains(info.pubkey);
    rows.add(
      _ParticipantRow(
        ctrl: ctrl,
        participant: info,
        isMe: false,
        isInitiator: initiator != null && initiator == info.pubkey,
        keyOffset: offset,
        readOnly: true,
        trailingOverride: Checkbox(
          value: isVerified,
          onChanged: enabled
              ? (v) => onToggle(info.pubkey, v ?? false)
              : null,
          visualDensity: VisualDensity.compact,
        ),
      ),
    );
  }
  return rows;
}

class _ThresholdHero extends StatelessWidget {
  const _ThresholdHero({required this.threshold, required this.total});
  final int threshold;
  final int total;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 16),
      child: Column(
        children: [
          RichText(
            textAlign: TextAlign.center,
            text: TextSpan(
              style: theme.textTheme.displayMedium?.copyWith(
                color: theme.colorScheme.onSurface,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
              children: [
                TextSpan(text: '$threshold'),
                TextSpan(
                  text: '  of  ',
                  style: theme.textTheme.titleMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                    letterSpacing: 1.4,
                  ),
                ),
                TextSpan(text: '$total'),
              ],
            ),
          ),
          const SizedBox(height: 4),
          Text(
            'signatures required to spend',
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
              letterSpacing: 0.3,
            ),
          ),
        ],
      ),
    );
  }
}

class _AcceptInfoRow extends StatelessWidget {
  const _AcceptInfoRow({
    required this.icon,
    required this.label,
    required this.value,
    this.valueColor,
  });
  final IconData icon;
  final String label;
  final String value;
  final Color? valueColor;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      children: [
        Icon(icon, size: 20, color: theme.colorScheme.onSurfaceVariant),
        const SizedBox(width: 12),
        Text(
          label,
          style: theme.textTheme.bodyMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        const Spacer(),
        Flexible(
          child: Text(
            value,
            textAlign: TextAlign.right,
            overflow: TextOverflow.ellipsis,
            style: theme.textTheme.titleSmall?.copyWith(
              color: valueColor ?? theme.colorScheme.onSurface,
            ),
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Shared pieces
// =============================================================================

class _WaitingForHostStatus extends StatelessWidget {
  const _WaitingForHostStatus();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 10),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 16,
            height: 16,
            child: CircularProgressIndicator(
              strokeWidth: 2,
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
          const SizedBox(width: 12),
          Text(
            'Waiting for host…',
            style: theme.textTheme.labelLarge?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}

/// Step header. Mirrors the spacing of the standard [TopBar] used by
/// `FullscreenDialogScaffold` (8px top spacer + `EdgeInsets.fromLTRB(16,
/// 12, 16, 16)` headline padding) so the title doesn't sit glued to the
/// dialog edge.
///
/// TODO: migrate the per-step Column layouts to `FullscreenDialogBody`
/// (sliver-based) so we can drop this in favour of the standard
/// `TopBarSliver` directly.
class _Header extends StatelessWidget {
  const _Header({required this.title, this.onBack});
  final String title;

  /// `null` hides the back arrow entirely — used on screens where the
  /// only valid exits are the explicit footer actions.
  final VoidCallback? onBack;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const SizedBox(height: 8),
        Padding(
          padding: EdgeInsets.fromLTRB(onBack == null ? 16 : 4, 12, 16, 16),
          child: Row(
            children: [
              if (onBack != null) ...[
                IconButton(
                  icon: const Icon(Icons.arrow_back_rounded),
                  onPressed: onBack,
                ),
                const SizedBox(width: 8),
              ],
              Expanded(child: Text(title, style: theme.textTheme.titleLarge)),
            ],
          ),
        ),
      ],
    );
  }
}

class _ChoiceCard extends StatelessWidget {
  const _ChoiceCard({
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.onTap,
    this.emphasized = false,
  });

  final IconData icon;
  final String title;
  final String subtitle;
  final VoidCallback onTap;
  final bool emphasized;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      elevation: emphasized ? 2 : 0,
      color: emphasized
          ? theme.colorScheme.secondaryContainer
          : theme.colorScheme.surfaceContainerHigh,
      clipBehavior: Clip.hardEdge,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(icon,
                  size: 32,
                  color: emphasized
                      ? theme.colorScheme.onSecondaryContainer
                      : theme.colorScheme.onSurfaceVariant),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  spacing: 4,
                  children: [
                    Text(title,
                        style: theme.textTheme.titleMedium?.copyWith(
                          color: emphasized
                              ? theme.colorScheme.onSecondaryContainer
                              : null,
                        )),
                    Text(subtitle,
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: emphasized
                              ? theme.colorScheme.onSecondaryContainer
                                  .withValues(alpha: 0.8)
                              : theme.colorScheme.onSurfaceVariant,
                        )),
                  ],
                ),
              ),
              Icon(Icons.chevron_right_rounded,
                  color: emphasized
                      ? theme.colorScheme.onSecondaryContainer
                      : theme.colorScheme.onSurfaceVariant),
            ],
          ),
        ),
      ),
    );
  }
}

// =============================================================================
// Controller helpers
// =============================================================================

extension on OrgKeygenController {
  /// Public-facing alias for the protected `notifyListeners`. Used by
  /// name-view `onChanged` when a bare TextField edit needs to nudge
  /// the bottom-button enabled state.
  void bump() {
    // ignore: invalid_use_of_visible_for_testing_member, invalid_use_of_protected_member
    notifyListeners();
  }
}
