import 'dart:math';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'device_identity.dart';
import 'keygen_mockup.dart' show AnimatedGradientCard;

// =============================================================================
// Steps
// =============================================================================

enum RemoteKeygenStep {
  walletType,
  name,
  lobby,
  review,
  generating,
  done,
}

// =============================================================================
// Mock data
// =============================================================================

/// A local device that has been "plugged in". Starts unnamed.
class LocalDevice {
  String? name;
  LocalDevice();

  bool get isNamed => name != null && name!.trim().isNotEmpty;
}

class RemoteParticipant {
  final String displayName;
  final int deviceCount;
  RemoteParticipant(this.displayName, {this.deviceCount = 1});
}

// =============================================================================
// Controller
// =============================================================================

class RemoteKeygenController extends ChangeNotifier {
  RemoteKeygenStep _step = RemoteKeygenStep.walletType;
  RemoteKeygenStep get step => _step;

  final nameController = TextEditingController();

  bool hasNostrIdentity = false;

  // App key toggle
  bool includeAppKey = false;

  // Local devices that have been "plugged in"
  final List<LocalDevice> localDevices = [];

  // Remote participants who have joined
  final List<RemoteParticipant> remoteParticipants = [];

  int? threshold;

  // Keygen progress
  final Set<int> ackedDevices = {};
  int get acksReceived => ackedDevices.length;
  String sessionHash = 'A3 F7 1B 9C';
  BuildContext? _keygenContext;

  String get inviteLink => 'frostsnap://join/a1b2c3d4e5f6';

  // --- Derived ---

  String get walletName => nameController.text.trim();
  bool get nameValid => walletName.isNotEmpty && walletName.length <= 20;

  int get namedLocalDeviceCount =>
      localDevices.where((d) => d.isNamed).length;
  int get remoteDeviceCount =>
      remoteParticipants.fold(0, (sum, p) => sum + p.deviceCount);
  int get totalDeviceCount =>
      namedLocalDeviceCount + remoteDeviceCount + (includeAppKey ? 1 : 0);

  bool get allLocalDevicesNamed => localDevices.every((d) => d.isNamed);

  bool get hasLocalKeys => localDevices.isNotEmpty || includeAppKey;

  bool get canStartKeygen =>
      totalDeviceCount >= 2 &&
      allLocalDevicesNamed &&
      hasLocalKeys &&
      threshold != null &&
      threshold! >= 1 &&
      threshold! <= totalDeviceCount;

  void selectWalletType({required bool isOrganisation}) {
    _step = RemoteKeygenStep.name;
    notifyListeners();
  }

  void submitName(BuildContext context) {
    if (!nameValid) return;
    _step = RemoteKeygenStep.lobby;
    threshold = 1;
    notifyListeners();
  }

  void plugInDevice() {
    localDevices.add(LocalDevice());
    notifyListeners();
  }

  void nameLocalDevice(int index, String name) {
    localDevices[index].name = name;
    _updateDefaultThreshold();
    notifyListeners();
  }

  void removeLocalDevice(int index) {
    localDevices.removeAt(index);
    _updateDefaultThreshold();
    notifyListeners();
  }

  void removeParticipant(int index) {
    remoteParticipants.removeAt(index);
    _updateDefaultThreshold();
    notifyListeners();
  }

  void setThreshold(int value) {
    threshold = value;
    notifyListeners();
  }

  void toggleAppKey() {
    includeAppKey = !includeAppKey;
    _updateDefaultThreshold();
    notifyListeners();
  }

  void _updateDefaultThreshold() {
    if (totalDeviceCount <= 1) {
      threshold = 1;
    } else {
      final suggested = max((totalDeviceCount * 2 / 3).ceil(), 1);
      threshold = min(suggested, totalDeviceCount);
    }
  }

  void simulateParticipantJoin(String name, {int deviceCount = 1}) {
    remoteParticipants.add(RemoteParticipant(name, deviceCount: deviceCount));
    _updateDefaultThreshold();
    notifyListeners();
  }

  void startKeygen() {
    if (!canStartKeygen) return;
    _step = RemoteKeygenStep.review;
    notifyListeners();
  }

  void confirmAndGenerate(BuildContext context) {
    _step = RemoteKeygenStep.generating;
    ackedDevices.clear();
    _keygenContext = context;
    // App key auto-acks immediately
    if (includeAppKey) {
      appKeyAcked = true;
    }
    notifyListeners();
  }

  bool appKeyAcked = false;

  void ackDevice(int index) async {
    if (_step != RemoteKeygenStep.generating) return;
    if (ackedDevices.contains(index)) return;
    ackedDevices.add(index);
    notifyListeners();

    if (acksReceived >= namedLocalDeviceCount) {
      final context = _keygenContext;
      if (context != null && context.mounted) {
        final confirmed = await showDialog<bool>(
              context: context,
              barrierDismissible: false,
              builder: (context) {
                final theme = Theme.of(context);
                return AlertDialog(
                  title: const Text('Final check'),
                  content: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    spacing: 16,
                    children: [
                      const Text('Do all your devices show this code?'),
                      Card.filled(
                        child: Center(
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                                vertical: 12, horizontal: 16),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text('$threshold-of-$totalDeviceCount',
                                    style: theme.textTheme.labelLarge),
                                Text(sessionHash,
                                    style: theme.textTheme.headlineLarge),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                  actionsAlignment: MainAxisAlignment.spaceBetween,
                  actions: [
                    TextButton(
                      onPressed: () => Navigator.pop(context, false),
                      child: const Text('No'),
                    ),
                    TextButton(
                      onPressed: () => Navigator.pop(context, true),
                      child: const Text('Yes'),
                    ),
                  ],
                );
              },
            ) ??
            false;

        if (confirmed) {
          _step = RemoteKeygenStep.done;
        } else {
          _step = RemoteKeygenStep.lobby;
          ackedDevices.clear();
        }
        notifyListeners();
      }
    }
  }

  void back(BuildContext context) {
    switch (_step) {
      case RemoteKeygenStep.walletType:
        Navigator.pop(context);
        return;
      case RemoteKeygenStep.name:
        _step = RemoteKeygenStep.walletType;
      case RemoteKeygenStep.lobby:
        _step = RemoteKeygenStep.name;
        localDevices.clear();
        remoteParticipants.clear();
      case RemoteKeygenStep.review:
        _step = RemoteKeygenStep.lobby;
      case RemoteKeygenStep.generating:
        return;
      case RemoteKeygenStep.done:
        return;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    nameController.dispose();
    super.dispose();
  }
}

// =============================================================================
// Nostr setup dialog (mock)
// =============================================================================

Future<bool> showMockNostrSetupDialog(BuildContext context) async {
  final result = await showDialog<bool>(
    context: context,
    barrierDismissible: false,
    builder: (context) => const _MockNostrSetupDialog(),
  );
  return result ?? false;
}

class _MockNostrSetupDialog extends StatefulWidget {
  const _MockNostrSetupDialog();

  @override
  State<_MockNostrSetupDialog> createState() => _MockNostrSetupDialogState();
}

class _MockNostrSetupDialogState extends State<_MockNostrSetupDialog> {
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
        title: const Text('Import Nostr Identity'),
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
      title: const Text('Nostr Identity Required'),
      content: SizedBox(
        width: 400,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.person_outline_rounded,
                size: 64, color: theme.colorScheme.primary),
            const SizedBox(height: 16),
            Text(
              'Organisation wallets coordinate over Nostr. You\'ll need an identity to participate.',
              textAlign: TextAlign.center,
              style: theme.textTheme.bodyLarge,
            ),
            const SizedBox(height: 24),
            SizedBox(
              width: double.infinity,
              child: FilledButton.icon(
                onPressed: () => Navigator.of(context).pop(true),
                icon: const Icon(Icons.add),
                label: const Text('Generate New Identity'),
              ),
            ),
            const SizedBox(height: 12),
            SizedBox(
              width: double.infinity,
              child: OutlinedButton.icon(
                onPressed: () => setState(() => _showImport = true),
                icon: const Icon(Icons.key),
                label: const Text('Import Existing nsec'),
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

// =============================================================================
// Main page
// =============================================================================

class RemoteKeygenPage extends StatefulWidget {
  final RemoteKeygenController controller;

  const RemoteKeygenPage({super.key, required this.controller});

  @override
  State<RemoteKeygenPage> createState() => _RemoteKeygenPageState();
}

class _RemoteKeygenPageState extends State<RemoteKeygenPage> {
  RemoteKeygenController get _ctrl => widget.controller;

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
      case RemoteKeygenStep.walletType:
        return _WalletTypeView(key: const ValueKey('type'), ctrl: _ctrl);
      case RemoteKeygenStep.name:
        return _NameView(key: const ValueKey('name'), ctrl: _ctrl);
      case RemoteKeygenStep.lobby:
        return _LobbyView(key: const ValueKey('lobby'), ctrl: _ctrl);
      case RemoteKeygenStep.review:
        return _ReviewView(key: const ValueKey('review'), ctrl: _ctrl);
      case RemoteKeygenStep.generating:
        return const SizedBox.shrink(key: ValueKey('gen'));
      case RemoteKeygenStep.done:
        return _DoneView(key: const ValueKey('done'), ctrl: _ctrl);
    }
  }
}

// =============================================================================
// Step: Wallet type
// =============================================================================

class _WalletTypeView extends StatelessWidget {
  final RemoteKeygenController ctrl;
  const _WalletTypeView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 0, 16, 0),
          child: Row(
            children: [
              IconButton(
                icon: const Icon(Icons.arrow_back_rounded),
                onPressed: () => ctrl.back(context),
              ),
              const SizedBox(width: 8),
              Text('Create wallet', style: theme.textTheme.titleLarge),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 24, 16, 24),
          child: Column(
            spacing: 12,
            children: [
              _WalletTypeCard(
                icon: Icons.person_rounded,
                title: 'Personal wallet',
                subtitle:
                    'All your devices are here with you. Keys are generated locally.',
                onTap: () => Navigator.pop(context),
              ),
              _WalletTypeCard(
                icon: Icons.groups_rounded,
                title: 'Organisation wallet',
                subtitle:
                    'Devices are distributed across multiple locations. Coordinated over the internet.',
                emphasized: true,
                onTap: () async {
                  if (!ctrl.hasNostrIdentity) {
                    final ok = await showMockNostrSetupDialog(context);
                    if (!ok) return;
                    ctrl.hasNostrIdentity = true;
                  }
                  ctrl.selectWalletType(isOrganisation: true);
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
// Step: Name
// =============================================================================

class _NameView extends StatelessWidget {
  final RemoteKeygenController ctrl;
  const _NameView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 0, 16, 0),
          child: Row(
            children: [
              IconButton(
                icon: const Icon(Icons.arrow_back_rounded),
                onPressed: () => ctrl.back(context),
              ),
              const SizedBox(width: 8),
              Text('Name wallet', style: theme.textTheme.titleLarge),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
          child: Text('Choose a name for this wallet',
              style: theme.textTheme.titleMedium),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 24, 16, 24),
          child: TextField(
            autofocus: true,
            controller: ctrl.nameController,
            decoration: const InputDecoration(border: OutlineInputBorder()),
            maxLength: 20,
            textCapitalization: TextCapitalization.words,
            onSubmitted: (_) => ctrl.submitName(context),
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: Align(
            alignment: Alignment.centerRight,
            child: FilledButton(
              onPressed: ctrl.nameValid ? () => ctrl.submitName(context) : null,
              child: const Text('Next'),
            ),
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step: Lobby
// =============================================================================

class _LobbyView extends StatefulWidget {
  final RemoteKeygenController ctrl;
  const _LobbyView({super.key, required this.ctrl});

  @override
  State<_LobbyView> createState() => _LobbyViewState();
}

class _LobbyViewState extends State<_LobbyView> {
  RemoteKeygenController get ctrl => widget.ctrl;
  final Map<int, TextEditingController> _nameControllers = {};

  @override
  void dispose() {
    for (final c in _nameControllers.values) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final total = ctrl.totalDeviceCount;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 0, 16, 0),
          child: Row(
            children: [
              IconButton(
                icon: const Icon(Icons.arrow_back_rounded),
                onPressed: () => ctrl.back(context),
              ),
              const SizedBox(width: 8),
              Expanded(
                child:
                    Text(ctrl.walletName, style: theme.textTheme.titleLarge),
              ),
            ],
          ),
        ),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
            shrinkWrap: true,
            children: [
              // --- Invite link ---
              Card.outlined(
                child: Padding(
                  padding: const EdgeInsets.all(12),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    spacing: 8,
                    children: [
                      Text('Invite link',
                          style: theme.textTheme.labelLarge?.copyWith(
                              color: theme.colorScheme.onSurfaceVariant)),
                      SelectableText(
                        ctrl.inviteLink,
                        style: theme.textTheme.bodyMedium?.copyWith(
                            fontFamily: 'monospace',
                            color: theme.colorScheme.primary),
                      ),
                      Row(
                        spacing: 8,
                        children: [
                          Expanded(
                            child: FilledButton.tonalIcon(
                              icon: const Icon(Icons.copy_rounded, size: 18),
                              label: const Text('Copy'),
                              onPressed: () {
                                Clipboard.setData(
                                    ClipboardData(text: ctrl.inviteLink));
                                ScaffoldMessenger.of(context).showSnackBar(
                                  const SnackBar(content: Text('Copied')),
                                );
                              },
                            ),
                          ),
                          Expanded(
                            child: FilledButton.tonalIcon(
                              icon: const Icon(Icons.share_rounded, size: 18),
                              label: const Text('Share'),
                              onPressed: () {
                                ScaffoldMessenger.of(context).showSnackBar(
                                  const SnackBar(
                                      content:
                                          Text('Share sheet would open')),
                                );
                              },
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 16),

              // --- Your devices ---
              Text('Your devices',
                  style: theme.textTheme.labelLarge?.copyWith(
                      color: theme.colorScheme.secondary)),
              const SizedBox(height: 4),
              // App key toggle
              Card.outlined(
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(
                    color: ctrl.includeAppKey
                        ? theme.colorScheme.primary
                        : theme.colorScheme.outlineVariant,
                  ),
                ),
                child: SwitchListTile(
                  secondary: Icon(DeviceIdentity.icon,
                      color: ctrl.includeAppKey
                          ? theme.colorScheme.primary
                          : null),
                  title: Text('Include ${DeviceIdentity.name}'),
                  subtitle: const Text('Store a key share on this device'),
                  value: ctrl.includeAppKey,
                  onChanged: (_) => ctrl.toggleAppKey(),
                ),
              ),
              const SizedBox(height: 4),
              ...List.generate(ctrl.localDevices.length, (i) {
                final device = ctrl.localDevices[i];
                _nameControllers.putIfAbsent(
                    i, () => TextEditingController(text: device.name ?? ''));
                return Card.filled(
                  margin: const EdgeInsets.symmetric(vertical: 3),
                  color: theme.colorScheme.surfaceContainerHigh,
                  child: ListTile(
                    leading: const Icon(Icons.key),
                    title: device.isNamed
                        ? Text(device.name!)
                        : SizedBox(
                            height: 40,
                            child: TextField(
                              controller: _nameControllers[i],
                              autofocus: true,
                              decoration: InputDecoration(
                                hintText: 'Name this device',
                                isDense: true,
                                border: OutlineInputBorder(
                                    borderSide: BorderSide.none),
                                filled: true,
                                suffixIcon: IconButton(
                                  icon: const Icon(Icons.check, size: 20),
                                  onPressed: () {
                                    final text =
                                        _nameControllers[i]!.text.trim();
                                    if (text.isNotEmpty) {
                                      ctrl.nameLocalDevice(i, text);
                                    }
                                  },
                                ),
                              ),
                              onSubmitted: (text) {
                                if (text.trim().isNotEmpty) {
                                  ctrl.nameLocalDevice(i, text.trim());
                                }
                              },
                            ),
                          ),
                    trailing: IconButton(
                      icon: const Icon(Icons.remove_circle_outline,
                          color: Colors.red),
                      tooltip: 'Remove',
                      onPressed: () {
                        _nameControllers.remove(i);
                        ctrl.removeLocalDevice(i);
                      },
                    ),
                  ),
                );
              }),
              AnimatedGradientCard(
                child: const ListTile(
                  dense: true,
                  title:
                      Text('Plug in devices to add them to this wallet.'),
                  contentPadding: EdgeInsets.symmetric(horizontal: 16),
                  leading: Icon(Icons.usb_rounded),
                ),
              ),
              const SizedBox(height: 16),

              // --- Other participants ---
              Text('Other participants',
                  style: theme.textTheme.labelLarge?.copyWith(
                      color: theme.colorScheme.secondary)),
              const SizedBox(height: 4),
              if (ctrl.remoteParticipants.isEmpty)
                Card.filled(
                  color: theme.colorScheme.surfaceContainerHigh,
                  child: ListTile(
                    leading: Icon(Icons.hourglass_empty,
                        color: theme.colorScheme.onSurfaceVariant),
                    title: Text('No one has joined yet',
                        style: TextStyle(
                            color: theme.colorScheme.onSurfaceVariant)),
                    subtitle: const Text('Share the invite link above'),
                  ),
                ),
              ...List.generate(ctrl.remoteParticipants.length, (i) {
                final p = ctrl.remoteParticipants[i];
                return Card.filled(
                  margin: const EdgeInsets.symmetric(vertical: 3),
                  color: theme.colorScheme.surfaceContainerHigh,
                  child: ListTile(
                    leading: const Icon(Icons.person_rounded),
                    title: Text(p.displayName),
                    subtitle: p.deviceCount > 1
                        ? Text('${p.deviceCount} devices')
                        : null,
                    trailing: IconButton(
                      icon: const Icon(Icons.remove_circle_outline,
                          color: Colors.red),
                      tooltip: 'Remove',
                      onPressed: () => ctrl.removeParticipant(i),
                    ),
                  ),
                );
              }),
              const SizedBox(height: 20),

              // --- Threshold ---
              if (total >= 2) ...[
                Text('Threshold',
                    style: theme.textTheme.labelLarge?.copyWith(
                        color: theme.colorScheme.secondary)),
                const SizedBox(height: 4),
                Card.filled(
                  color: theme.colorScheme.surfaceContainerHigh,
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                        vertical: 8, horizontal: 4),
                    child: Column(
                      children: [
                        Slider(
                          value: (ctrl.threshold ?? 1)
                              .clamp(1, total)
                              .toDouble(),
                          label: '${ctrl.threshold}',
                          onChanged: (v) => ctrl.setThreshold(v.toInt()),
                          min: 1,
                          max: total.toDouble(),
                          divisions: max(total - 1, 1),
                        ),
                        Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text: '${ctrl.threshold}',
                                style: const TextStyle(
                                  fontWeight: FontWeight.bold,
                                  decoration: TextDecoration.underline,
                                ),
                              ),
                              TextSpan(text: ' of $total required to sign'),
                            ],
                            style: theme.textTheme.titleMedium,
                          ),
                        ),
                        const SizedBox(height: 4),
                      ],
                    ),
                  ),
                ),
              ],
            ],
          ),
        ),
        // Bottom bar
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: FilledButton(
            onPressed:
                ctrl.canStartKeygen ? () => ctrl.startKeygen() : null,
            child: Text(!ctrl.allLocalDevicesNamed && ctrl.localDevices.isNotEmpty
                ? 'Name all devices to continue'
                : total < 2
                    ? 'Add at least 2 devices to continue'
                    : 'Start key generation'),
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step: Review (key share distribution)
// =============================================================================

class _ReviewView extends StatelessWidget {
  final RemoteKeygenController ctrl;
  const _ReviewView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final namedDevices = ctrl.localDevices.where((d) => d.isNamed).toList();
    int shareIndex = 1;

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 0, 16, 0),
          child: Row(
            children: [
              IconButton(
                icon: const Icon(Icons.arrow_back_rounded),
                onPressed: () => ctrl.back(context),
              ),
              const SizedBox(width: 8),
              Text('Key distribution', style: theme.textTheme.titleLarge),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
          child: Text(
            'Review the key share distribution before generating.',
            style: theme.textTheme.titleMedium,
          ),
        ),
        Flexible(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 16),
            shrinkWrap: true,
            children: [
              // You
              Card.filled(
                color: theme.colorScheme.surfaceContainerHigh,
                child: Padding(
                  padding: const EdgeInsets.symmetric(vertical: 8),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      ListTile(
                        dense: true,
                        leading: const Icon(Icons.phone_android_rounded),
                        title: Text('You',
                            style: theme.textTheme.titleSmall),
                      ),
                      if (ctrl.includeAppKey)
                        ListTile(
                          dense: true,
                          contentPadding:
                              const EdgeInsets.only(left: 56, right: 16),
                          leading: Icon(DeviceIdentity.icon, size: 18),
                          title: Text(DeviceIdentity.name),
                          trailing: Text('Share ${shareIndex++}',
                              style: theme.textTheme.bodySmall?.copyWith(
                                  color:
                                      theme.colorScheme.onSurfaceVariant)),
                        ),
                      ...namedDevices.map((d) {
                        final idx = shareIndex++;
                        return ListTile(
                          dense: true,
                          contentPadding:
                              const EdgeInsets.only(left: 56, right: 16),
                          leading: const Icon(Icons.key, size: 18),
                          title: Text(d.name!),
                          trailing: Text('Share $idx',
                              style: theme.textTheme.bodySmall?.copyWith(
                                  color:
                                      theme.colorScheme.onSurfaceVariant)),
                        );
                      }),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 8),
              // Remote participants
              ...ctrl.remoteParticipants.map((p) {
                final startShare = shareIndex;
                shareIndex += p.deviceCount;
                final endShare = shareIndex - 1;
                final shareLabel = p.deviceCount == 1
                    ? 'Share $startShare'
                    : 'Shares $startShare\u2013$endShare';
                return Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Card.filled(
                    color: theme.colorScheme.surfaceContainerHigh,
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8),
                      child: ListTile(
                        dense: true,
                        leading: const Icon(Icons.person_rounded),
                        title: Text(p.displayName,
                            style: theme.textTheme.titleSmall),
                        subtitle: p.deviceCount > 1
                            ? Text('${p.deviceCount} devices')
                            : null,
                        trailing: Text(shareLabel,
                            style: theme.textTheme.bodySmall?.copyWith(
                                color:
                                    theme.colorScheme.onSurfaceVariant)),
                      ),
                    ),
                  ),
                );
              }),
              const SizedBox(height: 12),
              // Summary
              Card.filled(
                color: theme.colorScheme.primaryContainer,
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text('Threshold',
                          style: theme.textTheme.titleSmall?.copyWith(
                              color:
                                  theme.colorScheme.onPrimaryContainer)),
                      Text(
                        '${ctrl.threshold} of ${ctrl.totalDeviceCount}',
                        style: theme.textTheme.titleMedium?.copyWith(
                            fontWeight: FontWeight.bold,
                            color:
                                theme.colorScheme.onPrimaryContainer),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
        const Divider(height: 0),
        Padding(
          padding: const EdgeInsets.all(16),
          child: FilledButton(
            onPressed: () => ctrl.confirmAndGenerate(context),
            child: const Text('Confirm and generate keys'),
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Step: Done
// =============================================================================

class _DoneView extends StatelessWidget {
  final RemoteKeygenController ctrl;
  const _DoneView({super.key, required this.ctrl});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.all(32),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Icon(Icons.check_circle, size: 64, color: Colors.green),
          const SizedBox(height: 16),
          Text('Wallet "${ctrl.walletName}" created!',
              style: theme.textTheme.headlineSmall,
              textAlign: TextAlign.center),
          const SizedBox(height: 8),
          Text(
            '${ctrl.threshold}-of-${ctrl.totalDeviceCount} across ${ctrl.remoteParticipants.length + 1} participants',
            style: theme.textTheme.bodyLarge?.copyWith(
                color: theme.colorScheme.onSurfaceVariant),
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
// Wallet type card
// =============================================================================

class _WalletTypeCard extends StatelessWidget {
  final IconData icon;
  final String title;
  final String subtitle;
  final bool emphasized;
  final VoidCallback onTap;

  const _WalletTypeCard({
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
