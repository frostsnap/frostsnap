import 'dart:math';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:pretty_qr_code/pretty_qr_code.dart';
import 'device_identity.dart';
import 'keygen_mockup.dart'
    show AnimatedGradientCard, LargeCircularProgressIndicator;
import 'threshold_selector.dart';

// =============================================================================
// Organisation wallet keygen flow (redesign)
// =============================================================================
//
// User model:
//   - Coordinator = a person/signer with an identity (Nostr pubkey + name).
//   - Device = a key share holder. Each coordinator owns 1+ devices.
//   - Initiator is the organiser who sets wallet name + threshold + starts keygen.
//   - Other coordinators join via link, set up their own devices, mark Ready.
//   - Threshold is set only after all coordinators are Ready (N is known).
//
// Flow:
//   1. createRestore   → new / restore
//   2. walletType      → personal / organisation
//   3. nameWallet      → text input
//   4. inviteOwn       → initiator shares link (lobby with only them in it)
//   5. lobby           → participants list + "Set up devices" button
//                        device-setup is a DIALOG that returns here
//                        threshold control appears once all Ready
//   6. keygen          → fullscreen confirm
//   7. done

enum OrgKeygenStep {
  createRestore,
  walletType,
  sessionRole,     // Create a session / Join an existing session
  joinSession,     // Paste invite link or scan QR (participant only)
  inviteLanding,   // "Alice invites you to Acme Treasury" (participant only)
  nameWallet,      // Host only
  lobby,
  review,
  keygen,
  done,
}

enum OrgKeygenRole { host, participant }

enum ParticipantStatus {
  joining,   // link opened, has not set up yet
  settingUp, // device-setup dialog open on their end
  ready,     // finalized device list, marked Ready
  accepted,  // acked the threshold; ready for keygen
}

class Participant {
  final String id;
  final String displayName;
  final String pubkeyShort; // visible identity (e.g. "npub1x...abc")
  final bool isOrganiser;
  final bool isYou;
  ParticipantStatus status;
  bool includeAppKey;
  /// User-editable label for the coordinator device (laptop/phone).
  String coordinatorName;
  final List<String> deviceNames;

  Participant({
    required this.id,
    required this.displayName,
    required this.pubkeyShort,
    this.isOrganiser = false,
    this.isYou = false,
    this.status = ParticipantStatus.joining,
    this.includeAppKey = false,
    String? coordinatorName,
    List<String>? deviceNames,
  })  : coordinatorName = coordinatorName ?? DeviceIdentity.name,
        deviceNames = deviceNames ?? [];

  int get shareCount => deviceNames.length + (includeAppKey ? 1 : 0);
}

class OrgKeygenController extends ChangeNotifier {
  OrgKeygenStep _step = OrgKeygenStep.createRestore;
  OrgKeygenStep get step => _step;

  /// Whether you're hosting this session or joining someone else's.
  OrgKeygenRole _role = OrgKeygenRole.host;
  OrgKeygenRole get role => _role;
  bool get isHost => _role == OrgKeygenRole.host;

  final nameController = TextEditingController();
  String get walletName => nameController.text.trim();
  bool get nameValid => walletName.isNotEmpty && walletName.length <= 15;

  bool hasNostrIdentity = false;

  /// For the join-session step: link the user has pasted/scanned.
  final joinLinkController = TextEditingController();
  bool get joinLinkValid =>
      joinLinkController.text.trim().startsWith('frostsnap://join/');

  /// Preset landing data once a valid link is accepted (mock).
  String joiningWalletName = 'Acme Treasury';
  String joiningHostName = 'Alice';
  String joiningHostPubkey = 'npub1alice...abc';

  // Participants — "You" at index 0 (host in the default seed; the role
  // is flipped in the participant path before the lobby opens).
  final List<Participant> participants = [
    Participant(
      id: 'you',
      displayName: 'You',
      pubkeyShort: 'npub1you...you',
      isOrganiser: true,
      isYou: true,
    ),
  ];

  Participant get me => participants.firstWhere((p) => p.isYou);

  int? threshold;

  // Keygen progress
  final Set<int> ackedDevices = {};
  int get acksReceived => ackedDevices.length;
  String sessionHash = 'A3 F7 1B 9C';
  BuildContext? _keygenContext;

  String get inviteLink => 'frostsnap://join/a1b2c3d4e5f6';

  // --- Derived state ---

  int get totalShares =>
      participants.fold(0, (sum, p) => sum + p.shareCount);

  /// Every participant has finished their device setup (ready OR further).
  bool get allReady =>
      participants.isNotEmpty &&
      participants.every((p) =>
          p.status == ParticipantStatus.ready ||
          p.status == ParticipantStatus.accepted);

  /// Every participant has accepted the chosen threshold.
  bool get allAccepted =>
      participants.isNotEmpty &&
      participants.every((p) => p.status == ParticipantStatus.accepted);

  bool get canProceedToReview => allReady && totalShares >= 2;

  bool thresholdLocked = false;

  bool get canLockThreshold =>
      allReady &&
      !thresholdLocked &&
      threshold != null &&
      threshold! >= 1 &&
      threshold! <= totalShares;

  bool get canStartKeygen => thresholdLocked && allAccepted;

  // --- Step transitions ---

  void chooseCreate() {
    _step = OrgKeygenStep.walletType;
    notifyListeners();
  }

  void chooseOrganisation() {
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

  void submitJoinLink() {
    if (!joinLinkValid) return;
    _step = OrgKeygenStep.inviteLanding;
    notifyListeners();
  }

  /// Paste a preset mock link into the field (e.g. from the Scan button).
  void fillMockJoinLink() {
    joinLinkController.text = 'frostsnap://join/a1b2c3d4e5f6';
    notifyListeners();
  }

  /// Accept the invite — wallet seed is synthesized from the landing
  /// metadata, and the other participants + the host pre-populate the
  /// roster.
  void acceptInvite() {
    // Configure this mock session so it looks like Alice's wallet.
    nameController.text = joiningWalletName;
    // Mark yourself as a normal participant, not the organiser.
    participants[0] = Participant(
      id: 'you',
      displayName: 'You',
      pubkeyShort: 'npub1you...you',
      isOrganiser: false,
      isYou: true,
    );
    // Inject the host at the front of the list.
    participants.insert(
      0,
      Participant(
        id: 'host',
        displayName: joiningHostName,
        pubkeyShort: joiningHostPubkey,
        isOrganiser: true,
        isYou: false,
        // Pretend the host has already set up their devices.
        status: ParticipantStatus.ready,
        includeAppKey: true,
        coordinatorName: '$joiningHostName\'s phone',
        deviceNames: ['$joiningHostName\'s vault'],
      ),
    );
    _step = OrgKeygenStep.lobby;
    notifyListeners();
  }

  void submitName() {
    if (!nameValid) return;
    _step = OrgKeygenStep.lobby;
    notifyListeners();
  }

  void back(BuildContext context) {
    switch (_step) {
      case OrgKeygenStep.createRestore:
        Navigator.pop(context);
        return;
      case OrgKeygenStep.walletType:
        _step = OrgKeygenStep.createRestore;
      case OrgKeygenStep.sessionRole:
        _step = OrgKeygenStep.walletType;
      case OrgKeygenStep.joinSession:
        _step = OrgKeygenStep.sessionRole;
      case OrgKeygenStep.inviteLanding:
        _step = OrgKeygenStep.joinSession;
      case OrgKeygenStep.nameWallet:
        _step = OrgKeygenStep.sessionRole;
      case OrgKeygenStep.lobby:
        _step = isHost
            ? OrgKeygenStep.nameWallet
            : OrgKeygenStep.inviteLanding;
        // Tear down any roster / setup work so revisiting restarts clean.
        if (isHost) {
          participants.removeWhere((p) => !p.isYou);
        } else {
          participants.removeWhere((p) => !p.isYou && p.id != 'host');
        }
        me.status = ParticipantStatus.joining;
        me.deviceNames.clear();
        me.includeAppKey = false;
        threshold = null;
      case OrgKeygenStep.review:
        _step = OrgKeygenStep.lobby;
        // Going back invalidates prior readiness — everyone re-confirms.
        for (final p in participants) {
          p.status = ParticipantStatus.joining;
        }
        threshold = null;
        thresholdLocked = false;
      case OrgKeygenStep.keygen:
        return;
      case OrgKeygenStep.done:
        return;
    }
    notifyListeners();
  }

  // --- Simulation helpers (driven by the simulate panel) ---

  int _simCounter = 0;
  void simJoin(String displayName) {
    _simCounter += 1;
    participants.add(Participant(
      id: 'p$_simCounter',
      displayName: displayName,
      pubkeyShort: 'npub1${displayName.toLowerCase()}...${_simCounter}xy',
    ));
    threshold = _defaultThreshold();
    notifyListeners();
  }

  void simSettingUp(String participantId) {
    final p = participants.firstWhere((x) => x.id == participantId);
    p.status = ParticipantStatus.settingUp;
    notifyListeners();
  }

  void simMarkReady(String participantId, {int deviceCount = 1, bool appKey = false}) {
    final p = participants.firstWhere((x) => x.id == participantId);
    p.includeAppKey = appKey;
    p.deviceNames
      ..clear()
      ..addAll(List.generate(deviceCount, (i) => '${p.displayName}\'s device ${i + 1}'));
    p.status = ParticipantStatus.ready;
    threshold = _defaultThreshold();
    notifyListeners();
  }

  void removeParticipant(String participantId) {
    participants.removeWhere((p) => p.id == participantId && !p.isYou);
    threshold = _defaultThreshold();
    notifyListeners();
  }

  int _defaultThreshold() => recommendedThreshold;

  int get recommendedThreshold {
    if (totalShares <= 1) return 1;
    return max((totalShares * 2 / 3).toInt(), 1).clamp(1, totalShares);
  }

  // --- Organiser's own device setup (dialog actions) ---

  /// A hardware device is "pending" when it's been plugged in but not
  /// named yet. We represent it with an empty name in [deviceNames].
  /// The coordinator name must also be non-empty when enabled.
  bool get allMyDevicesNamed {
    if (me.includeAppKey && me.coordinatorName.trim().isEmpty) return false;
    return me.deviceNames.every((n) => n.trim().isNotEmpty);
  }

  void setMyCoordinatorName(String name) {
    me.coordinatorName = name.trim();
    notifyListeners();
  }

  /// Sim: physically plug in another device. Adds an unnamed slot.
  void simPlugInMyDevice() {
    me.deviceNames.add('');
    notifyListeners();
  }

  void setMyAppKey(bool on) {
    me.includeAppKey = on;
    notifyListeners();
  }

  /// Name (or rename) the hardware device at [index].
  void nameMyDevice(int index, String name) {
    me.deviceNames[index] = name.trim();
    notifyListeners();
  }

  void removeMyDevice(int index) {
    me.deviceNames.removeAt(index);
    notifyListeners();
  }

  void markMeReady() {
    if (me.shareCount < 1) return;
    if (!allMyDevicesNamed) return;
    me.status = ParticipantStatus.ready;
    threshold = _defaultThreshold();
    notifyListeners();
  }

  void reopenMySetup() {
    me.status = ParticipantStatus.settingUp;
    notifyListeners();
  }

  // --- Threshold / keygen ---

  void setThreshold(int v) {
    if (thresholdLocked) return;
    threshold = v;
    notifyListeners();
  }

  void goToReview() {
    if (!canProceedToReview) return;
    threshold ??= _defaultThreshold();
    _step = OrgKeygenStep.review;
    notifyListeners();
  }

  /// Organiser commits the threshold. They auto-accept it; others then
  /// have to accept individually.
  void lockThreshold() {
    if (!canLockThreshold) return;
    thresholdLocked = true;
    me.status = ParticipantStatus.accepted;
    notifyListeners();
  }

  /// Organiser unlocks the threshold to change it — resets any
  /// participant acceptances to `ready`.
  void unlockThreshold() {
    thresholdLocked = false;
    for (final p in participants) {
      if (p.status == ParticipantStatus.accepted) {
        p.status = ParticipantStatus.ready;
      }
    }
    notifyListeners();
  }

  /// Sim: a participant accepts the locked threshold.
  void simAcceptThreshold(String participantId) {
    if (!thresholdLocked) return;
    final p = participants.firstWhere((x) => x.id == participantId);
    if (p.status == ParticipantStatus.ready) {
      p.status = ParticipantStatus.accepted;
      notifyListeners();
    }
  }

  /// The viewer-participant accepts the host's locked threshold.
  void acceptThresholdAsMe() {
    if (!thresholdLocked) return;
    if (me.status == ParticipantStatus.ready) {
      me.status = ParticipantStatus.accepted;
      notifyListeners();
    }
  }

  /// Sim: the host locks the threshold (participant flow — when the
  /// viewer isn't the host, this is driven from the sim panel).
  void simHostLocksThreshold() {
    if (thresholdLocked) return;
    threshold ??= _defaultThreshold();
    thresholdLocked = true;
    // The host auto-accepts their own threshold.
    final host =
        participants.where((p) => p.isOrganiser).cast<Participant?>().firstWhere(
              (p) => true,
              orElse: () => null,
            );
    host?.status = ParticipantStatus.accepted;
    notifyListeners();
  }

  void startKeygen(BuildContext context) {
    if (!canStartKeygen) return;
    _step = OrgKeygenStep.keygen;
    ackedDevices.clear();
    _keygenContext = context;
    notifyListeners();
  }

  void cancelKeygen() {
    ackedDevices.clear();
    _step = OrgKeygenStep.lobby;
    notifyListeners();
  }

  void ackDevice(int myDeviceIndex) async {
    if (_step != OrgKeygenStep.keygen) return;
    if (ackedDevices.contains(myDeviceIndex)) return;
    ackedDevices.add(myDeviceIndex);
    notifyListeners();

    if (acksReceived >= me.deviceNames.length) {
      final context = _keygenContext;
      if (context != null && context.mounted) {
        final confirmed = await showDialog<bool>(
              context: context,
              barrierDismissible: false,
              builder: (ctx) {
                final theme = Theme.of(ctx);
                return AlertDialog(
                  title: const Text('Final check'),
                  content: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    spacing: 16,
                    children: [
                      const Text('Do all your devices show this code?'),
                      Card.filled(
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                              vertical: 12, horizontal: 16),
                          child: Column(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text('$threshold-of-$totalShares',
                                  style: theme.textTheme.labelLarge),
                              Text(sessionHash,
                                  style: theme.textTheme.headlineLarge),
                            ],
                          ),
                        ),
                      ),
                    ],
                  ),
                  actionsAlignment: MainAxisAlignment.spaceBetween,
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(ctx, false),
                        child: const Text('No')),
                    TextButton(
                        onPressed: () => Navigator.pop(ctx, true),
                        child: const Text('Yes')),
                  ],
                );
              },
            ) ??
            false;

        if (confirmed) {
          _step = OrgKeygenStep.done;
        } else {
          _step = OrgKeygenStep.lobby;
          ackedDevices.clear();
        }
        notifyListeners();
      }
    }
  }

  @override
  void dispose() {
    nameController.dispose();
    joinLinkController.dispose();
    super.dispose();
  }
}

// =============================================================================
// Page shell
// =============================================================================

class OrgKeygenPage extends StatefulWidget {
  final OrgKeygenController controller;
  const OrgKeygenPage({super.key, required this.controller});

  @override
  State<OrgKeygenPage> createState() => _OrgKeygenPageState();
}

class _OrgKeygenPageState extends State<OrgKeygenPage> {
  OrgKeygenController get _ctrl => widget.controller;

  @override
  void initState() {
    super.initState();
    _ctrl.addListener(_onUpdate);
  }

  void _onUpdate() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _ctrl.removeListener(_onUpdate);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _ctrl.back(context);
      },
      child: AnimatedSwitcher(
        duration: Durations.medium4,
        reverseDuration: Duration.zero,
        transitionBuilder: (child, animation) {
          final curved = CurvedAnimation(
            parent: animation,
            curve: Curves.easeInOutCubicEmphasized,
          );
          return SlideTransition(
            position: Tween<Offset>(
              begin: const Offset(1, 0),
              end: Offset.zero,
            ).animate(curved),
            child: FadeTransition(opacity: animation, child: child),
          );
        },
        child: _buildStep(context),
      ),
    );
  }

  Widget _buildStep(BuildContext context) {
    switch (_ctrl.step) {
      case OrgKeygenStep.createRestore:
        return _CreateRestoreView(
            key: const ValueKey('cr'), ctrl: _ctrl);
      case OrgKeygenStep.walletType:
        return _WalletTypeView(key: const ValueKey('wt'), ctrl: _ctrl);
      case OrgKeygenStep.sessionRole:
        return _SessionRoleView(
            key: const ValueKey('sr'), ctrl: _ctrl);
      case OrgKeygenStep.joinSession:
        return _JoinSessionView(
            key: const ValueKey('js'), ctrl: _ctrl);
      case OrgKeygenStep.inviteLanding:
        return _InviteLandingView(
            key: const ValueKey('il'), ctrl: _ctrl);
      case OrgKeygenStep.nameWallet:
        return _NameView(key: const ValueKey('nm'), ctrl: _ctrl);
      case OrgKeygenStep.lobby:
        return _LobbyView(key: const ValueKey('lb'), ctrl: _ctrl);
      case OrgKeygenStep.review:
        return _ReviewView(key: const ValueKey('rv'), ctrl: _ctrl);
      case OrgKeygenStep.keygen:
        return const SizedBox.shrink(key: ValueKey('gn'));
      case OrgKeygenStep.done:
        return _DoneView(key: const ValueKey('dn'), ctrl: _ctrl);
    }
  }
}

// =============================================================================
// Step 1: Create / Restore
// =============================================================================

class _CreateRestoreView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _CreateRestoreView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: 'Get started', onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
          child: Column(
            spacing: 12,
            children: [
              _ChoiceCard(
                icon: Icons.add_circle_outline_rounded,
                title: 'Create a new wallet',
                subtitle: 'Generate fresh keys with you and your participants.',
                emphasized: true,
                onTap: () => ctrl.chooseCreate(),
              ),
              _ChoiceCard(
                icon: Icons.restore_rounded,
                title: 'Restore a wallet',
                subtitle: 'Rejoin a wallet from an existing backup.',
                onTap: () {
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(content: Text('Restore not in mockup')),
                  );
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
// Step 2: Wallet type
// =============================================================================

class _WalletTypeView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _WalletTypeView({super.key, required this.ctrl});

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
                subtitle: 'A personal wallet. All your devices stay with you.',
                onTap: () {
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(content: Text('Personal flow lives elsewhere')),
                  );
                },
              ),
              _ChoiceCard(
                icon: Icons.groups_rounded,
                title: 'A group of us',
                subtitle:
                    'Share control with other participants. You can each be in a different place.',
                emphasized: true,
                onTap: () async {
                  if (!ctrl.hasNostrIdentity) {
                    final ok = await _showNostrSetupDialog(context);
                    if (!ok) return;
                    ctrl.hasNostrIdentity = true;
                  }
                  ctrl.chooseOrganisation();
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
// Step 2b: Create or Join session
// =============================================================================

class _SessionRoleView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _SessionRoleView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(
            title: 'Start or join a session',
            onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
          child: Column(
            spacing: 12,
            children: [
              _ChoiceCard(
                icon: Icons.add_circle_outline_rounded,
                title: 'Start a new session',
                subtitle:
                    'Invite others to join a wallet you\'re creating.',
                emphasized: true,
                onTap: () => ctrl.chooseCreateSession(),
              ),
              _ChoiceCard(
                icon: Icons.link_rounded,
                title: 'Join an existing session',
                subtitle:
                    'Accept an invite link from someone else.',
                onTap: () => ctrl.chooseJoinSession(),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step 2c: Join session (paste link or scan QR)
// =============================================================================

class _JoinSessionView extends StatefulWidget {
  final OrgKeygenController ctrl;
  const _JoinSessionView({super.key, required this.ctrl});

  @override
  State<_JoinSessionView> createState() => _JoinSessionViewState();
}

class _JoinSessionViewState extends State<_JoinSessionView> {
  OrgKeygenController get ctrl => widget.ctrl;

  @override
  void initState() {
    super.initState();
    ctrl.addListener(_onUpdate);
    ctrl.joinLinkController.addListener(_onUpdate);
  }

  void _onUpdate() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    ctrl.removeListener(_onUpdate);
    ctrl.joinLinkController.removeListener(_onUpdate);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(
            title: 'Join session',
            onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
          child: Text(
            'Paste the invite link you were sent, or scan a QR code.',
            style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
          child: TextField(
            autofocus: true,
            controller: ctrl.joinLinkController,
            decoration: InputDecoration(
              border: const OutlineInputBorder(),
              labelText: 'Invite link',
              hintText: 'frostsnap://join/…',
              suffixIcon: IconButton(
                icon: const Icon(Icons.content_paste_rounded),
                tooltip: 'Paste from clipboard',
                onPressed: () async {
                  final data = await Clipboard.getData('text/plain');
                  if (data?.text != null) {
                    ctrl.joinLinkController.text = data!.text!;
                  }
                },
              ),
            ),
            onSubmitted: (_) => ctrl.submitJoinLink(),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
          child: SizedBox(
            width: double.infinity,
            child: OutlinedButton.icon(
              icon: const Icon(Icons.qr_code_scanner_rounded),
              label: const Text('Scan QR code'),
              onPressed: () {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(
                    content: Text('Camera would open — mocked'),
                    duration: Duration(seconds: 1),
                  ),
                );
                ctrl.fillMockJoinLink();
              },
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
                  ctrl.joinLinkValid ? ctrl.submitJoinLink : null,
              label: const Text('Continue'),
            ),
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step 2d: Invite landing — "Alice invites you to Acme Treasury"
// =============================================================================

class _InviteLandingView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _InviteLandingView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(
            title: 'You\'re invited',
            onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 24, 24, 8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Icon(Icons.group_add_rounded,
                  size: 72, color: theme.colorScheme.primary),
              const SizedBox(height: 20),
              Text.rich(
                TextSpan(
                  style: theme.textTheme.titleMedium?.copyWith(
                      color: theme.colorScheme.onSurface),
                  children: [
                    TextSpan(
                      text: ctrl.joiningHostName,
                      style: const TextStyle(
                          fontWeight: FontWeight.w600),
                    ),
                    const TextSpan(text: ' invited you to join '),
                    TextSpan(
                      text: '"${ctrl.joiningWalletName}"',
                      style: const TextStyle(
                          fontWeight: FontWeight.w600),
                    ),
                    const TextSpan(text: '.'),
                  ],
                ),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 8),
              Text(ctrl.joiningHostPubkey,
                  style: theme.textTheme.bodySmall?.copyWith(
                      fontFamily: 'monospace',
                      color: theme.colorScheme.onSurfaceVariant)),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 24, 24, 8),
          child: Text(
            'Joining will make you a participant in this wallet. '
            'You\'ll need to contribute one or more devices to hold key shares.',
            style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant),
            textAlign: TextAlign.center,
          ),
        ),
        const SizedBox(height: 16),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              TextButton(
                onPressed: () => ctrl.back(context),
                child: const Text('Cancel'),
              ),
              FilledButton.icon(
                icon: const Icon(Icons.check_rounded),
                label: const Text('Join'),
                onPressed: ctrl.acceptInvite,
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step 3: Name wallet
// =============================================================================

class _NameView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _NameView({super.key, required this.ctrl});

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
            'This is shown to everyone you invite.',
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
// Step 4: Lobby (invite + device setup combined)
// =============================================================================

class _LobbyView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _LobbyView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final readyCount = ctrl.participants
        .where((p) => p.status == ParticipantStatus.ready)
        .length;
    final participantCount = ctrl.participants.length;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(title: ctrl.walletName, onBack: () => ctrl.back(context)),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
          child: Text(
            'Invite additional participants and select which devices you wish to use.',
            style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant),
          ),
        ),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            shrinkWrap: true,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text('Participants',
                        style: theme.textTheme.labelLarge),
                  ),
                  Text(
                    ctrl.allReady
                        ? 'All ready'
                        : '$readyCount of $participantCount ready',
                    style: theme.textTheme.labelLarge,
                  ),
                ],
              ),
              const SizedBox(height: 4),
              ...ctrl.participants.map((p) => _ParticipantRow(
                    ctrl: ctrl,
                    participant: p,
                  )),
              const SizedBox(height: 12),
              _InviteTile(onTap: () => _showInviteDialog(context, ctrl)),
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: _LobbyPrimaryButton(ctrl: ctrl),
        ),
      ],
    );
  }
}

class _LobbyPrimaryButton extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _LobbyPrimaryButton({required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final Widget button;
    if (!ctrl.allReady) {
      button = const FilledButton(
        onPressed: null,
        child: Text('Waiting for participants'),
      );
    } else if (ctrl.totalShares < 2) {
      button = const FilledButton(
        onPressed: null,
        child: Text('Need at least 2 devices total'),
      );
    } else if (!ctrl.isHost) {
      // Participants can't advance the flow.
      button = const FilledButton(
        onPressed: null,
        child: Text('Waiting for host'),
      );
    } else {
      button = FilledButton.icon(
        icon: const Icon(Icons.arrow_forward_rounded),
        iconAlignment: IconAlignment.end,
        onPressed: ctrl.canProceedToReview ? ctrl.goToReview : null,
        label: Text('Continue with ${ctrl.totalShares} devices'),
      );
    }
    return Align(
      alignment: Alignment.centerRight,
      child: button,
    );
  }
}

class _InviteTile extends StatelessWidget {
  final VoidCallback onTap;
  const _InviteTile({required this.onTap});

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
            padding: const EdgeInsets.symmetric(
                horizontal: 16, vertical: 16),
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

void _showInviteDialog(BuildContext context, OrgKeygenController ctrl) {
  showDialog<void>(
    context: context,
    builder: (_) => _InviteDialog(ctrl: ctrl),
  );
}

class _InviteDialog extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _InviteDialog({required this.ctrl});

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
            // Header
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 16, 8, 0),
              child: Row(
                children: [
                  Expanded(
                    child: Text('Invite participants',
                        style: theme.textTheme.titleLarge),
                  ),
                  IconButton(
                    icon: const Icon(Icons.close_rounded),
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 4, 20, 0),
              child: Text(
                'Anyone with this link can join before keygen starts.',
                style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant),
              ),
            ),
            // QR code
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 20, 20, 20),
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
                    data: ctrl.inviteLink,
                    decoration: const PrettyQrDecoration(
                      shape: PrettyQrSmoothSymbol(),
                    ),
                  ),
                ),
              ),
            ),
            // Link pill with inline copy
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
              child: _LinkPill(link: ctrl.inviteLink),
            ),
            // Share button
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 0, 20, 20),
              child: SizedBox(
                width: double.infinity,
                child: FilledButton.icon(
                  icon: const Icon(Icons.share_rounded, size: 18),
                  label: const Text('Share invite'),
                  onPressed: () {
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(
                        content: Text('Share sheet would open'),
                        duration: Duration(seconds: 2),
                      ),
                    );
                  },
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _LinkPill extends StatelessWidget {
  final String link;
  const _LinkPill({required this.link});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: theme.colorScheme.surfaceContainerHigh,
      borderRadius: BorderRadius.circular(28),
      child: InkWell(
        borderRadius: BorderRadius.circular(28),
        onTap: () {
          Clipboard.setData(ClipboardData(text: link));
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text('Link copied'),
              duration: Duration(seconds: 2),
            ),
          );
        },
        child: Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 8, 10),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  link,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    fontFamily: 'monospace',
                    color: theme.colorScheme.primary,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Container(
                padding: const EdgeInsets.symmetric(
                    horizontal: 12, vertical: 6),
                decoration: BoxDecoration(
                  color: theme.colorScheme.primaryContainer,
                  borderRadius: BorderRadius.circular(20),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.copy_rounded,
                        size: 16,
                        color: theme.colorScheme.onPrimaryContainer),
                    const SizedBox(width: 6),
                    Text(
                      'Copy',
                      style: theme.textTheme.labelMedium?.copyWith(
                          color:
                              theme.colorScheme.onPrimaryContainer),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ParticipantRow extends StatefulWidget {
  final OrgKeygenController ctrl;
  final Participant participant;
  final bool readOnly;
  const _ParticipantRow({
    required this.ctrl,
    required this.participant,
    this.readOnly = false,
  });

  @override
  State<_ParticipantRow> createState() => _ParticipantRowState();
}

class _ParticipantRowState extends State<_ParticipantRow> {
  late bool _expanded = !widget.readOnly;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final p = widget.participant;
    final ctrl = widget.ctrl;
    final isReady = p.status == ParticipantStatus.ready ||
        p.status == ParticipantStatus.accepted;

    // Special case: your row when not yet ready — the "Select devices"
    // button is the screen's primary action, replacing the usual
    // status + icon trailing. Hidden in read-only mode (review page).
    final Widget trailing;
    if (p.isYou && !isReady && !widget.readOnly) {
      trailing = FilledButton(
        onPressed: () => _openSetupDialog(context),
        child: const Text('Add devices'),
      );
    } else if (widget.readOnly) {
      // Review page: phase-aware status label + chevron (expand/collapse).
      final reviewLabel = _reviewStatusLabel(context, p);
      trailing = Row(
        mainAxisSize: MainAxisSize.min,
        spacing: 8,
        children: [
          reviewLabel,
          AnimatedRotation(
            turns: _expanded ? 0.5 : 0.0,
            duration: Durations.short3,
            child: Icon(Icons.keyboard_arrow_down_rounded,
                color: theme.colorScheme.onSurfaceVariant),
          ),
        ],
      );
    } else {
      // Trailing slot (36×36) — kept constant so status labels align.
      Widget trailingAction;
      if (p.isYou && isReady) {
        trailingAction = IconButton(
          icon: const Icon(Icons.edit_rounded, size: 18),
          tooltip: 'Edit your devices',
          visualDensity: VisualDensity.compact,
          padding: EdgeInsets.zero,
          color: theme.colorScheme.onSurfaceVariant,
          onPressed: () => _openSetupDialog(context),
        );
      } else if (!p.isYou && ctrl.isHost) {
        trailingAction = IconButton(
          icon: const Icon(Icons.remove_circle_outline, size: 18),
          tooltip: 'Remove participant',
          visualDensity: VisualDensity.compact,
          padding: EdgeInsets.zero,
          color: Colors.red,
          onPressed: () => ctrl.removeParticipant(p.id),
        );
      } else {
        trailingAction = const SizedBox.shrink();
      }

      Widget statusLabel;
      switch (p.status) {
        case ParticipantStatus.joining:
          statusLabel = Text('Joined',
              style: theme.textTheme.bodySmall
                  ?.copyWith(color: theme.colorScheme.onSurfaceVariant));
        case ParticipantStatus.settingUp:
          statusLabel = Text('Selecting keys',
              style: theme.textTheme.bodySmall
                  ?.copyWith(color: theme.colorScheme.onSurfaceVariant));
        case ParticipantStatus.ready:
          statusLabel = Row(
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
        case ParticipantStatus.accepted:
          statusLabel = Row(
            mainAxisSize: MainAxisSize.min,
            spacing: 4,
            children: [
              Text('Accepted',
                  style: theme.textTheme.labelMedium
                      ?.copyWith(color: Colors.green)),
              const Icon(Icons.verified_rounded,
                  size: 18, color: Colors.green),
            ],
          );
      }

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
                  backgroundColor: p.isYou
                      ? theme.colorScheme.surfaceContainerHighest
                      : theme.colorScheme.secondaryContainer,
                  child: Icon(
                    p.isYou
                        ? Icons.person_rounded
                        : Icons.person_outline_rounded,
                    color: p.isYou
                        ? theme.colorScheme.onSurfaceVariant
                        : theme.colorScheme.onSecondaryContainer,
                    size: 20,
                  ),
                ),
                if (p.isOrganiser)
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
            title: Text(p.displayName,
                style: theme.textTheme.titleSmall,
                overflow: TextOverflow.ellipsis),
            subtitle: Text(
              isReady
                  ? '${p.shareCount} ${p.shareCount == 1 ? "device" : "devices"}'
                  : '',
              style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant),
            ),
            trailing: trailing,
            onTap: isReady
                ? () => setState(() => _expanded = !_expanded)
                : null,
          ),
          AnimatedCrossFade(
            duration: Durations.short4,
            crossFadeState:
                (isReady && _expanded)
                    ? CrossFadeState.showSecond
                    : CrossFadeState.showFirst,
            firstChild: const SizedBox(width: double.infinity, height: 0),
            secondChild: _DeviceList(ctrl: ctrl, participant: p),
          ),
        ],
      ),
    );
  }

  void _openSetupDialog(BuildContext context) async {
    widget.ctrl.reopenMySetup();
    await showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => _DeviceSetupDialog(ctrl: widget.ctrl),
    );
  }

  /// Phase-aware label for the review page. Describes what the
  /// participant is currently doing.
  Widget _reviewStatusLabel(BuildContext context, Participant p) {
    final theme = Theme.of(context);
    final ctrl = widget.ctrl;

    // Everyone accepted → everyone reads "Ready".
    if (p.status == ParticipantStatus.accepted) {
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

    // Role-based status, independent of whose row this is.
    // The host picks the threshold; everyone else waits / reviews.
    final String text;
    if (!ctrl.thresholdLocked) {
      text = p.isOrganiser ? 'Selecting threshold' : 'Waiting';
    } else {
      text = p.isOrganiser ? 'Waiting' : 'Reviewing threshold';
    }
    return Text(text,
        style: theme.textTheme.bodySmall
            ?.copyWith(color: theme.colorScheme.onSurfaceVariant));
  }
}

class _DeviceList extends StatelessWidget {
  final OrgKeygenController ctrl;
  final Participant participant;
  const _DeviceList({required this.ctrl, required this.participant});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    // Global key numbering: sum shares of all participants that come
    // before this one in the ordered participants list.
    int keyNumber = 1;
    for (final p in ctrl.participants) {
      if (p.id == participant.id) break;
      keyNumber += p.shareCount;
    }

    final rows = <Widget>[];
    if (participant.includeAppKey) {
      rows.add(_deviceTile(
        context,
        keyNumber: keyNumber++,
        icon: DeviceIdentity.icon,
        label: participant.coordinatorName.trim().isNotEmpty
            ? participant.coordinatorName
            : (participant.isYou
                ? DeviceIdentity.name
                : '${participant.displayName}\'s phone'),
      ));
    }
    for (final name in participant.deviceNames) {
      rows.add(_deviceTile(
        context,
        keyNumber: keyNumber++,
        icon: FrostsnapIcons.device,
        label: name,
      ));
    }
    return Container(
      width: double.infinity,
      color: theme.colorScheme.surfaceContainerHighest,
      padding: const EdgeInsets.fromLTRB(72, 4, 16, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: rows,
      ),
    );
  }

  Widget _deviceTile(BuildContext context,
      {required int keyNumber,
      required IconData icon,
      required String label}) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Icon(icon,
              size: 16, color: theme.colorScheme.onSurfaceVariant),
          const SizedBox(width: 8),
          Text('Key #$keyNumber',
              style: theme.textTheme.bodyMedium?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant)),
          const SizedBox(width: 8),
          Expanded(
            child: Text(label,
                style: theme.textTheme.bodyMedium,
                overflow: TextOverflow.ellipsis),
          ),
        ],
      ),
    );
  }
}

// =============================================================================
// Step 5: Review + choose threshold
// =============================================================================

class _ReviewView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _ReviewView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final locked = ctrl.thresholdLocked;
    final acceptedCount = ctrl.participants
        .where((p) => p.status == ParticipantStatus.accepted)
        .length;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Header(
            title: 'Choose threshold',
            onBack: () => ctrl.back(context)),
        const SizedBox(height: 12),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
            shrinkWrap: true,
            children: [
              if (locked && ctrl.isHost)
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton.icon(
                    icon: const Icon(Icons.edit_rounded, size: 16),
                    label: const Text('Edit'),
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                    ),
                    onPressed: ctrl.unlockThreshold,
                  ),
                ),
              // The host picks; everyone else sees a read-only selector.
              IgnorePointer(
                ignoring: locked || !ctrl.isHost,
                child: Opacity(
                  opacity: (locked || !ctrl.isHost) ? 0.75 : 1.0,
                  child: _ThresholdCard(ctrl: ctrl),
                ),
              ),
              const SizedBox(height: 24),
              Row(
                children: [
                  Expanded(
                    child: Text('Participants',
                        style: theme.textTheme.labelLarge),
                  ),
                  if (locked)
                    Text(
                      ctrl.allAccepted
                          ? 'All accepted'
                          : '$acceptedCount of ${ctrl.participants.length} accepted',
                      style: theme.textTheme.labelLarge,
                    ),
                ],
              ),
              const SizedBox(height: 4),
              ...ctrl.participants.map((p) => _ParticipantRow(
                    ctrl: ctrl,
                    participant: p,
                    readOnly: true,
                  )),
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
  final OrgKeygenController ctrl;
  const _ReviewPrimaryButton({required this.ctrl});

  @override
  Widget build(BuildContext context) {
    // Participant path ---------------------------------------------------
    if (!ctrl.isHost) {
      if (!ctrl.thresholdLocked) {
        return const FilledButton(
          onPressed: null,
          child: Text('Waiting for host'),
        );
      }
      if (ctrl.me.status != ParticipantStatus.accepted) {
        return FilledButton.icon(
          icon: const Icon(Icons.check_rounded),
          onPressed: ctrl.acceptThresholdAsMe,
          label: const Text('Accept threshold'),
        );
      }
      return const FilledButton(
        onPressed: null,
        child: Text('Waiting for host'),
      );
    }

    // Host path ----------------------------------------------------------
    if (!ctrl.thresholdLocked) {
      return FilledButton.icon(
        icon: const Icon(Icons.arrow_forward_rounded),
        iconAlignment: IconAlignment.end,
        onPressed:
            ctrl.canLockThreshold ? ctrl.lockThreshold : null,
        label: const Text('Select threshold'),
      );
    }
    if (!ctrl.allAccepted) {
      return const FilledButton(
        onPressed: null,
        child: Text('Waiting for participants'),
      );
    }
    return FilledButton(
      onPressed: () => ctrl.startKeygen(context),
      child: const Text('Generate keys'),
    );
  }
}

class _ThresholdCard extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _ThresholdCard({required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final total = ctrl.totalShares;
    final value = (ctrl.threshold ?? ctrl.recommendedThreshold)
        .clamp(1, total);
    return ThresholdSelector(
      threshold: value,
      totalDevices: total,
      recommendedThreshold: ctrl.recommendedThreshold,
      onChanged: ctrl.setThreshold,
    );
  }
}

// =============================================================================
// Device setup dialog (your devices + app key + ready)
// =============================================================================

class _DeviceSetupDialog extends StatefulWidget {
  final OrgKeygenController ctrl;
  const _DeviceSetupDialog({required this.ctrl});

  @override
  State<_DeviceSetupDialog> createState() => _DeviceSetupDialogState();
}

class _DeviceSetupDialogState extends State<_DeviceSetupDialog> {
  OrgKeygenController get ctrl => widget.ctrl;

  /// Text controllers keyed by device index. Controllers persist for
  /// the lifetime of the dialog; when a device is removed, we drop the
  /// controller for that slot and reindex.
  final List<TextEditingController> _nameControllers = [];
  late final TextEditingController _coordinatorController;

  @override
  void initState() {
    super.initState();
    _coordinatorController =
        TextEditingController(text: ctrl.me.coordinatorName);
    _syncControllers();
    ctrl.addListener(_onUpdate);
  }

  void _onUpdate() {
    if (!mounted) return;
    _syncControllers();
    setState(() {});
  }

  /// Make [_nameControllers] match the length of `me.deviceNames`.
  /// New slots get an empty controller; removed slots are disposed.
  void _syncControllers() {
    final target = ctrl.me.deviceNames.length;
    while (_nameControllers.length < target) {
      final i = _nameControllers.length;
      _nameControllers.add(
          TextEditingController(text: ctrl.me.deviceNames[i]));
    }
    while (_nameControllers.length > target) {
      _nameControllers.removeLast().dispose();
    }
  }

  @override
  void dispose() {
    ctrl.removeListener(_onUpdate);
    for (final c in _nameControllers) {
      c.dispose();
    }
    _coordinatorController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final me = ctrl.me;
    final canReady = me.shareCount >= 1 && ctrl.allMyDevicesNamed;

    return Dialog(
      clipBehavior: Clip.hardEdge,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 580),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            // Header — matches _InviteDialog: title on left, close X on right.
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 16, 8, 0),
              child: Row(
                children: [
                  Expanded(
                    child: Text('Add devices',
                        style: theme.textTheme.titleLarge),
                  ),
                  IconButton(
                    icon: const Icon(Icons.close_rounded),
                    onPressed: () => Navigator.of(context).pop(),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 4, 20, 12),
              child: Text(
                'Plug in all devices you want to hold a key.',
                style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant),
              ),
            ),
            Flexible(
              child: ListView(
                padding: const EdgeInsets.fromLTRB(20, 0, 20, 16),
                shrinkWrap: true,
                children: [
                  // Coordinator toggle — the single source of truth for
                  // whether this laptop/phone contributes a key.
                  Card.outlined(
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(12),
                      side: BorderSide(
                        color: me.includeAppKey
                            ? theme.colorScheme.primary
                            : theme.colorScheme.outlineVariant,
                      ),
                    ),
                    child: SwitchListTile(
                      secondary: Icon(DeviceIdentity.icon,
                          color: me.includeAppKey
                              ? theme.colorScheme.primary
                              : null),
                      title: Text(
                          'Use this ${DeviceIdentity.name} as a key'),
                      value: me.includeAppKey,
                      onChanged: (v) => ctrl.setMyAppKey(v),
                    ),
                  ),

                  const SizedBox(height: 20),

                  Text('Devices', style: theme.textTheme.labelLarge),
                  const SizedBox(height: 4),

                  // Coordinator mirrored into the keys list when enabled.
                  // Read-only — toggle above is the control.
                  if (me.includeAppKey)
                    Card.filled(
                      margin: const EdgeInsets.symmetric(vertical: 3),
                      color: theme.colorScheme.surfaceContainerHigh,
                      child: ListTile(
                        leading: Icon(DeviceIdentity.icon),
                        title: ValueListenableBuilder<TextEditingValue>(
                          valueListenable: _coordinatorController,
                          builder: (context, value, _) => TextField(
                            controller: _coordinatorController,
                            maxLength: 14,
                            decoration: InputDecoration(
                              hintText: 'Name this device',
                              isDense: true,
                              counterText: '',
                              suffixText: '${value.text.length}/14',
                              suffixStyle: theme.textTheme.bodySmall
                                  ?.copyWith(
                                      color: theme
                                          .colorScheme.onSurfaceVariant),
                              border: const OutlineInputBorder(
                                  borderSide: BorderSide.none),
                              filled: true,
                            ),
                            onChanged: ctrl.setMyCoordinatorName,
                          ),
                        ),
                      ),
                    ),

                  // Hardware devices — each row is inline-editable so
                  // naming can be deferred until after plugging several
                  // in. The field IS the name; `Continue` enables once
                  // every slot has a non-empty name.
                  ...List.generate(me.deviceNames.length, (i) {
                    final controller = _nameControllers[i];
                    return Card.filled(
                      margin: const EdgeInsets.symmetric(vertical: 3),
                      color: theme.colorScheme.surfaceContainerHigh,
                      child: ListTile(
                        leading: const Icon(FrostsnapIcons.device),
                        title: ValueListenableBuilder<TextEditingValue>(
                          valueListenable: controller,
                          builder: (context, value, _) => TextField(
                            controller: controller,
                            autofocus:
                                me.deviceNames[i].trim().isEmpty,
                            maxLength: 14,
                            decoration: InputDecoration(
                              hintText: 'Name this device',
                              isDense: true,
                              counterText: '',
                              suffixText: '${value.text.length}/14',
                              suffixStyle: theme.textTheme.bodySmall
                                  ?.copyWith(
                                      color: theme
                                          .colorScheme.onSurfaceVariant),
                              border: const OutlineInputBorder(
                                  borderSide: BorderSide.none),
                              filled: true,
                            ),
                            onChanged: (v) => ctrl.nameMyDevice(i, v),
                          ),
                        ),
                      ),
                    );
                  }),

                  const SizedBox(height: 8),

                  // Always-visible plug-in prompt.
                  AnimatedGradientCard(
                    child: ListTile(
                      dense: true,
                      title: const Text(
                          'Plug in devices to include them in this wallet.'),
                      contentPadding:
                          const EdgeInsets.symmetric(horizontal: 16),
                      leading: const Icon(Icons.info_rounded),
                    ),
                  ),
                ],
              ),
            ),
            const Divider(height: 0),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 12, 20, 16),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      TextButton(
                        onPressed: () => Navigator.of(context).pop(),
                        child: const Text('Cancel'),
                      ),
                      const SizedBox(width: 4),
                      // Mockup-only: simulate a device being plugged in
                      // while the dialog is open.
                      Tooltip(
                        message: 'Simulate plug-in (mockup only)',
                        child: IconButton(
                          icon: const Icon(Icons.science_outlined),
                          color: theme.colorScheme.tertiary,
                          visualDensity: VisualDensity.compact,
                          onPressed: ctrl.simPlugInMyDevice,
                        ),
                      ),
                    ],
                  ),
                  FilledButton.icon(
                    onPressed: canReady
                        ? () {
                            ctrl.markMeReady();
                            Navigator.of(context).pop();
                          }
                        : null,
                    icon: const Icon(Icons.arrow_forward_rounded),
                    iconAlignment: IconAlignment.end,
                    label: Text(
                        'Continue with ${me.shareCount} ${me.shareCount == 1 ? "device" : "devices"}'),
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
// Step: Done
// =============================================================================

class _DoneView extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _DoneView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final participantCount = ctrl.participants.length;
    return Padding(
      padding: const EdgeInsets.all(32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Icon(Icons.verified_rounded, size: 64, color: Colors.green),
          const SizedBox(height: 16),
          Text('Wallet "${ctrl.walletName}" created!',
              style: theme.textTheme.headlineSmall,
              textAlign: TextAlign.center),
          const SizedBox(height: 8),
          Text(
            '${ctrl.threshold}-of-${ctrl.totalShares} across $participantCount participants',
            style: theme.textTheme.bodyLarge
                ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 24),
          FilledButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Done'),
          ),
        ],
      ),
    );
  }
}

// =============================================================================
// Shared header + card widgets
// =============================================================================

// =============================================================================
// Nostr setup dialog (mock)
// =============================================================================

Future<bool> _showNostrSetupDialog(BuildContext context) async {
  final result = await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (_) => const _NostrSetupDialog(),
  );
  return result ?? false;
}

class _NostrSetupDialog extends StatefulWidget {
  const _NostrSetupDialog();

  @override
  State<_NostrSetupDialog> createState() => _NostrSetupDialogState();
}

class _NostrSetupDialogState extends State<_NostrSetupDialog> {
  bool _showImport = false;
  final _nsecController = TextEditingController();

  @override
  void dispose() {
    _nsecController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    if (_showImport) {
      return AlertDialog(
        title: const Text('Import Nostr identity'),
        content: SizedBox(
          width: 400,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text('Enter your nsec:'),
              const SizedBox(height: 12),
              TextField(
                controller: _nsecController,
                decoration: const InputDecoration(
                  hintText: 'nsec1...',
                  border: OutlineInputBorder(),
                ),
                autocorrect: false,
                enableSuggestions: false,
              ),
              const SizedBox(height: 16),
              Row(
                children: [
                  Icon(Icons.warning_amber_rounded,
                      color: theme.colorScheme.error, size: 20),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Keep your nsec private! Never share it.',
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.error,
                      ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => setState(() => _showImport = false),
            child: const Text('Back'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Import'),
          ),
        ],
      );
    }

    return AlertDialog(
      title: const Text('Nostr identity required'),
      content: SizedBox(
        width: 400,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.person_outline_rounded,
                size: 64, color: theme.colorScheme.primary),
            const SizedBox(height: 16),
            Text(
              'Organisation wallets coordinate over Nostr. '
              'You\'ll need an identity to participate.',
              textAlign: TextAlign.center,
              style: theme.textTheme.bodyLarge,
            ),
            const SizedBox(height: 24),
            SizedBox(
              width: double.infinity,
              child: FilledButton.icon(
                onPressed: () => Navigator.of(context).pop(true),
                icon: const Icon(Icons.add),
                label: const Text('Generate new identity'),
              ),
            ),
            const SizedBox(height: 12),
            SizedBox(
              width: double.infinity,
              child: OutlinedButton.icon(
                onPressed: () => setState(() => _showImport = true),
                icon: const Icon(Icons.key),
                label: const Text('Import existing nsec'),
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
      ],
    );
  }
}

class _DashedBorderPainter extends CustomPainter {
  final Color color;
  final double radius;

  _DashedBorderPainter({required this.color, this.radius = 12});

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

class _Header extends StatelessWidget {
  final String title;
  final VoidCallback onBack;
  const _Header({
    required this.title,
    required this.onBack,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 0, 16, 0),
      child: Row(
        children: [
          IconButton(
              icon: const Icon(Icons.arrow_back_rounded),
              onPressed: onBack),
          const SizedBox(width: 8),
          Expanded(
            child: Text(title,
                style: theme.textTheme.titleLarge,
                overflow: TextOverflow.ellipsis),
          ),
        ],
      ),
    );
  }
}

class _ChoiceCard extends StatelessWidget {
  final IconData icon;
  final String title;
  final String subtitle;
  final bool emphasized;
  final VoidCallback onTap;
  const _ChoiceCard({
    required this.icon,
    required this.title,
    required this.subtitle,
    this.emphasized = false,
    required this.onTap,
  });

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
                                : null)),
                    Text(subtitle,
                        style: theme.textTheme.bodyMedium?.copyWith(
                            color: emphasized
                                ? theme.colorScheme.onSecondaryContainer
                                    .withValues(alpha: 0.8)
                                : theme.colorScheme.onSurfaceVariant)),
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
// Scaffold (entry point from mockup selector)
// =============================================================================

class OrgKeygenMockupScaffold extends StatefulWidget {
  /// When provided, pre-advances the flow past the create/restore and
  /// wallet-type pickers so the first visible screen matches this role.
  final OrgKeygenRole? startAsRole;

  const OrgKeygenMockupScaffold({super.key, this.startAsRole});

  @override
  State<OrgKeygenMockupScaffold> createState() =>
      _OrgKeygenMockupScaffoldState();
}

class _OrgKeygenMockupScaffoldState
    extends State<OrgKeygenMockupScaffold> {
  final _ctrl = OrgKeygenController();
  bool _simCollapsed = false;
  Offset _simOffset = const Offset(16, 16);

  @override
  void initState() {
    super.initState();
    _ctrl.addListener(() => mounted ? setState(() {}) : null);
    // Jump straight into the selected role so the mockup selector
    // can offer "host" and "participant" as separate entries.
    final start = widget.startAsRole;
    if (start == OrgKeygenRole.participant) {
      // Skip create/restore + wallet-type cards; land on join session.
      _ctrl.chooseJoinSession();
    }
  }

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final backgroundColor = theme.colorScheme.surface;
    final showSim = _ctrl.step == OrgKeygenStep.lobby ||
        _ctrl.step == OrgKeygenStep.review ||
        _ctrl.step == OrgKeygenStep.keygen;

    return Scaffold(
      backgroundColor: theme.colorScheme.surfaceContainerLowest,
      body: Stack(
        children: [
          if (_ctrl.step == OrgKeygenStep.keygen)
            _KeygenOverlay(ctrl: _ctrl)
          else
            Center(
              child: Dialog(
                backgroundColor: backgroundColor,
                clipBehavior: Clip.hardEdge,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 580),
                  child: OrgKeygenPage(controller: _ctrl),
                ),
              ),
            ),
          if (showSim)
            Positioned(
              left: _simOffset.dx,
              top: _simOffset.dy,
              child: GestureDetector(
                onPanUpdate: (d) => setState(() => _simOffset += d.delta),
                child: _buildSimPanel(context),
              ),
            ),
        ],
      ),
    );
  }

  Widget _buildSimPanel(BuildContext context) {
    final theme = Theme.of(context);
    if (_simCollapsed) {
      return Material(
        elevation: 8,
        borderRadius: BorderRadius.circular(28),
        color: theme.colorScheme.primaryContainer,
        child: InkWell(
          borderRadius: BorderRadius.circular(28),
          onTap: () => setState(() => _simCollapsed = false),
          child: Padding(
            padding:
                const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(Icons.science,
                    size: 18,
                    color: theme.colorScheme.onPrimaryContainer),
                const SizedBox(width: 8),
                Text('Simulate',
                    style: theme.textTheme.labelLarge?.copyWith(
                        color: theme.colorScheme.onPrimaryContainer)),
              ],
            ),
          ),
        ),
      );
    }

    return Material(
      elevation: 8,
      borderRadius: BorderRadius.circular(16),
      color: theme.colorScheme.surfaceContainerHigh,
      child: SizedBox(
        width: 300,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 4, 0),
              child: Row(
                children: [
                  Icon(Icons.science,
                      size: 16,
                      color: theme.colorScheme.onSurfaceVariant),
                  const SizedBox(width: 8),
                  Text('Simulate', style: theme.textTheme.labelLarge),
                  const Spacer(),
                  IconButton(
                    icon: const Icon(Icons.minimize, size: 18),
                    tooltip: 'Collapse',
                    onPressed: () =>
                        setState(() => _simCollapsed = true),
                    visualDensity: VisualDensity.compact,
                  ),
                ],
              ),
            ),
            if (_ctrl.step == OrgKeygenStep.lobby)
              ..._simLobbyRows(context),
            if (_ctrl.step == OrgKeygenStep.review)
              ..._simReviewRows(context),
            if (_ctrl.step == OrgKeygenStep.keygen)
              ..._simKeygenRows(context),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }

  List<Widget> _simLobbyRows(BuildContext context) {
    final rows = <Widget>[
      ListTile(
        dense: true,
        leading: const Icon(Icons.usb_rounded, size: 20),
        title: const Text('Plug in device'),
        subtitle: const Text('Triggers naming in setup dialog'),
        trailing: FilledButton.tonal(
          onPressed:
              _ctrl.simPlugInMyDevice,
          child: const Text('Plug in'),
        ),
      ),
      const Divider(height: 8),
      ListTile(
        dense: true,
        leading: const Icon(Icons.person_add, size: 20),
        title: const Text('Alice joins'),
        trailing: FilledButton.tonal(
          onPressed: _ctrl.participants.any((p) => p.displayName == 'Alice')
              ? null
              : () => _ctrl.simJoin('Alice'),
          child: const Text('Join'),
        ),
      ),
      ListTile(
        dense: true,
        leading: const Icon(Icons.person_add, size: 20),
        title: const Text('Bob joins'),
        trailing: FilledButton.tonal(
          onPressed: _ctrl.participants.any((p) => p.displayName == 'Bob')
              ? null
              : () => _ctrl.simJoin('Bob'),
          child: const Text('Join'),
        ),
      ),
      const Divider(height: 8),
    ];

    for (final p in _ctrl.participants) {
      if (p.isYou) continue;

      final String actionLabel;
      final VoidCallback onPressed;
      switch (p.status) {
        case ParticipantStatus.joining:
          actionLabel = 'Select';
          onPressed = () => _ctrl.simSettingUp(p.id);
        case ParticipantStatus.settingUp:
          actionLabel = 'Ready';
          onPressed = () =>
              _ctrl.simMarkReady(p.id, deviceCount: 1, appKey: true);
        case ParticipantStatus.ready:
        case ParticipantStatus.accepted:
          actionLabel = 'Edit';
          onPressed = () => _ctrl.simSettingUp(p.id);
      }

      rows.add(ListTile(
        dense: true,
        leading: const Icon(Icons.done_all, size: 20),
        title: Text(p.displayName),
        subtitle: Text('Status: ${_statusText(p.status)}'),
        trailing: FilledButton.tonal(
          onPressed: onPressed,
          child: Text(actionLabel),
        ),
      ));
    }

    // Participant-only: sim the host advancing everyone to review.
    if (!_ctrl.isHost) {
      rows.add(const Divider(height: 8));
      rows.add(ListTile(
        dense: true,
        leading: const Icon(Icons.star_rounded,
            size: 20, color: Color(0xFFFFC107)),
        title: const Text('Host advances'),
        subtitle: const Text('Go to threshold review'),
        trailing: FilledButton.tonal(
          onPressed: _ctrl.canProceedToReview ? _ctrl.goToReview : null,
          child: const Text('Continue'),
        ),
      ));
    }

    return rows;
  }

  String _statusText(ParticipantStatus s) => switch (s) {
        ParticipantStatus.joining => 'Joined',
        ParticipantStatus.settingUp => 'Selecting keys',
        ParticipantStatus.ready => 'Ready',
        ParticipantStatus.accepted => 'Accepted threshold',
      };

  List<Widget> _simReviewRows(BuildContext context) {
    final rows = <Widget>[];
    if (!_ctrl.thresholdLocked) {
      // Participant flow: host hasn't picked yet — expose a sim action
      // for them to lock. Host flow: they drive it from the page.
      if (!_ctrl.isHost) {
        rows.add(ListTile(
          dense: true,
          leading: const Icon(Icons.star_rounded,
              size: 20, color: Color(0xFFFFC107)),
          title: const Text('Host picks threshold'),
          subtitle: const Text('Locks it in for everyone to review'),
          trailing: FilledButton.tonal(
            onPressed: _ctrl.simHostLocksThreshold,
            child: const Text('Lock'),
          ),
        ));
      } else {
        rows.add(const ListTile(
          dense: true,
          title: Text('Threshold not locked yet'),
          subtitle: Text(
              'Press "Select threshold" on the page to lock it in.'),
        ));
      }
      return rows;
    }
    for (final p in _ctrl.participants) {
      if (p.isYou) continue;
      // Don't offer to "accept" for the host — they auto-accept when
      // they lock the threshold.
      if (p.isOrganiser) continue;
      final isAccepted = p.status == ParticipantStatus.accepted;
      rows.add(ListTile(
        dense: true,
        leading: const Icon(Icons.verified_rounded, size: 20),
        title: Text('${p.displayName} accepts'),
        subtitle: Text(
            isAccepted ? 'Already accepted' : 'Accepts locked threshold'),
        trailing: FilledButton.tonal(
          onPressed: isAccepted
              ? null
              : () => _ctrl.simAcceptThreshold(p.id),
          child: const Text('Accept'),
        ),
      ));
    }
    if (rows.isEmpty) {
      rows.add(const ListTile(
        dense: true,
        title: Text('No remote participants'),
      ));
    }
    // Participant-only: sim the host starting keygen once all accepted.
    if (!_ctrl.isHost) {
      rows.add(const Divider(height: 8));
      rows.add(ListTile(
        dense: true,
        leading: const Icon(Icons.star_rounded,
            size: 20, color: Color(0xFFFFC107)),
        title: const Text('Host generates keys'),
        subtitle: const Text('Starts the keygen ceremony'),
        trailing: FilledButton.tonal(
          onPressed: _ctrl.canStartKeygen
              ? () => _ctrl.startKeygen(context)
              : null,
          child: const Text('Generate'),
        ),
      ));
    }
    return rows;
  }

  List<Widget> _simKeygenRows(BuildContext context) {
    return List.generate(_ctrl.me.deviceNames.length, (i) {
      final acked = _ctrl.ackedDevices.contains(i);
      final name = _ctrl.me.deviceNames[i];
      return ListTile(
        dense: true,
        leading: const Icon(FrostsnapIcons.device, size: 20),
        title: Text(name),
        trailing: acked
            ? const Icon(Icons.verified_rounded,
                color: Colors.green, size: 20)
            : FilledButton.tonal(
                onPressed: () => _ctrl.ackDevice(i),
                child: const Text('Confirm'),
              ),
      );
    });
  }
}

// =============================================================================
// Keygen overlay
// =============================================================================

class _KeygenOverlay extends StatelessWidget {
  final OrgKeygenController ctrl;
  const _KeygenOverlay({required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      backgroundColor: Colors.black,
      body: Center(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16.0),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(FrostsnapIcons.device,
                  size: 96, color: Colors.white),
              const SizedBox(height: 32),
              Text('Security check',
                  style: theme.textTheme.headlineSmall
                      ?.copyWith(color: Colors.white)),
              const SizedBox(height: 24),
              Text('Confirm this code is shown on all your devices',
                  style: theme.textTheme.bodyLarge
                      ?.copyWith(color: Colors.white70),
                  textAlign: TextAlign.center),
              const SizedBox(height: 16),
              Card.filled(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text('${ctrl.threshold}-of-${ctrl.totalShares}',
                          style: theme.textTheme.labelLarge),
                      Text(ctrl.sessionHash,
                          style: theme.textTheme.headlineLarge),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
      persistentFooterButtons: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            OutlinedButton(
              onPressed: ctrl.cancelKeygen,
              child: const Text('Cancel'),
            ),
            Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 12,
              children: [
                Text('Confirm on device',
                    style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant)),
                LargeCircularProgressIndicator(
                  size: 36,
                  progress: ctrl.acksReceived,
                  total: ctrl.me.deviceNames.length,
                ),
              ],
            ),
          ],
        ),
      ],
    );
  }
}
