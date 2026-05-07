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
import 'package:frostsnap/wallet_create.dart'
    show LargeCircularProgressIndicator;
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'package:sliver_tools/sliver_tools.dart';

// =============================================================================
// Steps
// =============================================================================

enum OrgKeygenStep { walletType, sessionRole, joinSession, nameWallet }

enum LobbyAndKeygenStep {
  lobby,
  review,
  acceptKeygenDecision,
  acceptKeygenAwaiting,
  acceptKeygenVerify,
}

enum OrgKeygenRole { host, participant }

/// Result popped by `OrgKeygenPage`; `null` means cancelled/backed out.
sealed class WalletTypeChoice {
  const WalletTypeChoice();
}

final class WalletTypeChoicePersonal extends WalletTypeChoice {
  const WalletTypeChoicePersonal();
}

final class WalletTypeChoiceOrganisation extends WalletTypeChoice {
  const WalletTypeChoiceOrganisation(this.accessStructureRef);
  final AccessStructureRef accessStructureRef;
}

/// Pre-lobby controller. Produces a [RemoteLobbyHandle] on submit.
class OrgKeygenController extends ChangeNotifier {
  OrgKeygenController({required this.nostrClient});

  final NostrClient nostrClient;

  OrgKeygenStep _step = OrgKeygenStep.walletType;
  OrgKeygenStep get step => _step;

  bool _isAnimationForward = true;
  bool get isAnimationForward => _isAnimationForward;

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

  String? _connectError;
  String? get connectError => _connectError;

  bool _connecting = false;
  bool get connecting => _connecting;

  void chosePersonal(BuildContext context) {
    Navigator.of(context).pop(const WalletTypeChoicePersonal());
  }

  void choseOrganisation() {
    _isAnimationForward = true;
    _step = OrgKeygenStep.sessionRole;
    notifyListeners();
  }

  void chooseCreateSession() {
    _isAnimationForward = true;
    _role = OrgKeygenRole.host;
    _step = OrgKeygenStep.nameWallet;
    notifyListeners();
  }

  void chooseJoinSession() {
    _isAnimationForward = true;
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
    _isAnimationForward = false;
    notifyListeners();
  }

  @override
  void dispose() {
    nameController.dispose();
    joinLinkController.dispose();
    super.dispose();
  }
}

/// Owns a live lobby and its remote keygen sub-flow.
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

  bool _isAnimationForward = true;
  bool get isAnimationForward => _isAnimationForward;

  /// Async keygen work can resume after the page has been dismissed.
  bool _disposed = false;

  LobbyState? _state;
  LobbyState? get lobbyState => _state;
  StreamSubscription<LobbyState>? _stateSub;
  // Held alive so Dart-side GC doesn't trigger Rust's `_stop()`.
  LobbyStateBroadcastSubscription? _stateBroadcastSub;

  /// Shared with `_DeviceSetupDialog` so typed names + upgrade state
  /// survive across dialog close / reopen. Owned by this controller —
  /// the dialog merely subscribes for rebuilds.
  final DeviceSetupController deviceSetup = DeviceSetupController();

  /// Host-side local threshold choice. Never published on its own —
  /// only put on the wire as part of `StartKeygen`.
  int? _pendingThreshold;

  /// Host-side deselection set; self-exclusion is rejected.
  final Set<String> _excludedHex = {};

  bool isExcluded(PublicKey pk) => _excludedHex.contains(pk.toHex());

  void setIncluded(PublicKey pk, bool included) {
    if (pk == _myPubkey) return;
    final hex = pk.toHex();
    final changed = included ? _excludedHex.remove(hex) : _excludedHex.add(hex);
    if (changed) notifyListeners();
  }

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

  bool get meIsReady {
    final s = _state;
    if (s == null) return false;
    return s.participants.values.any(
      (p) => p.pubkey == _myPubkey && p.status == ParticipantStatus.ready,
    );
  }

  Future<void> markReady(List<({DeviceId id, String name})> devices) async {
    final regs = devices
        .map(
          (d) => DeviceRegistration(
            deviceId: d.id,
            name: d.name,
            kind: DeviceKind.frostsnap,
          ),
        )
        .toList();
    await handle.markReady(devices: regs);
  }

  void goToReview() {
    final s = _state;
    if (s == null || !s.allReady()) return;
    _pendingThreshold ??= recommendedThreshold;
    _isAnimationForward = true;
    _step = LobbyAndKeygenStep.review;
    notifyListeners();
  }

  void goToAcceptKeygenDecision() {
    _ensureLocalDevicesComputed();
    if (_step == LobbyAndKeygenStep.acceptKeygenDecision) return;
    _isAnimationForward = true;
    _step = LobbyAndKeygenStep.acceptKeygenDecision;
    notifyListeners();
  }

  /// Local transition after ack publish or when relay state already has our ack.
  void goToAcceptKeygenAwaiting() {
    _ensureLocalDevicesComputed();
    if (_step == LobbyAndKeygenStep.acceptKeygenAwaiting) return;
    _isAnimationForward = true;
    _step = LobbyAndKeygenStep.acceptKeygenAwaiting;
    notifyListeners();
  }

  void _ensureLocalDevicesComputed() {
    if (_localDevices.isEmpty) {
      _localDevices = _computeLocalDevices();
    }
  }

  void goToAcceptKeygenVerify() {
    if (_step == LobbyAndKeygenStep.acceptKeygenVerify) return;
    _isAnimationForward = true;
    _step = LobbyAndKeygenStep.acceptKeygenVerify;
    notifyListeners();
  }

  bool get isOnAcceptKeygenStep =>
      _step == LobbyAndKeygenStep.acceptKeygenDecision ||
      _step == LobbyAndKeygenStep.acceptKeygenAwaiting ||
      _step == LobbyAndKeygenStep.acceptKeygenVerify;

  final Set<PublicKey> _verified = {};
  Set<PublicKey> get verifiedParticipants => _verified;

  void toggleVerified(PublicKey pk, bool ok) {
    final changed = ok ? _verified.add(pk) : _verified.remove(pk);
    if (changed) notifyListeners();
  }

  bool _confirming = false;
  bool get confirming => _confirming;

  List<DeviceId> _localDevices = const [];
  List<DeviceId> get localDevices => _localDevices;

  List<DeviceId> _computeLocalDevices() {
    final s = _state;
    final pending = s?.keygen;
    if (pending == null) return const [];
    for (final p in pending.participants) {
      if (p.pubkey == _myPubkey) {
        return p.devices.map((d) => d.deviceId).toList(growable: false);
      }
    }
    return const [];
  }

  Future<void> confirmMatch({required SymmetricKey encryptionKey}) async {
    final session = _keygenSession;
    if (session == null || _confirming) return;
    _confirming = true;
    notifyListeners();
    try {
      await session.confirmMatch(encryptionKey: encryptionKey);
    } finally {
      _confirming = false;
      notifyListeners();
    }
  }

  /// Local-only teardown; no protocol message is published.
  void cancelKeygen() {
    _keygenSession?.cancel();
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
      selected.add(
        SelectedCoordinator(pubkey: p.pubkey, registerEventId: regId),
      );
    }
    if (selected.isEmpty) {
      throw StateError('no Ready participants to include');
    }
    await handle.startKeygen(threshold: threshold, selected: selected);
  }

  Future<void> ackKeygen() async {
    final s = _state;
    if (s == null || s.keygen == null) {
      throw StateError('no pending keygen to ack');
    }
    await handle.ackKeygen(startKeygenEventId: s.keygen!.keygenEventId);
    goToAcceptKeygenAwaiting();
  }

  Future<void> cancelLobby() => handle.cancel();

  Future<void> leaveLobby() => handle.leave();

  bool back() {
    switch (_step) {
      case LobbyAndKeygenStep.lobby:
      case LobbyAndKeygenStep.acceptKeygenDecision:
      case LobbyAndKeygenStep.acceptKeygenAwaiting:
      case LobbyAndKeygenStep.acceptKeygenVerify:
        return false;
      case LobbyAndKeygenStep.review:
        _isAnimationForward = false;
        _step = LobbyAndKeygenStep.lobby;
        notifyListeners();
        return true;
    }
  }

  RemoteKeygenSessionHandle? _keygenSession;
  RemoteKeygenSessionHandle? get keygenSession => _keygenSession;

  KeyGenState? _keygenState;
  KeyGenState? get keygenState => _keygenState;

  StreamSubscription<KeyGenState>? _keygenStateSub;
  // Held alive so Dart-side GC doesn't collect the opaque subscription
  // and trigger Rust's `_stop()`.
  KeyGenStateBroadcastSubscription? _keygenBroadcastSub;

  bool _keygenStarting = false;

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
  void notifyListeners() {
    if (_disposed) return;
    super.notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
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

  bool _joinLinkAttempted = false;

  @override
  void initState() {
    super.initState();
    _ctrl = _ConcreteController(
      nostrClient: widget.nostrClient,
      nostrContextLookup: () => NostrContext.of(context),
    );
    _ctrl.addListener(_onUpdate);
    _ctrl.joinLinkController.addListener(_onJoinLinkTextChanged);
  }

  @override
  void dispose() {
    _ctrl.joinLinkController.removeListener(_onJoinLinkTextChanged);
    _ctrl.removeListener(_onUpdate);
    _ctrl.dispose();
    super.dispose();
  }

  void _onUpdate() {
    if (mounted) setState(() {});
  }

  void _onJoinLinkTextChanged() {
    if (!mounted) return;
    if (_joinLinkAttempted) {
      setState(() => _joinLinkAttempted = false);
    } else {
      setState(() {});
    }
  }

  /// Re-entry must be gated by `_ctrl.connecting` because the
  /// TextField's `onSubmitted` (Enter key) routes here too and the
  /// keyboard isn't blocked while the footer button is.
  void _trySubmitJoinLink() {
    if (_ctrl.connecting) return;
    if (_ctrl.joinLinkValid) {
      unawaited(_submitJoinLink());
    } else {
      setState(() => _joinLinkAttempted = true);
    }
  }

  Future<void> _submitName() async {
    final handle = await _ctrl.openLobbyAsHost();
    if (handle == null || !mounted) return;
    final settings = NostrContext.of(context).nostrSettings;
    final asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: LobbyAndKeygenPage(
        handle: handle,
        isHost: true,
        walletName: _ctrl.walletName,
        loadNsec: () async => settings.getNsec(),
      ),
    );
    if (!mounted) return;
    Navigator.of(
      context,
    ).pop(asRef == null ? null : WalletTypeChoiceOrganisation(asRef));
  }

  Future<void> _submitJoinLink() async {
    final handle = await _ctrl.openLobbyAsJoiner();
    if (handle == null || !mounted) return;
    final settings = NostrContext.of(context).nostrSettings;
    final asRef = await MaybeFullscreenDialog.show<AccessStructureRef>(
      context: context,
      barrierDismissible: false,
      child: LobbyAndKeygenPage(
        handle: handle,
        isHost: false,
        walletName: '', // joiner learns it via state.keyName
        loadNsec: () async => settings.getNsec(),
      ),
    );
    if (!mounted) return;
    Navigator.of(
      context,
    ).pop(asRef == null ? null : WalletTypeChoiceOrganisation(asRef));
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _ctrl.back(context);
      },
      child: SafeArea(child: _buildStep(context)),
    );
  }

  MultiStepDialogScaffold _buildStep(BuildContext context) {
    switch (_ctrl.step) {
      case OrgKeygenStep.walletType:
        return _buildWalletTypeStep(context);
      case OrgKeygenStep.sessionRole:
        return _buildSessionRoleStep(context);
      case OrgKeygenStep.joinSession:
        return _buildJoinSessionStep(context);
      case OrgKeygenStep.nameWallet:
        return _buildNameStep(context);
    }
  }

  Widget _backLeading() => IconButton(
    icon: const Icon(Icons.arrow_back_rounded),
    onPressed: () => _ctrl.back(context),
    tooltip: 'Back',
  );

  MultiStepDialogScaffold _buildWalletTypeStep(BuildContext context) {
    return MultiStepDialogScaffold(
      stepKey: OrgKeygenStep.walletType,
      title: const Text('Who is this for?'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: Column(
          spacing: 12,
          children: [
            _ChoiceCard(
              icon: Icons.person_rounded,
              title: 'Just me',
              subtitle:
                  'A personal wallet. You visit your devices in person to sign.',
              onTap: () => _ctrl.chosePersonal(context),
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
                _ctrl.choseOrganisation();
              },
            ),
          ],
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildSessionRoleStep(BuildContext context) {
    return MultiStepDialogScaffold(
      stepKey: OrgKeygenStep.sessionRole,
      title: const Text('Start or join a session'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: Column(
          spacing: 12,
          children: [
            _ChoiceCard(
              icon: Icons.add_circle_outline_rounded,
              title: 'Start a new session',
              subtitle: 'Invite others to join a wallet you\'re creating.',
              emphasized: true,
              onTap: _ctrl.chooseCreateSession,
            ),
            _ChoiceCard(
              icon: Icons.link_rounded,
              title: 'Join an existing session',
              subtitle: 'Accept an invite link from someone else.',
              onTap: _ctrl.chooseJoinSession,
            ),
          ],
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildJoinSessionStep(BuildContext context) {
    final errorText = (_joinLinkAttempted && !_ctrl.joinLinkValid)
        ? 'Not a valid invite link'
        : _ctrl.connectError;
    return MultiStepDialogScaffold(
      stepKey: OrgKeygenStep.joinSession,
      title: const Text('Join session'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: _JoinSessionInput(
          ctrl: _ctrl,
          errorText: errorText,
          onSubmit: _trySubmitJoinLink,
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          icon: _ctrl.connecting
              ? const SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.arrow_forward_rounded),
          iconAlignment: IconAlignment.end,
          onPressed:
              (_ctrl.connecting || _ctrl.joinLinkController.text.trim().isEmpty)
              ? null
              : _trySubmitJoinLink,
          label: const Text('Join'),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildNameStep(BuildContext context) {
    final devMode =
        SettingsContext.of(context)?.settings.isInDeveloperMode() ?? false;
    final canSubmit = _ctrl.nameValid && !_ctrl.connecting;
    return MultiStepDialogScaffold(
      stepKey: OrgKeygenStep.nameWallet,
      title: const Text('Name this wallet'),
      subtitle: 'All wallet participants will see this name.',
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              autofocus: true,
              controller: _ctrl.nameController,
              decoration: InputDecoration(
                border: const OutlineInputBorder(),
                hintText: 'e.g. Acme Treasury',
                errorText: _ctrl.connectError,
                errorMaxLines: 2,
              ),
              maxLength: 15,
              textCapitalization: TextCapitalization.words,
              onChanged: (_) => _ctrl.bump(),
              onSubmitted: (_) {
                if (canSubmit) unawaited(_submitName());
              },
            ),
            if (devMode)
              NetworkAdvancedOptions(
                selected: _ctrl.network,
                onChanged: _ctrl.setNetwork,
              ),
          ],
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          onPressed: canSubmit ? () => unawaited(_submitName()) : null,
          icon: _ctrl.connecting
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
    );
  }
}

class _ConcreteController extends OrgKeygenController {
  _ConcreteController({
    required super.nostrClient,
    required this.nostrContextLookup,
  });

  final NostrContext Function() nostrContextLookup;

  @override
  Future<String> _loadNsec() async {
    return nostrContextLookup().nostrSettings.getNsec();
  }
}

// =============================================================================
// Lobby + Keygen page (post-handle-acquisition)
// =============================================================================

/// Lobby/review/accept-keygen flow. Pops with [AccessStructureRef] on success.
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

  FullscreenActionDialogController? _fullscreenController;

  /// `keygenState.sessionAcks` grows monotonically; this set dedupes
  /// per-ack `removeActionNeeded` calls.
  final Set<DeviceId> _ackedForwardedToDialog = <DeviceId>{};

  bool _popped = false;

  bool _verifyTransitionInFlight = false;

  bool _ceremonyStarted = false;

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
    _ctrl.addListener(_onAcceptKeygenChanged);
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
    // If we've already acked (e.g. page rebuilt after relay-echo of our
    // own AckKeygen), skip the decision step and go straight to awaiting.
    final iAmAcked = pending.acked.any((pk) => pk == _ctrl.myPubkey);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      if (iAmAcked) {
        _ctrl.goToAcceptKeygenAwaiting();
      } else {
        _ctrl.goToAcceptKeygenDecision();
      }
    });
  }

  void _onAcceptKeygenChanged() {
    if (!mounted || _popped) return;
    final ctrl = _ctrl;

    if (ctrl.step == LobbyAndKeygenStep.acceptKeygenAwaiting &&
        !_ceremonyStarted) {
      _ceremonyStarted = true;
      unawaited(ctrl.startKeygenCeremony());
    }

    // Edge-detect "should overlay be up?" against a predicate (NOT
    // stage equality), so a redundant notify doesn't reinstantiate.
    final shouldShowOverlay =
        ctrl.step == LobbyAndKeygenStep.acceptKeygenAwaiting &&
        ctrl.keygenState != null;
    final overlayUp = _fullscreenController != null;
    if (shouldShowOverlay && !overlayUp) {
      _fullscreenController = _buildFullscreenController(context);
    } else if (!shouldShowOverlay && overlayUp) {
      _maybeDisposeFullscreen();
    }

    final kgState = ctrl.keygenState;
    if (kgState == null) return;

    final fc = _fullscreenController;
    if (fc != null) {
      for (final id in kgState.sessionAcks) {
        if (_ackedForwardedToDialog.add(id)) {
          unawaited(fc.removeActionNeeded(id));
        }
      }
    }

    if (kgState.aborted != null) {
      _popped = true;
      unawaited(_dismissOverlayThenPop(null));
      return;
    }

    if (kgState.finished != null) {
      _popped = true;
      unawaited(_dismissOverlayThenPop(kgState.finished));
      return;
    }

    if (kgState.allAcks &&
        ctrl.step == LobbyAndKeygenStep.acceptKeygenAwaiting &&
        !_verifyTransitionInFlight) {
      _verifyTransitionInFlight = true;
      unawaited(_transitionToVerify());
    }
  }

  Future<void> _transitionToVerify() async {
    await _fullscreenController?.awaitDismissed();
    if (!mounted || _popped) return;
    _ctrl.goToAcceptKeygenVerify();
  }

  void _maybeDisposeFullscreen() {
    final fc = _fullscreenController;
    if (fc == null) return;
    _fullscreenController = null;
    fc.dispose();
  }

  /// Overlay route lives on the root navigator and would orphan
  /// above a popped child dialog. `dispose()` alone doesn't dismiss
  /// it — must `clearAllActionsNeeded` first.
  Future<void> _dismissOverlayThenPop(AccessStructureRef? result) async {
    final fc = _fullscreenController;
    if (fc != null) {
      await fc.clearAllActionsNeeded();
      _maybeDisposeFullscreen();
    }
    if (!mounted) return;
    Navigator.of(context).pop(result);
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
      // Best-effort: if the publish failed (no relay reachable), still
      // pop the page so the user isn't stuck on the decision screen.
    }
    // Pop on both success and failure paths — `Leave` doesn't flip
    // `state.cancelled` for non-selected participants, so unlike the
    // host's CancelLobby we can't rely on `_watchForCancellation` to
    // unwind us. Same pattern as the lobby Leave button.
    if (mounted) Navigator.of(context).pop();
  }

  Future<void> _onCancelKeygenTapped() async {
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
    _ctrl.cancelKeygen();
  }

  Future<void> _confirmAndFinalize() async {
    try {
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      await _ctrl.confirmMatch(encryptionKey: encryptionKey);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Finalize failed: $e')));
    }
  }

  @override
  void dispose() {
    _ctrl.removeListener(_onUpdate);
    _ctrl.removeListener(_watchForCancellation);
    _ctrl.removeListener(_watchForPendingKeygen);
    _ctrl.removeListener(_onAcceptKeygenChanged);
    _maybeDisposeFullscreen();
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        _ctrl.back();
      },
      child: SafeArea(child: _buildStep(context)),
    );
  }

  MultiStepDialogScaffold _buildStep(BuildContext context) {
    switch (_ctrl.step) {
      case LobbyAndKeygenStep.lobby:
        return _buildLobbyStep(context);
      case LobbyAndKeygenStep.review:
        return _buildReviewStep(context);
      case LobbyAndKeygenStep.acceptKeygenDecision:
        return _buildAcceptDecisionStep(context);
      case LobbyAndKeygenStep.acceptKeygenAwaiting:
        return _buildAcceptAwaitingStep(context);
      case LobbyAndKeygenStep.acceptKeygenVerify:
        return _buildAcceptVerifyStep(context);
    }
  }

  MultiStepDialogScaffold _buildLobbyStep(BuildContext context) {
    final theme = Theme.of(context);
    final state = _ctrl.lobbyState;
    final channelReady = state != null && state.initiator != null;

    return MultiStepDialogScaffold(
      stepKey: LobbyAndKeygenStep.lobby,
      title: Text(state?.keyName ?? _ctrl.walletName),
      forward: _ctrl.isAnimationForward,
      // Exit must publish Cancel/Leave.
      subtitle: channelReady
          ? 'Add your devices while you wait for others to join.'
          : null,
      body: SliverToBoxAdapter(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (state != null &&
                state.keygen != null &&
                !state.keygen!.includes(pubkey: _ctrl.myPubkey))
              Card.filled(
                color: theme.colorScheme.surfaceContainerHighest,
                child: ListTile(
                  leading: Icon(
                    Icons.info_outline_rounded,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
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
            if (!channelReady)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 48),
                child: Column(
                  children: [
                    const CircularProgressIndicator(),
                    const SizedBox(height: 16),
                    Text(
                      'Connecting to relay…',
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              )
            else ...[
              Row(
                children: [
                  Expanded(
                    child: Text(
                      'Participants',
                      style: theme.textTheme.labelLarge,
                    ),
                  ),
                  Text(
                    state.allReady()
                        ? 'All ready'
                        : '${state.participants.values.where((p) => p.status != ParticipantStatus.joining).length} of ${state.participants.length} ready',
                    style: theme.textTheme.labelLarge,
                  ),
                ],
              ),
              const SizedBox(height: 4),
              ..._participantRows(ctrl: _ctrl, state: state, readOnly: false),
              const SizedBox(height: 12),
              if (_ctrl.isHost)
                _InviteTile(
                  onTap: () => _showInviteDialog(context, _ctrl.handle),
                ),
            ],
          ],
        ),
      ),
      footer: Row(
        children: [
          AsyncActionButton(
            onPressed: _ctrl.isHost
                ? _ctrl.cancelLobby
                : () async {
                    await _ctrl.leaveLobby();
                    if (context.mounted) Navigator.of(context).pop();
                  },
            style: FilledButton.styleFrom(
              backgroundColor: theme.colorScheme.error,
              foregroundColor: theme.colorScheme.onError,
            ),
            child: Text(_ctrl.isHost ? 'Cancel lobby' : 'Leave lobby'),
          ),
          const Spacer(),
          _LobbyPrimaryButton(ctrl: _ctrl),
        ],
      ),
    );
  }

  MultiStepDialogScaffold _buildReviewStep(BuildContext context) {
    final state = _ctrl.lobbyState;
    final total = _ctrl.totalDevices;
    return MultiStepDialogScaffold(
      stepKey: LobbyAndKeygenStep.review,
      title: const Text('Choose threshold'),
      leading: IconButton(
        icon: const Icon(Icons.arrow_back_rounded),
        onPressed: () => _ctrl.back(),
        tooltip: 'Back',
      ),
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            IgnorePointer(
              ignoring: !_ctrl.isHost,
              child: Opacity(
                opacity: _ctrl.isHost ? 1.0 : 0.75,
                child: ThresholdSelector(
                  threshold: _ctrl.displayThreshold.clamp(1, max(total, 1)),
                  totalDevices: max(total, 1),
                  recommendedThreshold: _ctrl.recommendedThreshold,
                  onChanged: (v) => _ctrl.setPendingThreshold(v),
                ),
              ),
            ),
            const SizedBox(height: 16),
            if (state != null)
              ..._participantRows(ctrl: _ctrl, state: state, readOnly: true),
          ],
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: _ReviewPrimaryButton(ctrl: _ctrl),
      ),
    );
  }

  MultiStepDialogScaffold _buildAcceptDecisionStep(BuildContext context) {
    final theme = Theme.of(context);
    final state = _ctrl.lobbyState;
    final pending = state?.keygen;
    final keyName = state?.keyName ?? '';
    final purpose = state?.purpose;
    final network = purpose?.bitcoinNetwork();
    final showNetwork = network != null && !network.isMainnet();

    return MultiStepDialogScaffold(
      stepKey: LobbyAndKeygenStep.acceptKeygenDecision,
      title: const Text('Generate this key?'),
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _AcceptInfoRow(
              icon: Icons.account_balance_wallet_rounded,
              label: 'Wallet',
              value: keyName,
            ),
            const SizedBox(height: 12),
            if (pending != null)
              _AcceptInfoRow(
                icon: Icons.security_rounded,
                label: 'Threshold',
                value:
                    '${pending.threshold} of ${pending.participants.length} required to spend',
              ),
            if (showNetwork)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: _AcceptInfoRow(
                  icon: Icons.dns_rounded,
                  label: 'Network',
                  value: network.name(),
                  valueColor: theme.colorScheme.error,
                ),
              ),
            const SizedBox(height: 24),
            Text('Participants', style: theme.textTheme.labelLarge),
            const SizedBox(height: 8),
            if (state != null && pending != null)
              ..._ackParticipantRows(
                ctrl: _ctrl,
                state: state,
                pending: pending,
              ),
          ],
        ),
      ),
      footer: Row(
        children: [
          TextButton(
            onPressed: () => _declineKeygen(),
            style: TextButton.styleFrom(
              foregroundColor: theme.colorScheme.error,
            ),
            child: const Text('Decline'),
          ),
          const Spacer(),
          AsyncActionButton(
            onPressed: _ctrl.ackKeygen,
            icon: Icons.arrow_forward_rounded,
            child: const Text('Accept'),
          ),
        ],
      ),
    );
  }

  MultiStepDialogScaffold _buildAcceptAwaitingStep(BuildContext context) {
    final theme = Theme.of(context);
    final state = _ctrl.lobbyState;
    final pending = state?.keygen;
    final ackedCount = pending?.acked.length ?? 0;
    final total = pending?.participants.length ?? 0;
    final allAcked = total > 0 && ackedCount == total;
    final kg = _ctrl.keygenState;

    return MultiStepDialogScaffold(
      stepKey: LobbyAndKeygenStep.acceptKeygenAwaiting,
      title: const Text('Waiting on keygen'),
      subtitle: _statusLine(allAcked, ackedCount, total, kg),
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (pending != null)
              _ThresholdHero(
                threshold: pending.threshold,
                total: pending.participants.length,
              ),
            const SizedBox(height: 12),
            if (state != null && pending != null)
              ..._ackParticipantRows(
                ctrl: _ctrl,
                state: state,
                pending: pending,
              ),
          ],
        ),
      ),
      footer: Row(
        children: [
          TextButton(
            onPressed: _onCancelKeygenTapped,
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
                Text(_spinnerLabel(kg), style: theme.textTheme.labelLarge),
              ],
            ),
        ],
      ),
    );
  }

  MultiStepDialogScaffold _buildAcceptVerifyStep(BuildContext context) {
    final theme = Theme.of(context);
    final state = _ctrl.lobbyState;
    final pending = state?.keygen;
    final me = _ctrl.myPubkey;
    final others = pending == null
        ? const []
        : pending.participants.where((p) => p.pubkey != me).toList();
    final verified = _ctrl.verifiedParticipants;
    // A one-participant keygen (others empty) is finalize-able as soon as
    // the user taps Continue — there's nothing to verify out-of-band.
    final allChecked = verified.length == others.length;
    final confirming = _ctrl.confirming;
    final canContinue = allChecked && !confirming;

    return MultiStepDialogScaffold(
      stepKey: LobbyAndKeygenStep.acceptKeygenVerify,
      title: const Text('Verify the security code'),
      subtitle:
          'Contact each other participant out-of-band — phone, video '
          'call, or in person. Confirm the code below matches what they '
          'see on their device. Tick each one off as you do.',
      forward: _ctrl.isAnimationForward,
      body: SliverToBoxAdapter(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (pending != null)
              Card.filled(
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
                        _formatChecksum(_ctrl.keygenState?.sessionHash?.field0),
                        style: theme.textTheme.headlineLarge?.copyWith(
                          fontFamily: monospaceTextStyle.fontFamily,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            const SizedBox(height: 12),
            if (state != null && pending != null)
              ..._verifyChecklistRows(
                ctrl: _ctrl,
                state: state,
                pending: pending,
                verified: verified,
                enabled: !confirming,
                onToggle: _ctrl.toggleVerified,
              ),
          ],
        ),
      ),
      footer: Row(
        children: [
          TextButton(
            onPressed: confirming ? null : _onCancelKeygenTapped,
            style: TextButton.styleFrom(
              foregroundColor: theme.colorScheme.error,
            ),
            child: const Text('Cancel keygen'),
          ),
          const Spacer(),
          FilledButton.icon(
            onPressed: canContinue ? _confirmAndFinalize : null,
            icon: confirming
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
    );
  }

  String _statusLine(
    bool allAcked,
    int ackedCount,
    int total,
    KeyGenState? kg,
  ) {
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

  String _formatChecksum(List<int>? sessionHashBytes) {
    final bytes = sessionHashBytes != null && sessionHashBytes.length >= 4
        ? sessionHashBytes.sublist(0, 4)
        : <int>[0, 0, 0, 0];
    return toSpacedHex(Uint8List.fromList(bytes));
  }

  FullscreenActionDialogController _buildFullscreenController(
    BuildContext context,
  ) {
    final localDevices = _ctrl.localDevices;
    return FullscreenActionDialogController(
      context: context,
      devices: localDevices,
      title: 'Security Check',
      body: (context) => ListenableBuilder(
        listenable: _ctrl,
        builder: (context, _) {
          final theme = Theme.of(context);
          final pending = _ctrl.lobbyState?.keygen;
          final sessionHash = _ctrl.keygenState?.sessionHash;
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
                    secondChild: pending == null
                        ? const SizedBox.shrink()
                        : Column(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                '${pending.threshold}-of-${pending.participants.length}',
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
          onPressed: () => _ctrl.cancelKeygen(),
          child: const Text('Cancel'),
        ),
        ListenableBuilder(
          listenable: _ctrl,
          builder: (context, _) {
            final theme = Theme.of(context);
            final state = _ctrl.keygenState;
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
                      : state.sessionAcks.where(localDevices.contains).length,
                  total: localDevices.length,
                ),
              ],
            );
          },
        ),
      ],
    );
  }
}

// =============================================================================
// Step 3a body: join-link input
// =============================================================================

class _JoinSessionInput extends StatefulWidget {
  const _JoinSessionInput({
    required this.ctrl,
    required this.errorText,
    required this.onSubmit,
  });
  final OrgKeygenController ctrl;

  final String? errorText;

  final VoidCallback onSubmit;

  @override
  State<_JoinSessionInput> createState() => _JoinSessionInputState();
}

class _JoinSessionInputState extends State<_JoinSessionInput> {
  OrgKeygenController get ctrl => widget.ctrl;
  bool _prefilled = false;
  final _focusNode = FocusNode();
  static const _prefix = 'frostsnap://keygen/';

  @override
  void initState() {
    super.initState();
    _focusNode.addListener(_onFocus);
  }

  @override
  void dispose() {
    _focusNode.removeListener(_onFocus);
    _focusNode.dispose();
    super.dispose();
  }

  void _onFocus() {
    if (!_focusNode.hasFocus) return;
    if (_prefilled || ctrl.joinLinkController.text.isNotEmpty) return;
    _prefilled = true;
    ctrl.joinLinkController.text = _prefix;
    ctrl.joinLinkController.selection = TextSelection.collapsed(
      offset: _prefix.length,
    );
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
    widget.onSubmit();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card.outlined(
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
                errorText: widget.errorText,
                errorMaxLines: 2,
              ),
              onSubmitted: (_) => widget.onSubmit(),
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
      return const FilledButton(
        onPressed: null,
        child: Text('Waiting for participants'),
      );
    }
    if (ctrl.totalDevices < 2) {
      return const FilledButton(
        onPressed: null,
        child: Text('Need at least 2 devices total'),
      );
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

  final bool isInitiator;

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
    final showExclusionToggle = widget.ctrl.isHost && !widget.isMe && isReady;
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
            child: Icon(
              Icons.keyboard_arrow_down_rounded,
              color: theme.colorScheme.onSurfaceVariant,
            ),
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
            child: Icon(
              Icons.keyboard_arrow_down_rounded,
              color: theme.colorScheme.onSurfaceVariant,
            ),
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
          style: theme.textTheme.bodySmall?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
        ),
        ParticipantStatus.ready => Row(
          mainAxisSize: MainAxisSize.min,
          spacing: 4,
          children: [
            Text(
              'Ready',
              style: theme.textTheme.labelMedium?.copyWith(color: Colors.green),
            ),
            const Icon(Icons.verified_rounded, size: 18, color: Colors.green),
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
                          child: const Icon(
                            Icons.star_rounded,
                            size: 14,
                            color: Color(0xFFFFC107),
                          ),
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
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
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
          Text(
            'Ready',
            style: theme.textTheme.labelMedium?.copyWith(color: Colors.green),
          ),
          const Icon(Icons.verified_rounded, size: 18, color: Colors.green),
        ],
      );
    }
    return Text(
      'Joining',
      style: theme.textTheme.bodySmall?.copyWith(
        color: theme.colorScheme.onSurfaceVariant,
      ),
    );
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
                  Icon(
                    Icons.key,
                    size: 16,
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                  const SizedBox(width: 8),
                  Text(
                    'Key #${keyOffset + i}',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      devices[i].name,
                      style: theme.textTheme.bodyMedium,
                      overflow: TextOverflow.ellipsis,
                    ),
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
    final isInitiator = state.initiator != null && state.initiator! == p.pubkey;
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
                Icon(
                  Icons.person_add_rounded,
                  size: 20,
                  color: theme.colorScheme.primary,
                ),
                const SizedBox(width: 10),
                Text(
                  'Invite participants',
                  style: theme.textTheme.titleSmall?.copyWith(
                    color: theme.colorScheme.primary,
                    fontWeight: FontWeight.w600,
                  ),
                ),
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
                  ScaffoldMessenger.of(
                    context,
                  ).showSnackBar(const SnackBar(content: Text('Copied')));
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

void _showDeviceSetupDialog(
  BuildContext context,
  LobbyAndKeygenController ctrl,
) {
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
                    style: TextStyle(color: theme.colorScheme.onErrorContainer),
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
// Step 6: accept keygen (joiner) — see _LobbyAndKeygenPageState's
// `_buildAcceptDecisionStep` / `_buildAcceptAwaitingStep` /
// `_buildAcceptVerifyStep` builder methods. Helpers below
// (`_AckStatusPill`, `_ThresholdHero`, `_AcceptInfoRow`,
// `_ackParticipantRows`, `_verifyChecklistRows`) are reused by those
// builders.
// =============================================================================

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
          Icon(
            Icons.check_circle_rounded,
            size: 18,
            color: theme.colorScheme.primary,
          ),
          const SizedBox(width: 6),
          Text(
            'Accepted',
            style: theme.textTheme.labelMedium?.copyWith(
              color: theme.colorScheme.primary,
            ),
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
          style: theme.textTheme.labelMedium?.copyWith(
            color: theme.colorScheme.onSurfaceVariant,
          ),
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
        trailingOverride: _AckStatusPill(
          isAcked: ackedSet.contains(info.pubkey),
        ),
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
          onChanged: enabled ? (v) => onToggle(info.pubkey, v ?? false) : null,
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
              Icon(
                icon,
                size: 32,
                color: emphasized
                    ? theme.colorScheme.onSecondaryContainer
                    : theme.colorScheme.onSurfaceVariant,
              ),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  spacing: 4,
                  children: [
                    Text(
                      title,
                      style: theme.textTheme.titleMedium?.copyWith(
                        color: emphasized
                            ? theme.colorScheme.onSecondaryContainer
                            : null,
                      ),
                    ),
                    Text(
                      subtitle,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: emphasized
                            ? theme.colorScheme.onSecondaryContainer.withValues(
                                alpha: 0.8,
                              )
                            : theme.colorScheme.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
              Icon(
                Icons.chevron_right_rounded,
                color: emphasized
                    ? theme.colorScheme.onSecondaryContainer
                    : theme.colorScheme.onSurfaceVariant,
              ),
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
