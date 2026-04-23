import 'dart:async';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/animated_gradient_card.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/nostr_chat/nostr_state.dart';
import 'package:frostsnap/nostr_chat/setup_dialog.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/name.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/remote_keygen.dart';
import 'package:frostsnap/threshold_selector.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';

// =============================================================================
// Steps
// =============================================================================

enum OrgKeygenStep { walletType, sessionRole, joinSession, nameWallet, lobby, review }

enum OrgKeygenRole { host, participant }

/// Result popped by `OrgKeygenPage` when the user picks a wallet type on
/// the first step. Organisation continues inside the page — the page
/// pops with `null` at that point only if the user backs out.
enum WalletTypeChoice { personal }

// =============================================================================
// Controller
// =============================================================================

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

  RemoteLobbyHandle? _handle;
  RemoteLobbyHandle? get handle => _handle;

  StreamSubscription<FfiLobbyState>? _stateSub;
  FfiLobbyState? _state;
  FfiLobbyState? get lobbyState => _state;

  PublicKey? _myPubkey;
  PublicKey? get myPubkey => _myPubkey;

  String? _openError;
  String? get openError => _openError;

  /// Host-side local threshold choice, before it's been published.
  /// After `set_threshold`, the authoritative value is on
  /// `_state.threshold`.
  int? _pendingThreshold;

  int get totalDevices {
    final s = _state;
    if (s == null) return 0;
    return s.participants.fold(0, (sum, p) => sum + p.devices.length);
  }

  int get recommendedThreshold {
    final total = totalDevices;
    if (total <= 1) return 1;
    return max((total * 2 / 3).ceil(), 1).clamp(1, total);
  }

  int get displayThreshold =>
      _state?.threshold ?? _pendingThreshold ?? recommendedThreshold;

  /// Whether the local user has already marked themselves Ready.
  bool get meIsReady {
    final me = _myPubkey;
    final s = _state;
    if (me == null || s == null) return false;
    return s.participants.any(
      (p) => p.pubkey.equals(other: me) && p.status != FfiParticipantStatus.joining,
    );
  }

  /// Whether the local user has accepted the current host-proposed threshold.
  bool get meAccepted {
    final me = _myPubkey;
    final s = _state;
    if (me == null || s == null) return false;
    return s.participants.any(
      (p) => p.pubkey.equals(other: me) && p.status == FfiParticipantStatus.accepted,
    );
  }

  // --- step transitions ---

  void chosePersonal(BuildContext context) {
    Navigator.of(context).pop(WalletTypeChoice.personal);
  }

  /// Organisation tile tapped (after the nostr-identity setup check).
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

  Future<void> submitName() async {
    if (!nameValid) return;
    _step = OrgKeygenStep.lobby;
    notifyListeners();
    await _openLobbyAsHost();
  }

  Future<void> submitJoinLink() async {
    if (!joinLinkValid) return;
    _step = OrgKeygenStep.lobby;
    notifyListeners();
    await _openLobbyAsJoiner(joinLinkController.text.trim());
  }

  Future<void> _openLobbyAsHost() async {
    try {
      final nsec = await _loadNsec();
      final secret = ChannelSecret.generate();
      final handle = await nostrClient.createRemoteLobby(
        channelSecret: secret,
        nsec: nsec,
      );
      _attachHandle(handle);
      // Host publishes the wallet name immediately so joiners see it.
      await handle.setKeyName(keyName: walletName, purpose: keyPurposeTest());
    } catch (e) {
      _openError = '$e';
      notifyListeners();
    }
  }

  Future<void> _openLobbyAsJoiner(String inviteLink) async {
    try {
      final secret = ChannelSecret.fromKeygenLink(link: inviteLink);
      final nsec = await _loadNsec();
      final handle = await nostrClient.joinRemoteLobby(
        channelSecret: secret,
        nsec: nsec,
      );
      _attachHandle(handle);
    } catch (e) {
      _openError = '$e';
      notifyListeners();
    }
  }

  void _attachHandle(RemoteLobbyHandle handle) {
    _handle = handle;
    _myPubkey = handle.myPubkey();
    _stateSub = handle.subState().start().listen((state) {
      _state = state;
      notifyListeners();
    });
    notifyListeners();
  }

  /// Throws on failure so the caller (the device-setup dialog) can keep
  /// itself open and surface the error. Previously this swallowed the
  /// exception into `_openError`, which left the dialog looking dead.
  Future<void> markReady(List<({DeviceId id, String name})> devices) async {
    final h = _handle;
    if (h == null) {
      throw StateError('lobby handle is gone');
    }
    final regs = devices
        .map(
          (d) => DeviceRegistration(
            deviceId: d.id,
            name: d.name,
            kind: DeviceKind.frostsnap,
          ),
        )
        .toList();
    await h.markReady(devices: regs);
  }

  /// Host — propose or change threshold (publishes immediately).
  Future<void> proposeThreshold(int threshold) async {
    final h = _handle;
    if (h == null) return;
    _pendingThreshold = threshold;
    await h.setThreshold(threshold: threshold);
  }

  Future<void> acceptThreshold() async {
    final h = _handle;
    final s = _state;
    if (h == null || s == null) return;
    final t = s.threshold;
    if (t == null) return;
    await h.acceptThreshold(threshold: t);
  }

  void setPendingThreshold(int v) {
    _pendingThreshold = v;
    notifyListeners();
  }

  Future<void> goToReview() async {
    final s = _state;
    if (s == null || !s.allReady) return;
    _pendingThreshold ??= recommendedThreshold;
    // Ensure the host's proposal is on the wire as we enter the review step.
    await proposeThreshold(_pendingThreshold!);
    _step = OrgKeygenStep.review;
    notifyListeners();
  }

  Future<void> tryStartKeygen(BuildContext context) async {
    final h = _handle;
    if (h == null) return;
    try {
      await h.startKeygen();
    } catch (e) {
      if (context.mounted) showErrorSnackbar(context, '$e');
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
      case OrgKeygenStep.lobby:
        // Leaving the lobby: hosts cancel for everyone, joiners just leave.
        unawaited(_teardownHandle(cancel: isHost));
        _step = isHost ? OrgKeygenStep.nameWallet : OrgKeygenStep.joinSession;
        _handle = null;
        _state = null;
        _myPubkey = null;
        _pendingThreshold = null;
      case OrgKeygenStep.review:
        _step = OrgKeygenStep.lobby;
    }
    notifyListeners();
  }

  Future<void> _teardownHandle({required bool cancel}) async {
    final sub = _stateSub;
    _stateSub = null;
    await sub?.cancel();
    final h = _handle;
    _handle = null;
    if (h != null) {
      try {
        if (cancel) {
          await h.cancel();
        } else {
          await h.leave();
        }
      } catch (_) {
        // Best-effort; relay may be unreachable.
      }
    }
  }

  @override
  void dispose() {
    unawaited(_teardownHandle(cancel: isHost));
    nameController.dispose();
    joinLinkController.dispose();
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
      child: SafeArea(child: _buildStep(context)),
    );
  }

  Widget _buildStep(BuildContext context) {
    switch (_ctrl.step) {
      case OrgKeygenStep.walletType:
        return _WalletTypeView(ctrl: _ctrl);
      case OrgKeygenStep.sessionRole:
        return _SessionRoleView(ctrl: _ctrl);
      case OrgKeygenStep.joinSession:
        return _JoinSessionView(ctrl: _ctrl);
      case OrgKeygenStep.nameWallet:
        return _NameView(ctrl: _ctrl);
      case OrgKeygenStep.lobby:
        return _LobbyView(ctrl: _ctrl);
      case OrgKeygenStep.review:
        return _ReviewView(ctrl: _ctrl);
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
  const _JoinSessionView({required this.ctrl});
  final OrgKeygenController ctrl;

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
    if (_attempted) setState(() => _attempted = false);
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
      unawaited(ctrl.submitJoinLink());
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

  void _scan() {
    // QR scanning exists elsewhere in the app but hasn't been wired up
    // to this flow yet. Surface a placeholder so the button isn't a
    // dead-end, matching the mockup's shape.
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content: Text('QR scanning from here is not wired up yet'),
        duration: Duration(seconds: 2),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final errorText = (_attempted && !ctrl.joinLinkValid)
        ? 'Not a valid invite link'
        : null;
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
              icon: const Icon(Icons.arrow_forward_rounded),
              iconAlignment: IconAlignment.end,
              onPressed:
                  ctrl.joinLinkController.text.trim().isEmpty ? null : _trySubmit,
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
  const _NameView({required this.ctrl});
  final OrgKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
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
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              hintText: 'e.g. Acme Treasury',
            ),
            maxLength: 15,
            textCapitalization: TextCapitalization.words,
            onChanged: (_) => (ctrl as _ConcreteController).bump(),
            onSubmitted: (_) => ctrl.submitName(),
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Align(
            alignment: Alignment.centerRight,
            child: FilledButton(
              onPressed: ctrl.nameValid ? ctrl.submitName : null,
              child: const Text('Next'),
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
  final OrgKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final state = ctrl.lobbyState;
    final handle = ctrl.handle;
    // Until the NIP-28 ChannelCreation event lands, the lobby has no
    // known initiator and rendering it would be misleading (host
    // missing, participant counts wrong). Show a spinner instead.
    final channelReady = state != null && state.initiator != null;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(
          title: state?.keyName ?? ctrl.walletName,
          onBack: () => ctrl.back(context),
        ),
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
              if (ctrl.openError != null)
                Card.filled(
                  color: theme.colorScheme.errorContainer,
                  child: ListTile(
                    leading: Icon(Icons.error_outline,
                        color: theme.colorScheme.onErrorContainer),
                    title: Text('${ctrl.openError}',
                        style: TextStyle(color: theme.colorScheme.onErrorContainer)),
                  ),
                ),
              if (state?.cancelled == true)
                Card.filled(
                  color: theme.colorScheme.errorContainer,
                  child: ListTile(
                    leading: Icon(Icons.cancel_outlined,
                        color: theme.colorScheme.onErrorContainer),
                    title: Text('Lobby cancelled by host',
                        style: TextStyle(color: theme.colorScheme.onErrorContainer)),
                  ),
                ),
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
                      state.allReady
                          ? 'All ready'
                          : '${state.participants.where((p) => p.status != FfiParticipantStatus.joining).length} of ${state.participants.length} ready',
                      style: theme.textTheme.labelLarge,
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                ..._participantRows(ctrl: ctrl, state: state, readOnly: false),
                const SizedBox(height: 12),
                if (ctrl.isHost && handle != null)
                  _InviteTile(onTap: () => _showInviteDialog(context, handle)),
              ],
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Align(
            alignment: Alignment.centerRight,
            child: _LobbyPrimaryButton(ctrl: ctrl),
          ),
        ),
      ],
    );
  }
}

class _LobbyPrimaryButton extends StatelessWidget {
  const _LobbyPrimaryButton({required this.ctrl});
  final OrgKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final state = ctrl.lobbyState;
    final handle = ctrl.handle;
    if (handle == null || state == null || state.initiator == null) {
      return const FilledButton(onPressed: null, child: Text('Connecting…'));
    }
    if (!ctrl.meIsReady) {
      return FilledButton.icon(
        icon: const Icon(Icons.add_rounded),
        label: const Text('Add your devices'),
        onPressed: () => _showDeviceSetupDialog(context, ctrl),
      );
    }
    if (!state.allReady) {
      return const FilledButton(onPressed: null, child: Text('Waiting for participants'));
    }
    if (ctrl.totalDevices < 2) {
      return const FilledButton(onPressed: null, child: Text('Need at least 2 devices total'));
    }
    if (!ctrl.isHost) {
      return const FilledButton(onPressed: null, child: Text('Waiting for host'));
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
    required this.keyOffset,
    this.readOnly = false,
  });

  final OrgKeygenController ctrl;
  final FfiLobbyParticipant participant;
  final bool isMe;
  /// The key-number of this participant's first device in the global
  /// (per-lobby) numbering — computed by the parent so device rows can
  /// show "Key #N" consistently.
  final int keyOffset;
  /// In review/readonly mode, the trailing slot is a phase-aware label
  /// instead of an edit icon, and the row starts expanded.
  final bool readOnly;

  @override
  State<_ParticipantRow> createState() => _ParticipantRowState();
}

class _ParticipantRowState extends State<_ParticipantRow> {
  late bool _expanded = widget.readOnly;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final p = widget.participant;
    final isReady = p.status == FfiParticipantStatus.ready ||
        p.status == FfiParticipantStatus.accepted;

    final Widget trailing;
    if (widget.readOnly) {
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
      final Widget trailingAction;
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
        trailingAction = const SizedBox.shrink();
      }

      final statusLabel = switch (p.status) {
        FfiParticipantStatus.joining => Text(
            widget.isMe ? 'Waiting for you' : 'Joined',
            style: theme.textTheme.bodySmall
                ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
          ),
        FfiParticipantStatus.ready => Row(
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
        FfiParticipantStatus.accepted => Row(
            mainAxisSize: MainAxisSize.min,
            spacing: 4,
            children: [
              Text('Accepted',
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
          SizedBox(width: 36, height: 36, child: trailingAction),
        ],
      );
    }

    return Card.filled(
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
                if (p.isInitiator)
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
    );
  }

  /// Phase-aware status for the review step:
  /// - everyone accepted → green "Ready"
  /// - threshold not set: host "Selecting threshold" / others "Waiting"
  /// - threshold set but participant hasn't accepted: host "Waiting" /
  ///   others "Reviewing threshold"
  Widget _reviewStatusLabel(BuildContext context, FfiLobbyParticipant p) {
    final theme = Theme.of(context);
    if (p.status == FfiParticipantStatus.accepted) {
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
    final thresholdSet = widget.ctrl.lobbyState?.threshold != null;
    final String text;
    if (!thresholdSet) {
      text = p.isInitiator ? 'Selecting threshold' : 'Waiting';
    } else {
      text = p.isInitiator ? 'Waiting' : 'Reviewing threshold';
    }
    return Text(text,
        style: theme.textTheme.bodySmall
            ?.copyWith(color: theme.colorScheme.onSurfaceVariant));
  }
}

class _DeviceList extends StatelessWidget {
  const _DeviceList({required this.devices, required this.keyOffset});
  final List<FfiLobbyDevice> devices;
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
  required OrgKeygenController ctrl,
  required FfiLobbyState state,
  required bool readOnly,
}) {
  final rows = <Widget>[];
  int keyNumber = 1;
  for (final p in state.participants) {
    final offset = keyNumber;
    keyNumber += p.devices.length;
    rows.add(
      _ParticipantRow(
        ctrl: ctrl,
        participant: p,
        isMe: ctrl.myPubkey != null && p.pubkey.equals(other: ctrl.myPubkey!),
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
  showDialog<void>(
    context: context,
    builder: (_) => _InviteDialog(inviteLink: handle.inviteLink()),
  );
}

class _InviteDialog extends StatelessWidget {
  const _InviteDialog({required this.inviteLink});
  final String inviteLink;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Dialog(
      clipBehavior: Clip.hardEdge,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 580),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 16, 8, 0),
              child: Row(
                children: [
                  Expanded(
                      child: Text('Invite participants',
                          style: theme.textTheme.titleLarge)),
                  IconButton(
                    icon: const Icon(Icons.close_rounded),
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 20, 20, 16),
              child: Center(
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
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 20),
              child: SelectableText(
                inviteLink,
                textAlign: TextAlign.center,
                style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 20),
              child: Row(
                spacing: 12,
                children: [
                  Expanded(
                    child: FilledButton.tonalIcon(
                      icon: const Icon(Icons.copy_rounded, size: 18),
                      label: const Text('Copy'),
                      onPressed: () async {
                        await Clipboard.setData(
                            ClipboardData(text: inviteLink));
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
            ),
          ],
        ),
      ),
    );
  }
}

// =============================================================================
// Device setup dialog
// =============================================================================

void _showDeviceSetupDialog(BuildContext context, OrgKeygenController ctrl) {
  showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (_) => _DeviceSetupDialog(ctrl: ctrl),
  );
}

class _DeviceSetupDialog extends StatefulWidget {
  const _DeviceSetupDialog({required this.ctrl});
  final OrgKeygenController ctrl;

  @override
  State<_DeviceSetupDialog> createState() => _DeviceSetupDialogState();
}

class _DeviceSetupDialogState extends State<_DeviceSetupDialog> {
  StreamSubscription<DeviceListUpdate>? _sub;
  List<ConnectedDevice> _devices = [];
  // Keyed by device id using the custom hash/eq helpers (DeviceId has no
  // working `==`). Text controllers persist for the lifetime of the
  // dialog; controllers for disconnected devices are disposed eagerly so
  // the user's typed names for re-plugged devices don't accidentally
  // resurrect.
  final Map<DeviceId, TextEditingController> _nameControllers = deviceIdMap();

  bool _submitting = false;
  String? _submitError;

  @override
  void initState() {
    super.initState();
    _sub = GlobalStreams.deviceListSubject.listen((update) {
      if (!mounted) return;
      setState(() {
        _devices = update.state.devices.toList();
        _syncControllers();
      });
    });
  }

  void _syncControllers() {
    final present = deviceIdSet(_devices.map((d) => d.id));
    // Add controllers for newly-plugged devices, pre-seeding with the
    // device's on-device name if it has one.
    for (final dev in _devices) {
      _nameControllers.putIfAbsent(
        dev.id,
        () => TextEditingController(text: dev.name ?? ''),
      );
    }
    // Drop controllers for devices that are no longer connected.
    final stale = _nameControllers.keys
        .where((id) => !present.contains(id))
        .toList();
    for (final id in stale) {
      _nameControllers.remove(id)?.dispose();
    }
  }

  @override
  void dispose() {
    _sub?.cancel();
    for (final c in _nameControllers.values) {
      c.dispose();
    }
    super.dispose();
  }

  String _name(DeviceId id) => _nameControllers[id]?.text.trim() ?? '';

  bool get _allNamed =>
      _devices.isNotEmpty && _devices.every((d) => _name(d.id).isNotEmpty);

  Future<void> _submit() async {
    setState(() {
      _submitting = true;
      _submitError = null;
    });
    try {
      final devices = _devices.map((d) => (id: d.id, name: _name(d.id))).toList();
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
    return Dialog(
      clipBehavior: Clip.hardEdge,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 580),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(4, 8, 20, 0),
              child: Row(
                children: [
                  IconButton(
                    icon: const Icon(Icons.arrow_back_rounded),
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                  const SizedBox(width: 4),
                  Expanded(
                    child: Text('Add your devices',
                        style: theme.textTheme.titleLarge),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 4, 20, 12),
              child: Text(
                'Each device you add will hold one key in the wallet.',
                style: theme.textTheme.bodySmall
                    ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
              ),
            ),
            Flexible(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(20, 0, 20, 16),
                shrinkWrap: true,
                children: [
                  if (_submitError != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 8),
                      child: Card.filled(
                        color: theme.colorScheme.errorContainer,
                        child: ListTile(
                          leading: Icon(Icons.error_outline,
                              color: theme.colorScheme.onErrorContainer),
                          title: Text(_submitError!,
                              style: TextStyle(
                                  color: theme.colorScheme.onErrorContainer)),
                        ),
                      ),
                    ),
                  if (_devices.isNotEmpty)
                    Text('Devices', style: theme.textTheme.labelLarge),
                  if (_devices.isNotEmpty) const SizedBox(height: 4),
                  for (final device in _devices)
                    _DeviceNameRow(
                      controller: _nameControllers[device.id]!,
                      onChanged: (_) => setState(() {}),
                    ),
                  const SizedBox(height: 8),
                  // Always-visible plug-in prompt — both the empty state
                  // and the "plug in more" hint.
                  AnimatedGradientCard(
                    child: const ListTile(
                      dense: true,
                      title: Text(
                          'Plug in all devices you want to hold a key.'),
                      contentPadding: EdgeInsets.symmetric(horizontal: 16),
                      leading: Icon(Icons.usb_rounded),
                    ),
                  ),
                ],
              ),
            ),
            const Divider(height: 0),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 12, 20, 16),
              child: Align(
                alignment: Alignment.centerRight,
                child: FilledButton.icon(
                  onPressed: (_allNamed && !_submitting) ? _submit : null,
                  icon: _submitting
                      ? const SizedBox(
                          width: 18,
                          height: 18,
                          child: CircularProgressIndicator(strokeWidth: 2))
                      : const Icon(Icons.arrow_forward_rounded),
                  iconAlignment: IconAlignment.end,
                  label: Text(
                    'Continue with ${_devices.length} '
                    '${_devices.length == 1 ? "device" : "devices"}',
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _DeviceNameRow extends StatelessWidget {
  const _DeviceNameRow({required this.controller, required this.onChanged});
  final TextEditingController controller;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card.filled(
      margin: const EdgeInsets.symmetric(vertical: 3),
      color: theme.colorScheme.surfaceContainerHigh,
      child: ListTile(
        leading: const Icon(Icons.key),
        title: ValueListenableBuilder<TextEditingValue>(
          valueListenable: controller,
          builder: (context, value, _) => TextField(
            controller: controller,
            autofocus: value.text.trim().isEmpty,
            maxLength: DeviceName.maxLength(),
            inputFormatters: [nameInputFormatter],
            decoration: InputDecoration(
              hintText: 'Name this device',
              isDense: true,
              counterText: '',
              suffixText: '${value.text.length}/${DeviceName.maxLength()}',
              suffixStyle: theme.textTheme.bodySmall
                  ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
              border: const OutlineInputBorder(borderSide: BorderSide.none),
              filled: true,
            ),
            onChanged: onChanged,
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
  final OrgKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final state = ctrl.lobbyState;
    final total = ctrl.totalDevices;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: 'Choose threshold', onBack: () => ctrl.back(context)),
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
                    onChanged: (v) {
                      // Host's drag updates local pending state AND
                      // publishes the proposal. Re-sending a SetThreshold
                      // clears acceptances, matching the mockup's
                      // lock/unlock semantics.
                      ctrl.setPendingThreshold(v);
                      unawaited(ctrl.proposeThreshold(v));
                    },
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
  final OrgKeygenController ctrl;

  @override
  Widget build(BuildContext context) {
    final state = ctrl.lobbyState;
    if (state == null) {
      return const FilledButton(onPressed: null, child: Text('Connecting…'));
    }
    if (!ctrl.isHost) {
      if (state.threshold == null) {
        return const FilledButton(onPressed: null, child: Text('Waiting for host'));
      }
      if (!ctrl.meAccepted) {
        return FilledButton.icon(
          icon: const Icon(Icons.check_rounded),
          onPressed: ctrl.acceptThreshold,
          label: const Text('Accept threshold'),
        );
      }
      return const FilledButton(onPressed: null, child: Text('Waiting for host'));
    }
    if (!state.allAccepted) {
      return const FilledButton(
        onPressed: null,
        child: Text('Waiting for participants'),
      );
    }
    return FilledButton(
      onPressed: () => ctrl.tryStartKeygen(context),
      child: const Text('Generate keys'),
    );
  }
}

// =============================================================================
// Shared pieces
// =============================================================================

class _Header extends StatelessWidget {
  const _Header({required this.title, required this.onBack});
  final String title;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 0, 16, 0),
      child: Row(
        children: [
          IconButton(icon: const Icon(Icons.arrow_back_rounded), onPressed: onBack),
          const SizedBox(width: 8),
          Expanded(child: Text(title, style: theme.textTheme.titleLarge)),
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
