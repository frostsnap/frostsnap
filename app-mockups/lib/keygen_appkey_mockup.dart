import 'dart:math';
import 'package:flutter/material.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'device_identity.dart';
import 'keygen_mockup.dart' show LargeCircularProgressIndicator, AnimatedGradientCard;

// Re-export for other files that still import from here
String appKeyLabel(BuildContext context) => DeviceIdentity.name;
IconData appKeyIcon(BuildContext context) => DeviceIdentity.icon;

// =============================================================================
// Steps
// =============================================================================

enum AppKeyKeygenStep { name, pickDevices, nameDevices, threshold, generating, done }

// =============================================================================
// Controller — like KeygenController but with an App Key option
// =============================================================================

class AppKeyKeygenController extends ChangeNotifier {
  AppKeyKeygenStep _step = AppKeyKeygenStep.name;
  AppKeyKeygenStep get step => _step;

  final nameController = TextEditingController();
  String? nameError;

  // Physical devices "plugged in"
  final int connectedDeviceCount = 2;

  // App Key toggle
  bool includeAppKey = false;

  final Map<int, String> deviceNames = {};

  int? threshold;

  // keygen progress
  final Set<int> ackedDevices = {};
  bool appKeyAcked = false;
  int get acksReceived => ackedDevices.length + (appKeyAcked ? 1 : 0);
  String sessionHash = 'A3 F7 1B 9C';
  BuildContext? _keygenContext;

  // --- Derived ---

  String get walletName => nameController.text.trim();
  bool get nameValid => walletName.isNotEmpty && walletName.length <= 20;

  int get totalDeviceCount => connectedDeviceCount + (includeAppKey ? 1 : 0);

  bool get allDevicesNamed {
    return List.generate(connectedDeviceCount, (i) => i)
        .every((i) => (deviceNames[i] ?? '').trim().isNotEmpty);
  }

  bool get canGoNext => switch (_step) {
        AppKeyKeygenStep.name => nameValid,
        AppKeyKeygenStep.pickDevices => totalDeviceCount > 0,
        AppKeyKeygenStep.nameDevices => allDevicesNamed,
        AppKeyKeygenStep.threshold =>
          threshold != null && threshold! >= 1 && threshold! <= totalDeviceCount,
        AppKeyKeygenStep.generating => false,
        AppKeyKeygenStep.done => false,
      };

  String get title => switch (_step) {
        AppKeyKeygenStep.name => 'Name wallet',
        AppKeyKeygenStep.pickDevices => 'Pick devices',
        AppKeyKeygenStep.nameDevices => 'Name devices',
        AppKeyKeygenStep.threshold => 'Choose threshold',
        AppKeyKeygenStep.generating => 'Security Check',
        AppKeyKeygenStep.done => 'Done',
      };

  String get subtitle => switch (_step) {
        AppKeyKeygenStep.name => 'Choose a name for this wallet',
        AppKeyKeygenStep.pickDevices =>
          'Connect devices to become keys for "$walletName"',
        AppKeyKeygenStep.nameDevices => 'Each device needs a name to identify it',
        AppKeyKeygenStep.threshold =>
          'Decide how many devices will be required to sign transactions',
        AppKeyKeygenStep.generating =>
          'Confirm that this code is shown on all devices',
        AppKeyKeygenStep.done => '',
      };

  String? get nextText => switch (_step) {
        AppKeyKeygenStep.name => 'Next',
        AppKeyKeygenStep.pickDevices => totalDeviceCount == 0
            ? 'Add devices'
            : 'Continue with $totalDeviceCount device${totalDeviceCount > 1 ? 's' : ''}',
        AppKeyKeygenStep.nameDevices =>
          allDevicesNamed ? 'Next' : 'Name all devices to continue',
        AppKeyKeygenStep.threshold => 'Generate keys',
        AppKeyKeygenStep.generating => null,
        AppKeyKeygenStep.done => null,
      };

  void next(BuildContext context) {
    if (!canGoNext) return;
    switch (_step) {
      case AppKeyKeygenStep.name:
        _step = AppKeyKeygenStep.pickDevices;
      case AppKeyKeygenStep.pickDevices:
        threshold = max((totalDeviceCount * 2 / 3).toInt(), 1);
        _step = AppKeyKeygenStep.nameDevices;
      case AppKeyKeygenStep.nameDevices:
        _step = AppKeyKeygenStep.threshold;
      case AppKeyKeygenStep.threshold:
        _step = AppKeyKeygenStep.generating;
        ackedDevices.clear();
        appKeyAcked = includeAppKey; // auto-ack immediately
        _keygenContext = context;
      case AppKeyKeygenStep.generating:
      case AppKeyKeygenStep.done:
        break;
    }
    notifyListeners();
  }

  void back(BuildContext context) {
    switch (_step) {
      case AppKeyKeygenStep.name:
        Navigator.pop(context);
        return;
      case AppKeyKeygenStep.pickDevices:
        _step = AppKeyKeygenStep.name;
      case AppKeyKeygenStep.nameDevices:
        _step = AppKeyKeygenStep.pickDevices;
      case AppKeyKeygenStep.threshold:
        _step = AppKeyKeygenStep.nameDevices;
      case AppKeyKeygenStep.generating:
        _step = AppKeyKeygenStep.threshold;
        ackedDevices.clear();
        appKeyAcked = false;
      case AppKeyKeygenStep.done:
        return;
    }
    notifyListeners();
  }

  void toggleAppKey() {
    includeAppKey = !includeAppKey;
    notifyListeners();
  }

  void setDeviceName(int index, String name) {
    deviceNames[index] = name;
    notifyListeners();
  }

  void ackDevice(int index) {
    if (_step != AppKeyKeygenStep.generating) return;
    if (ackedDevices.contains(index)) return;
    ackedDevices.add(index);
    notifyListeners();
    _checkAllAcked();
  }

  void ackAppKey() {
    if (_step != AppKeyKeygenStep.generating) return;
    if (appKeyAcked) return;
    appKeyAcked = true;
    notifyListeners();
    _checkAllAcked();
  }

  void _checkAllAcked() async {
    if (acksReceived < totalDeviceCount) return;

    final context = _keygenContext;
    if (context == null || !context.mounted) return;

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
                  Text(includeAppKey
                      ? 'Do all hardware devices show this code?'
                      : 'Do all devices show this code?'),
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
      _step = AppKeyKeygenStep.done;
    } else {
      _step = AppKeyKeygenStep.threshold;
      ackedDevices.clear();
      appKeyAcked = false;
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
// Page
// =============================================================================

class AppKeyKeygenPage extends StatefulWidget {
  final AppKeyKeygenController controller;

  const AppKeyKeygenPage({super.key, required this.controller});

  @override
  State<AppKeyKeygenPage> createState() => _AppKeyKeygenPageState();
}

class _AppKeyKeygenPageState extends State<AppKeyKeygenPage> {
  AppKeyKeygenController get _ctrl => widget.controller;

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
    final theme = Theme.of(context);

    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _ctrl.back(context);
      },
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Flexible(
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
              child: CustomScrollView(
                key: ValueKey<AppKeyKeygenStep>(_ctrl.step),
                physics: const ClampingScrollPhysics(),
                shrinkWrap: true,
                slivers: [
                  SliverToBoxAdapter(
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(4, 0, 16, 0),
                      child: Row(
                        children: [
                          IconButton(
                            icon: const Icon(Icons.arrow_back_rounded),
                            onPressed: () => _ctrl.back(context),
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(_ctrl.title,
                                style: theme.textTheme.titleLarge),
                          ),
                        ],
                      ),
                    ),
                  ),
                  if (_ctrl.subtitle.isNotEmpty)
                    SliverToBoxAdapter(
                      child: Padding(
                        padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
                        child: Text(_ctrl.subtitle,
                            style: theme.textTheme.titleMedium),
                      ),
                    ),
                  SliverPadding(
                    padding: const EdgeInsets.fromLTRB(16, 16, 16, 24),
                    sliver: _buildBody(context),
                  ),
                  const SliverPadding(padding: EdgeInsets.only(bottom: 32)),
                ],
              ),
            ),
          ),
          if (_ctrl.nextText != null) ...[
            const Divider(height: 0),
            Padding(
              padding: const EdgeInsets.all(16),
              child: Align(
                alignment: Alignment.centerRight,
                child: FilledButton(
                  onPressed:
                      _ctrl.canGoNext ? () => _ctrl.next(context) : null,
                  child: Text(_ctrl.nextText!,
                      softWrap: false, overflow: TextOverflow.fade),
                ),
              ),
            ),
          ],
          if (_ctrl.step == AppKeyKeygenStep.done)
            Padding(
              padding: const EdgeInsets.all(16),
              child: FilledButton(
                onPressed: () => Navigator.pop(context),
                child: const Text('Done'),
              ),
            ),
        ],
      ),
    );
  }

  Widget _buildBody(BuildContext context) {
    switch (_ctrl.step) {
      case AppKeyKeygenStep.name:
        return _buildNameStep(context);
      case AppKeyKeygenStep.pickDevices:
        return _buildPickDevicesStep(context);
      case AppKeyKeygenStep.nameDevices:
        return _buildNameDevicesStep(context);
      case AppKeyKeygenStep.threshold:
        return _buildThresholdStep(context);
      case AppKeyKeygenStep.generating:
        return const SliverToBoxAdapter(child: SizedBox());
      case AppKeyKeygenStep.done:
        return _buildDoneStep(context);
    }
  }

  Widget _buildNameStep(BuildContext context) {
    return SliverToBoxAdapter(
      child: TextField(
        autofocus: true,
        controller: _ctrl.nameController,
        decoration: InputDecoration(
          border: const OutlineInputBorder(),
          errorText: _ctrl.nameError,
        ),
        maxLength: 20,
        textCapitalization: TextCapitalization.words,
        onSubmitted: (_) => _ctrl.next(context),
      ),
    );
  }

  Widget _buildPickDevicesStep(BuildContext context) {
    final theme = Theme.of(context);
    return SliverList.list(
      children: [
        // Physical devices
        ...List.generate(_ctrl.connectedDeviceCount, (i) {
          return Card.filled(
            margin: const EdgeInsets.symmetric(vertical: 4),
            color: theme.colorScheme.surfaceContainerHigh,
            child: ListTile(
              leading: const Icon(Icons.key),
              title: Text('Device ${i + 1}'),
              trailing: Row(
                mainAxisSize: MainAxisSize.min,
                spacing: 8,
                children: [
                  Text('Ready',
                      style: theme.textTheme.titleSmall
                          ?.copyWith(color: Colors.green)),
                  Icon(Icons.check_circle_rounded, color: Colors.green),
                ],
              ),
            ),
          );
        }),

        // App Key toggle
        const SizedBox(height: 8),
        Card.outlined(
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
            side: BorderSide(
              color: _ctrl.includeAppKey
                  ? theme.colorScheme.primary
                  : theme.colorScheme.outlineVariant,
            ),
          ),
          child: SwitchListTile(
            secondary: Icon(appKeyIcon(context),
                color: _ctrl.includeAppKey
                    ? theme.colorScheme.primary
                    : null),
            title: Text('Include ${appKeyLabel(context)}'),
            subtitle: Text('Store a key share on ${appKeyLabel(context).toLowerCase()}'),
            value: _ctrl.includeAppKey,
            onChanged: (_) => _ctrl.toggleAppKey(),
          ),
        ),

        const SizedBox(height: 8),
        AnimatedGradientCard(
          child: ListTile(
            dense: true,
            title: const Text(
                'Plug in devices to include them in this wallet.'),
            contentPadding: const EdgeInsets.symmetric(horizontal: 16),
            leading: const Icon(Icons.info_rounded),
          ),
        ),
      ],
    );
  }

  final Map<int, TextEditingController> _nameControllers = {};

  Widget _buildNameDevicesStep(BuildContext context) {
    final theme = Theme.of(context);
    final indices = List.generate(_ctrl.connectedDeviceCount, (i) => i);

    return SliverList.list(
      children: [
        ...indices.map((i) {
          _nameControllers.putIfAbsent(
            i,
            () => TextEditingController(text: _ctrl.deviceNames[i] ?? ''),
          );
          return Card.filled(
            margin: const EdgeInsets.symmetric(vertical: 4),
            color: theme.colorScheme.surface,
            child: ListTile(
              leading: const Icon(Icons.key),
              contentPadding: const EdgeInsets.symmetric(horizontal: 12),
              title: TextField(
                decoration: InputDecoration(
                  hintText: 'Enter device name',
                  border: OutlineInputBorder(borderSide: BorderSide.none),
                  suffixIcon: const Icon(Icons.edit_rounded),
                  filled: true,
                ),
                controller: _nameControllers[i],
                onChanged: (name) => _ctrl.setDeviceName(i, name),
              ),
            ),
          );
        }),

        // App Key — no name needed, just shown as a fixed label
        if (_ctrl.includeAppKey)
          Card.filled(
            margin: const EdgeInsets.symmetric(vertical: 4),
            color: theme.colorScheme.surface,
            child: ListTile(
              leading: Icon(appKeyIcon(context), color: theme.colorScheme.primary),
              title: Text(appKeyLabel(context)),
            ),
          ),
      ],
    );
  }

  Widget _buildThresholdStep(BuildContext context) {
    final theme = Theme.of(context);
    final total = _ctrl.totalDeviceCount;
    return SliverList.list(
      children: [
        if (total > 1)
          Slider(
            value: (_ctrl.threshold ?? 1).toDouble(),
            label: '${_ctrl.threshold}',
            onChanged: (v) {
              _ctrl.threshold = v.toInt();
              _ctrl.notifyListeners();
            },
            min: 1,
            max: total.toDouble(),
            divisions: max(total - 1, 1),
          ),
        Center(
          child: Card.filled(
            child: Padding(
              padding:
                  const EdgeInsets.symmetric(vertical: 12, horizontal: 16),
              child: Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text: '${_ctrl.threshold}',
                      style: const TextStyle(
                        decoration: TextDecoration.underline,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    TextSpan(text: ' of $total'),
                  ],
                  style: theme.textTheme.headlineSmall,
                ),
              ),
            ),
          ),
        ),
        if (_ctrl.includeAppKey)
          Padding(
            padding: const EdgeInsets.only(top: 16),
            child: Card.outlined(
              child: ListTile(
                dense: true,
                leading: Icon(Icons.info_outline,
                    color: theme.colorScheme.primary),
                title: Text(
                  'An App Key is convenient but less secure than a hardware device. '
                  'If your phone is compromised, this key share could be exposed.',
                  style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant),
                ),
              ),
            ),
          ),
      ],
    );
  }

  Widget _buildDoneStep(BuildContext context) {
    final theme = Theme.of(context);
    return SliverList.list(
      children: [
        Center(
          child: Column(
            spacing: 16,
            children: [
              Icon(Icons.check_circle, size: 64, color: Colors.green),
              Text('Wallet "${_ctrl.walletName}" created!',
                  style: theme.textTheme.headlineSmall),
              if (_ctrl.includeAppKey)
                Chip(
                  avatar: Icon(appKeyIcon(context), size: 18),
                  label: Text('Includes ${appKeyLabel(context).toLowerCase()}'),
                ),
            ],
          ),
        ),
      ],
    );
  }
}
