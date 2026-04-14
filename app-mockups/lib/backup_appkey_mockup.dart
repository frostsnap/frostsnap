import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'main.dart' show deviceSvg;
import 'keygen_appkey_mockup.dart' show appKeyIcon, appKeyLabel;

// =============================================================================
// Data
// =============================================================================

class BackupDevice {
  final int shareIndex;
  final String name;
  final bool isAppKey;
  bool backedUp;

  BackupDevice({
    required this.shareIndex,
    required this.name,
    this.isAppKey = false,
    this.backedUp = false,
  });
}

class BackupController extends ChangeNotifier {
  final String walletName;
  final int threshold;
  final List<BackupDevice> devices;

  int? connectedPhysicalIndex;
  BackupDevice? activePhysicalBackup;

  BackupController({
    this.walletName = 'My Wallet',
    this.threshold = 2,
    required this.devices,
  });

  BackupDevice? get connectedPhysical => connectedPhysicalIndex != null
      ? physicalDevices[connectedPhysicalIndex!]
      : null;

  List<BackupDevice> get physicalDevices =>
      devices.where((d) => !d.isAppKey).toList();

  BackupDevice? get appKeyDevice =>
      devices.where((d) => d.isAppKey).firstOrNull;

  int get completedCount => devices.where((d) => d.backedUp).length;
  bool get allComplete => completedCount == devices.length;

  void connectPhysical(int index) {
    connectedPhysicalIndex = index;
    notifyListeners();
  }

  void disconnectPhysical() {
    connectedPhysicalIndex = null;
    notifyListeners();
  }

  void markBackedUp(BackupDevice device) {
    device.backedUp = true;
    notifyListeners();
  }

  void startPhysicalBackup(BackupDevice device) {
    activePhysicalBackup = device;
    notifyListeners();
  }

  void finishPhysicalBackup() {
    if (activePhysicalBackup != null) {
      activePhysicalBackup!.backedUp = true;
      activePhysicalBackup = null;
    }
    notifyListeners();
  }

  void cancelPhysicalBackup() {
    activePhysicalBackup = null;
    notifyListeners();
  }

  void reset() {
    for (final d in devices) {
      d.backedUp = false;
    }
    connectedPhysicalIndex = null;
    activePhysicalBackup = null;
    notifyListeners();
  }
}

// =============================================================================
// Scaffold
// =============================================================================

class BackupAppKeyScaffold extends StatefulWidget {
  const BackupAppKeyScaffold({super.key});

  @override
  State<BackupAppKeyScaffold> createState() => _BackupAppKeyScaffoldState();
}

class _BackupAppKeyScaffoldState extends State<BackupAppKeyScaffold> {
  final _ctrl = BackupController(
    walletName: 'My Wallet',
    threshold: 2,
    devices: [
      BackupDevice(shareIndex: 0, name: 'Living Room Device'),
      BackupDevice(shareIndex: 1, name: 'Office Safe'),
      BackupDevice(shareIndex: 2, name: '', isAppKey: true),
    ],
  );

  bool _simCollapsed = false;
  Offset _simOffset = const Offset(16, 16);

  @override
  void initState() {
    super.initState();
    _ctrl.addListener(() => mounted ? setState(() {}) : null);
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

    return Scaffold(
      backgroundColor: theme.colorScheme.surfaceContainerLowest,
      body: ListenableBuilder(
        listenable: _ctrl,
        builder: (context, _) => Stack(
          children: [
            Center(
              child: Dialog(
                backgroundColor: backgroundColor,
                clipBehavior: Clip.hardEdge,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 580),
                  child: BackupAppKeyPage(controller: _ctrl),
                ),
              ),
            ),
            if (_ctrl.activePhysicalBackup != null)
              _buildFullscreenBackupOverlay(context),
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
      ),
    );
  }

  Widget _buildFullscreenBackupOverlay(BuildContext context) {
    final theme = Theme.of(context);

    return Scaffold(
      backgroundColor: Colors.black,
      body: Center(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16.0),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SvgPicture.string(
                deviceSvg,
                width: 162,
                height: 134,
                colorFilter: ColorFilter.mode(
                  theme.colorScheme.onSurface,
                  BlendMode.srcATop,
                ),
              ),
              const SizedBox(height: 32),
              Text('Record key backup',
                  style: theme.textTheme.headlineSmall
                      ?.copyWith(color: Colors.white)),
              const SizedBox(height: 24),
              Card(
                margin: EdgeInsets.zero,
                child: Padding(
                  padding: const EdgeInsets.all(16.0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        'The device is displaying the key backup. Write down the:',
                        style: theme.textTheme.bodyLarge,
                      ),
                      const SizedBox(height: 16),
                      Text('1. Key number', style: theme.textTheme.bodyLarge),
                      const SizedBox(height: 8),
                      Text('2. All 25 words in order',
                          style: theme.textTheme.bodyLarge),
                      const SizedBox(height: 16),
                      Container(
                        padding: const EdgeInsets.all(12),
                        decoration: BoxDecoration(
                          color: theme.colorScheme.errorContainer,
                          borderRadius: BorderRadius.circular(8),
                        ),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Icon(Icons.warning_amber_rounded,
                                color: theme.colorScheme.error),
                            const SizedBox(width: 12),
                            Expanded(
                              child: Text(
                                'This key backup is secret information. Anyone with access to ${_ctrl.threshold} of the ${_ctrl.devices.length} keys can steal all your bitcoin.',
                                style: theme.textTheme.bodyMedium?.copyWith(
                                  color: theme.colorScheme.onErrorContainer,
                                  fontWeight: FontWeight.bold,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
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
              onPressed: () => _ctrl.cancelPhysicalBackup(),
              child: const Text('Cancel'),
            ),
            Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 8,
              children: [
                Icon(Icons.edit_note,
                    color: theme.colorScheme.onSurfaceVariant),
                Text('Write down backup',
                    style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant)),
              ],
            ),
          ],
        ),
      ],
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
                Icon(Icons.science, size: 18,
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
        width: 280,
        child: ListenableBuilder(
          listenable: _ctrl,
          builder: (context, _) => Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 8, 4, 0),
                child: Row(
                  children: [
                    Icon(Icons.science, size: 16,
                        color: theme.colorScheme.onSurfaceVariant),
                    const SizedBox(width: 8),
                    Text('Simulate USB',
                        style: theme.textTheme.labelLarge),
                    const Spacer(),
                    IconButton(
                      icon: const Icon(Icons.refresh, size: 18),
                      tooltip: 'Reset',
                      onPressed: _ctrl.reset,
                      visualDensity: VisualDensity.compact,
                    ),
                    IconButton(
                      icon: const Icon(Icons.minimize, size: 18),
                      tooltip: 'Collapse',
                      onPressed: () => setState(() => _simCollapsed = true),
                      visualDensity: VisualDensity.compact,
                    ),
                  ],
                ),
              ),
              ..._ctrl.physicalDevices.asMap().entries.map((entry) {
                final i = entry.key;
                final d = entry.value;
                final isConnected = _ctrl.connectedPhysicalIndex == i;
                return ListTile(
                  dense: true,
                  leading: Icon(
                    isConnected ? Icons.usb : Icons.usb_off, size: 20),
                  title: Text(d.name),
                  trailing: FilterChip(
                    label: Text(isConnected ? 'Connected' : 'Disconnected'),
                    selected: isConnected,
                    onSelected: (_) => isConnected
                        ? _ctrl.disconnectPhysical()
                        : _ctrl.connectPhysical(i),
                    visualDensity: VisualDensity.compact,
                  ),
                );
              }),
              if (_ctrl.activePhysicalBackup != null)
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
                  child: FilledButton(
                    onPressed: () => _ctrl.finishPhysicalBackup(),
                    child: const Text('Confirm backup written'),
                  ),
                )
              else
                const SizedBox(height: 8),
            ],
          ),
        ),
      ),
    );
  }
}

// =============================================================================
// Page — redesigned: one card per key
// =============================================================================

class BackupAppKeyPage extends StatefulWidget {
  final BackupController controller;

  const BackupAppKeyPage({super.key, required this.controller});

  @override
  State<BackupAppKeyPage> createState() => _BackupAppKeyPageState();
}

class _BackupAppKeyPageState extends State<BackupAppKeyPage> {
  BackupController get _ctrl => widget.controller;

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

    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        // Header
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 8, 16, 0),
          child: Row(
            children: [
              IconButton(
                icon: const Icon(Icons.arrow_back_rounded),
                onPressed: () => Navigator.pop(context),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text('Backup keys', style: theme.textTheme.titleLarge),
              ),
              // Progress chip
              Container(
                padding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
                decoration: BoxDecoration(
                  color: _ctrl.allComplete
                      ? theme.colorScheme.primaryContainer
                      : theme.colorScheme.surfaceContainerHighest,
                  borderRadius: BorderRadius.circular(16),
                ),
                child: Text(
                  '${_ctrl.completedCount}/${_ctrl.devices.length}',
                  style: theme.textTheme.labelLarge?.copyWith(
                    color: _ctrl.allComplete
                        ? theme.colorScheme.onPrimaryContainer
                        : theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            ],
          ),
        ),
        Flexible(
          child: CustomScrollView(
            shrinkWrap: true,
            physics: const ClampingScrollPhysics(),
            slivers: [
              SliverPadding(
                padding: const EdgeInsets.all(16),
                sliver: SliverList.list(
                  children: [
                    // Security warning — compact
                    Container(
                      padding: const EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: theme.colorScheme.errorContainer
                            .withValues(alpha: 0.3),
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Icon(Icons.warning_amber_rounded,
                              color: theme.colorScheme.error, size: 20),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Text(
                              "Each backup is secret. Anyone with ${_ctrl.threshold} of ${_ctrl.devices.length} keys can take all your bitcoin.",
                              style: theme.textTheme.bodySmall,
                            ),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 20),

                    // One card per key
                    ..._ctrl.devices.map(
                        (device) => _buildKeyCard(context, device)),

                    const SizedBox(height: 24),
                    Center(
                      child: _ctrl.allComplete
                          ? FilledButton(
                              onPressed: () => Navigator.pop(context),
                              child: const Text('Done'),
                            )
                          : TextButton(
                              onPressed: () => Navigator.pop(context),
                              child: const Text('Finish later'),
                            ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildKeyCard(BuildContext context, BackupDevice device) {
    final theme = Theme.of(context);

    final bool isPhysical = !device.isAppKey;
    final physicalIndex = isPhysical
        ? _ctrl.physicalDevices
            .indexWhere((d) => d.shareIndex == device.shareIndex)
        : -1;
    final isConnected = isPhysical && _ctrl.connectedPhysicalIndex == physicalIndex;
    final needsConnection = isPhysical && !device.backedUp && !isConnected;

    // Icon + name
    final IconData leadIcon;
    final Color leadColor;
    final String displayName;
    if (device.isAppKey) {
      leadIcon = appKeyIcon(context);
      leadColor = theme.colorScheme.primary;
      displayName = appKeyLabel(context);
    } else {
      leadIcon = Icons.key;
      leadColor = theme.colorScheme.onSurfaceVariant;
      displayName = device.name;
    }

    // Status text
    final String statusText;
    final Color statusColor;
    if (device.backedUp) {
      statusText = 'Backed up';
      statusColor = theme.colorScheme.onSurfaceVariant;
    } else if (device.isAppKey) {
      statusText = 'Ready to back up';
      statusColor = theme.colorScheme.onSurfaceVariant;
    } else if (isConnected) {
      statusText = 'Connected';
      statusColor = theme.colorScheme.primary;
    } else {
      statusText = 'Plug in to back up';
      statusColor = theme.colorScheme.onSurfaceVariant;
    }

    // Trailing widget
    final Widget trailing;
    if (device.isAppKey || isConnected || device.backedUp) {
      trailing = _buildActions(context, device);
    } else {
      trailing = Icon(Icons.usb_rounded, size: 20,
          color: theme.colorScheme.onSurfaceVariant.withValues(alpha: 0.5));
    }

    // The card content — identical regardless of glow wrapper
    final content = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      child: Row(
        children: [
          _statusIcon(context, device),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  spacing: 6,
                  children: [
                    Icon(leadIcon, size: 16, color: leadColor),
                    Text(displayName, style: theme.textTheme.titleSmall),
                  ],
                ),
                Text(statusText,
                    style: theme.textTheme.bodySmall
                        ?.copyWith(color: statusColor)),
              ],
            ),
          ),
          trailing,
        ],
      ),
    );

    final glowColors = needsConnection
        ? [
            theme.colorScheme.outlineVariant,
            theme.colorScheme.primary,
            theme.colorScheme.secondary,
            theme.colorScheme.tertiary,
          ]
        : [Colors.transparent, Colors.transparent];

    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: AnimatedGradientBorder(
        stretchAlongAxis: true,
        borderSize: 1.0,
        glowSize: 3.0,
        animationTime: 6,
        borderRadius: BorderRadius.circular(12.0),
        gradientColors: glowColors,
        child: Card.filled(
          margin: EdgeInsets.zero,
          color: theme.colorScheme.surfaceContainerHigh,
          child: content,
        ),
      ),
    );
  }

  Widget _statusIcon(BuildContext context, BackupDevice device) {
    final theme = Theme.of(context);
    if (device.backedUp) {
      return Icon(Icons.check_circle, color: theme.colorScheme.primary, size: 24);
    }
    return Icon(Icons.circle_outlined,
        color: theme.colorScheme.onSurfaceVariant, size: 24);
  }

  Widget _buildActions(BuildContext context, BackupDevice device) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      spacing: 8,
      children: [
        if (!device.backedUp)
          FilledButton.tonal(
            onPressed: () => _showBackupDialog(context, device),
            child: const Text('Backup'),
          ),
        if (device.backedUp) ...[
          FilledButton.tonal(
            onPressed: () => _showBackupDialog(context, device),
            child: const Text('Backup'),
          ),
          OutlinedButton(
            onPressed: () => _showCheckDialog(context, device),
            child: const Text('Check'),
          ),
        ],
      ],
    );
  }

  // ===========================================================================
  // Dialogs
  // ===========================================================================

  static const _fakeSeedWords = [
    'abandon', 'ability', 'able', 'about', 'above', 'absent',
    'absorb', 'abstract', 'absurd', 'abuse', 'access', 'accident',
    'account', 'accuse', 'achieve', 'acid', 'acoustic', 'acquire',
    'across', 'act', 'action', 'actor', 'actress', 'actual', 'adapt',
  ];

  Widget _buildBackupInfoCard(BuildContext context, BackupDevice device) {
    final theme = Theme.of(context);
    final deviceName =
        device.isAppKey ? appKeyLabel(context) : device.name;

    Widget labeledField(String value, String label) => Column(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(value,
                style: theme.textTheme.titleLarge?.copyWith(
                    color: theme.colorScheme.primary,
                    fontWeight: FontWeight.bold),
                textAlign: TextAlign.center),
            const SizedBox(height: 2),
            Container(
                height: 1,
                width: double.infinity,
                color: theme.colorScheme.outline),
            const SizedBox(height: 2),
            Text(label,
                style: theme.textTheme.bodySmall
                    ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
                textAlign: TextAlign.center),
          ],
        );

    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 300),
      child: Container(
        padding: const EdgeInsets.all(16.0),
        decoration: BoxDecoration(
          border: Border.all(color: theme.colorScheme.outline),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text('Threshold:', style: theme.textTheme.bodyLarge),
                const SizedBox(width: 8),
                Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  decoration: BoxDecoration(
                    border: Border.all(color: theme.colorScheme.outline),
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text('${_ctrl.threshold}',
                      style: theme.textTheme.titleLarge?.copyWith(
                          color: theme.colorScheme.primary,
                          fontWeight: FontWeight.bold)),
                ),
                const SizedBox(width: 8),
                Text('of', style: theme.textTheme.bodyLarge),
                const SizedBox(width: 8),
                Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  decoration: BoxDecoration(
                    border: Border.all(color: theme.colorScheme.outline),
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text('${_ctrl.devices.length}',
                      style: theme.textTheme.titleLarge?.copyWith(
                          color: theme.colorScheme.primary,
                          fontWeight: FontWeight.bold)),
                ),
              ],
            ),
            const SizedBox(height: 16),
            labeledField(_ctrl.walletName, 'Wallet Name'),
            const SizedBox(height: 24),
            labeledField(deviceName, 'Device Name'),
          ],
        ),
      ),
    );
  }

  void _showBackupDialog(BuildContext context, BackupDevice device) async {
    if (device.isAppKey) {
      await _showAppKeyBackupFlow(context, device);
    } else {
      await _showPhysicalBackupFlow(context, device);
    }
  }

  Future<void> _showPhysicalBackupFlow(
      BuildContext context, BackupDevice device) async {
    final proceed = await showDialog<bool>(
      context: context,
      builder: (context) {
        return AlertDialog(
          title: const Text('Record backup information'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                'Write down the device information on your backup sheet.',
                style: Theme.of(context).textTheme.bodyMedium,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 24),
              Center(child: _buildBackupInfoCard(context, device)),
            ],
          ),
          actionsAlignment: MainAxisAlignment.end,
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Show secret backup'),
            ),
          ],
        );
      },
    );

    if (proceed != true) return;
    _ctrl.startPhysicalBackup(device);
  }

  Future<void> _showAppKeyBackupFlow(
      BuildContext context, BackupDevice device) async {
    final proceed = await showDialog<bool>(
      context: context,
      builder: (context) {
        final theme = Theme.of(context);
        return AlertDialog(
          title: const Text('Record backup information'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                'Write down the device information on your backup sheet.',
                style: theme.textTheme.bodyMedium,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 24),
              Center(child: _buildBackupInfoCard(context, device)),
              const SizedBox(height: 16),
              Container(
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  color: theme.colorScheme.errorContainer,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Icon(Icons.warning_amber_rounded,
                        color: theme.colorScheme.error),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Text(
                        'The key backup will be shown on your screen. Make sure no one else can see it.',
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: theme.colorScheme.onErrorContainer,
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          actionsAlignment: MainAxisAlignment.end,
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Show secret backup'),
            ),
          ],
        );
      },
    );

    if (proceed != true || !context.mounted) return;

    final confirmed = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) {
        final theme = Theme.of(context);
        return AlertDialog(
          title: Row(
            children: [
              Icon(appKeyIcon(context), size: 20),
              const SizedBox(width: 8),
              Text('Key #${device.shareIndex} backup'),
            ],
          ),
          content: SizedBox(
            width: 400,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Container(
                  padding: const EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color: theme.colorScheme.surfaceContainerHighest,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Wrap(
                    spacing: 4,
                    runSpacing: 4,
                    children: _fakeSeedWords.asMap().entries.map((entry) {
                      return SizedBox(
                        width: 115,
                        child: Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text: '${entry.key + 1}. ',
                                style: theme.textTheme.bodySmall?.copyWith(
                                    color:
                                        theme.colorScheme.onSurfaceVariant),
                              ),
                              TextSpan(
                                text: entry.value,
                                style: theme.textTheme.bodyMedium?.copyWith(
                                    fontWeight: FontWeight.w600),
                              ),
                            ],
                          ),
                        ),
                      );
                    }).toList(),
                  ),
                ),
                const SizedBox(height: 16),
                Text('Write down all 25 words in order. Store securely.',
                    style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant)),
              ],
            ),
          ),
          actionsAlignment: MainAxisAlignment.spaceBetween,
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('I\'ve written them down'),
            ),
          ],
        );
      },
    );

    if (confirmed == true) {
      _ctrl.markBackedUp(device);
    }
  }

  void _showCheckDialog(BuildContext context, BackupDevice device) async {
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Check backup'),
        content: Text(device.isAppKey
            ? 'Enter the 25 words to verify your backup of ${appKeyLabel(context)}.'
            : 'Enter the words on the device to verify your backup of "${device.name}".'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Done'),
          ),
        ],
      ),
    );
  }
}
