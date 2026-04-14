import 'dart:math';
import 'package:flutter/material.dart';
import 'package:glowy_borders/glowy_borders.dart';

const seedColor = Color(0xFF1595B2);

// =============================================================================
// Steps
// =============================================================================

enum KeygenStep { name, pickDevices, nameDevices, threshold, generating, done }

// =============================================================================
// Controller
// =============================================================================

class KeygenController extends ChangeNotifier {
  KeygenStep _step = KeygenStep.name;
  KeygenStep get step => _step;

  final nameController = TextEditingController();
  String? nameError;

  // Devices that are "plugged in" — in the real app these appear dynamically
  // via SliverDeviceList as you plug in USB devices. Here we just pretend 3
  // are already connected.
  final int connectedDeviceCount = 3;

  // All connected devices are automatically selected — no checkbox picking.
  final Map<int, String> deviceNames = {};

  int? threshold;

  // keygen progress
  final Set<int> ackedDevices = {};
  int get acksReceived => ackedDevices.length;
  String sessionHash = 'A3 F7 1B 9C';
  BuildContext? _keygenContext;

  // --- Derived ---

  String get walletName => nameController.text.trim();
  bool get nameValid => walletName.isNotEmpty && walletName.length <= 20;

  int get deviceCount => connectedDeviceCount;
  bool get allDevicesNamed => List.generate(deviceCount, (i) => i)
      .every((i) => (deviceNames[i] ?? '').trim().isNotEmpty);

  bool get canGoNext => switch (_step) {
        KeygenStep.name => nameValid,
        KeygenStep.pickDevices => connectedDeviceCount > 0,
        KeygenStep.nameDevices => allDevicesNamed,
        KeygenStep.threshold =>
          threshold != null && threshold! >= 1 && threshold! <= deviceCount,
        KeygenStep.generating => false,
        KeygenStep.done => false,
      };

  String get title => switch (_step) {
        KeygenStep.name => 'Name wallet',
        KeygenStep.pickDevices => 'Pick devices',
        KeygenStep.nameDevices => 'Name devices',
        KeygenStep.threshold => 'Choose threshold',
        KeygenStep.generating => 'Security Check',
        KeygenStep.done => 'Done',
      };

  String get subtitle => switch (_step) {
        KeygenStep.name => 'Choose a name for this wallet',
        KeygenStep.pickDevices =>
          'Select devices to become keys for "$walletName"',
        KeygenStep.nameDevices => 'Each device needs a name to identify it',
        KeygenStep.threshold =>
          'Decide how many devices will be required to sign transactions',
        KeygenStep.generating => 'Confirm that this code is shown on all devices',
        KeygenStep.done => '',
      };

  String? get nextText => switch (_step) {
        KeygenStep.name => 'Next',
        KeygenStep.pickDevices => connectedDeviceCount == 0
            ? 'Plug in devices'
            : connectedDeviceCount == 1
                ? 'Continue with 1 device'
                : 'Continue with $connectedDeviceCount devices',
        KeygenStep.nameDevices => allDevicesNamed ? 'Next' : 'Name all devices to continue',
        KeygenStep.threshold => 'Generate keys',
        KeygenStep.generating => null,
        KeygenStep.done => null,
      };

  void next(BuildContext context) {
    if (!canGoNext) return;
    switch (_step) {
      case KeygenStep.name:
        _step = KeygenStep.pickDevices;
      case KeygenStep.pickDevices:
        // Set default threshold
        threshold = max((deviceCount * 2 / 3).toInt(), 1);
        _step = KeygenStep.nameDevices;
      case KeygenStep.nameDevices:
        _step = KeygenStep.threshold;
      case KeygenStep.threshold:
        _step = KeygenStep.generating;
        ackedDevices.clear();
        _keygenContext = context;
      case KeygenStep.generating:
      case KeygenStep.done:
        break;
    }
    notifyListeners();
  }

  void back(BuildContext context) {
    switch (_step) {
      case KeygenStep.name:
        Navigator.pop(context);
        return;
      case KeygenStep.pickDevices:
        _step = KeygenStep.name;
      case KeygenStep.nameDevices:
        _step = KeygenStep.pickDevices;
      case KeygenStep.threshold:
        _step = KeygenStep.nameDevices;
      case KeygenStep.generating:
        // Can't go back during generation
        return;
      case KeygenStep.done:
        return;
    }
    notifyListeners();
  }

  void setDeviceName(int index, String name) {
    deviceNames[index] = name;
    notifyListeners();
  }

  void ackDevice(int index) async {
    if (_step != KeygenStep.generating) return;
    if (ackedDevices.contains(index)) return;
    ackedDevices.add(index);
    notifyListeners();

    if (acksReceived >= deviceCount) {
      // All devices confirmed — show final check dialog
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
                      const Text('Do all devices show this code?'),
                      Card.filled(
                        child: Center(
                          child: Padding(
                            padding: const EdgeInsets.symmetric(
                                vertical: 12, horizontal: 16),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              children: [
                                Text('$threshold-of-$deviceCount',
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
          _step = KeygenStep.done;
        } else {
          _step = KeygenStep.threshold;
          ackedDevices.clear();
        }
        notifyListeners();
      }
    }
  }

  @override
  void dispose() {
    nameController.dispose();
    super.dispose();
  }
}

// =============================================================================
// Animated progress indicator (from real app's LargeCircularProgressIndicator)
// =============================================================================

class LargeCircularProgressIndicator extends StatefulWidget {
  final int progress;
  final int total;
  final double size;

  const LargeCircularProgressIndicator({
    super.key,
    required this.progress,
    required this.total,
    this.size = 70,
  });

  @override
  State<LargeCircularProgressIndicator> createState() =>
      _LargeCircularProgressIndicatorState();
}

class _LargeCircularProgressIndicatorState
    extends State<LargeCircularProgressIndicator>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;
  late Animation<double> _animation;
  double _oldFraction = 0;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 600),
    );
    _initAnimation();
  }

  void _initAnimation() {
    final newFraction = widget.total == 0
        ? 0.0
        : (widget.progress / widget.total).clamp(0.0, 1.0);
    _animation = Tween<double>(begin: _oldFraction, end: newFraction).animate(
      CurvedAnimation(parent: _controller, curve: Curves.easeOutCubic),
    )..addListener(() => setState(() {}));
    _controller.forward(from: 0);
    _oldFraction = newFraction;
  }

  @override
  void didUpdateWidget(covariant LargeCircularProgressIndicator oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.progress != widget.progress ||
        oldWidget.total != widget.total) {
      _initAnimation();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final complete = widget.total > 0 && widget.progress >= widget.total;
    final fraction = complete ? 1.0 : _animation.value;

    return UnconstrainedBox(
      child: SizedBox.square(
        dimension: widget.size,
        child: Stack(
          alignment: Alignment.center,
          children: [
            AspectRatio(
              aspectRatio: 1,
              child: CircularProgressIndicator(
                value: fraction,
                strokeWidth: widget.size * 0.07,
                backgroundColor: cs.surfaceContainerHighest,
                color: cs.primary,
              ),
            ),
            complete
                ? Icon(Icons.check, size: widget.size * 0.5, color: cs.primary)
                : SizedBox(
                    width: widget.size * 0.6,
                    height: widget.size * 0.6,
                    child: FittedBox(
                      fit: BoxFit.scaleDown,
                      child: Text(
                        '${widget.progress}/${widget.total}',
                        style: Theme.of(context).textTheme.titleLarge,
                        textAlign: TextAlign.center,
                      ),
                    ),
                  ),
          ],
        ),
      ),
    );
  }
}

// =============================================================================
// Main page
// =============================================================================

class KeygenMockupPage extends StatefulWidget {
  final KeygenController controller;

  const KeygenMockupPage({super.key, required this.controller});

  @override
  State<KeygenMockupPage> createState() => _KeygenMockupPageState();
}

class _KeygenMockupPageState extends State<KeygenMockupPage> {
  KeygenController get _ctrl => widget.controller;

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
                key: ValueKey<KeygenStep>(_ctrl.step),
                physics: const ClampingScrollPhysics(),
                shrinkWrap: true,
                slivers: [
                  // Header
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
          // Bottom bar with Next button
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
          if (_ctrl.step == KeygenStep.done)
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
      case KeygenStep.name:
        return _buildNameStep(context);
      case KeygenStep.pickDevices:
        return _buildPickDevicesStep(context);
      case KeygenStep.nameDevices:
        return _buildNameDevicesStep(context);
      case KeygenStep.threshold:
        return _buildThresholdStep(context);
      case KeygenStep.generating:
        // Shown as fullscreen overlay from the scaffold, not inline
        return const SliverToBoxAdapter(child: SizedBox());
      case KeygenStep.done:
        return _buildDoneStep(context);
    }
  }

  // --- Step: Name wallet ---

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

  // --- Step: Pick devices ---

  Widget _buildPickDevicesStep(BuildContext context) {
    final theme = Theme.of(context);
    return SliverList.list(
      children: [
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

  // --- Step: Name devices ---

  final Map<int, TextEditingController> _nameControllers = {};

  Widget _buildNameDevicesStep(BuildContext context) {
    final theme = Theme.of(context);
    final indices = List.generate(_ctrl.connectedDeviceCount, (i) => i);
    return SliverList.list(
      children: indices.map((i) {
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
      }).toList(),
    );
  }

  // --- Step: Threshold ---

  Widget _buildThresholdStep(BuildContext context) {
    final theme = Theme.of(context);
    final total = _ctrl.deviceCount;
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
      ],
    );
  }

  // --- Step: Generating (security check) ---

  Widget _buildGeneratingStep(BuildContext context) {
    final theme = Theme.of(context);
    return SliverList.list(
      children: [
        Center(
          child: Card.filled(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                spacing: 12,
                children: [
                  Text('${_ctrl.threshold}-of-${_ctrl.deviceCount}',
                      style: theme.textTheme.labelLarge),
                  Text(_ctrl.sessionHash,
                      style: theme.textTheme.headlineLarge),
                ],
              ),
            ),
          ),
        ),
        const SizedBox(height: 24),
        Center(
          child: Row(
            mainAxisSize: MainAxisSize.min,
            spacing: 12,
            children: [
              Text('Confirm on device',
                  style: theme.textTheme.labelMedium?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant)),
              LargeCircularProgressIndicator(
                size: 36,
                progress: _ctrl.acksReceived,
                total: _ctrl.deviceCount,
              ),
            ],
          ),
        ),
      ],
    );
  }

  // --- Step: Done ---

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
            ],
          ),
        ),
      ],
    );
  }
}

// =============================================================================
// Animated gradient card (standalone copy)
// =============================================================================

class AnimatedGradientCard extends StatelessWidget {
  final Widget child;

  const AnimatedGradientCard({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedGradientBorder(
      stretchAlongAxis: true,
      borderSize: 1.0,
      glowSize: 4.0,
      animationTime: 6,
      borderRadius: BorderRadius.circular(12.0),
      gradientColors: [
        theme.colorScheme.outlineVariant,
        theme.colorScheme.primary,
        theme.colorScheme.secondary,
        theme.colorScheme.tertiary,
      ],
      child: Card(margin: EdgeInsets.zero, child: child),
    );
  }
}
