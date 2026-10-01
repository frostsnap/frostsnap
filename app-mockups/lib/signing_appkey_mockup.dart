import 'dart:math' as math;
import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'device_identity.dart';
import 'main.dart' show deviceSvg, MockTopBar, SatoshiText, AnimatedCheckCircle;
import 'keygen_appkey_mockup.dart' show appKeyIcon, appKeyLabel;

// =============================================================================
// Controller
// =============================================================================

class MockSigningDevice {
  final String id;
  final String name;
  final bool isAppKey;
  bool isConnected;
  bool hasSigned;

  MockSigningDevice({
    required this.id,
    required this.name,
    this.isAppKey = false,
    this.isConnected = false,
    this.hasSigned = false,
  });
}

class AppKeySigningController extends ChangeNotifier {
  final int threshold = 2;

  final List<MockSigningDevice> allDevices = [
    MockSigningDevice(id: 'app', name: '', isAppKey: true),
    MockSigningDevice(id: '1', name: 'Living Room Device'),
    MockSigningDevice(id: '2', name: 'Office Safe'),
    MockSigningDevice(id: '3', name: 'Bank Vault'),
  ];

  List<MockSigningDevice> get physicalDevices =>
      allDevices.where((d) => !d.isAppKey).toList();

  MockSigningDevice get appKeyDevice =>
      allDevices.firstWhere((d) => d.isAppKey);

  final Set<String> selectedIds = {'app'};
  bool signingStarted = false;

  bool get appKeySelected => selectedIds.contains('app');

  List<MockSigningDevice> get neededFrom =>
      allDevices.where((d) => selectedIds.contains(d.id)).toList();

  List<MockSigningDevice> get physicalNeededFrom =>
      neededFrom.where((d) => !d.isAppKey).toList();

  bool get selectionComplete => selectedIds.length >= threshold;

  int get remaining => threshold - selectedIds.length;

  Set<String> get gotShares =>
      {for (final d in neededFrom) if (d.hasSigned) d.id};

  int get physicalGotShares =>
      neededFrom.where((d) => !d.isAppKey && d.hasSigned).length;

  bool get signingDone =>
      signingStarted && gotShares.length >= neededFrom.length;

  bool get needsPhysicalDevices =>
      physicalNeededFrom.any((d) => !d.hasSigned);

  void toggleSelected(String id) {
    if (selectedIds.contains(id)) {
      selectedIds.remove(id);
    } else if (!selectionComplete) {
      selectedIds.add(id);
    }
    notifyListeners();
  }

  void startSigning() {
    if (!selectionComplete) return;
    signingStarted = true;
    // Pre-select app key but don't sign yet — user taps "Sign" button
    notifyListeners();
  }

  void signWithAppKey() {
    if (!appKeySelected || appKeyDevice.hasSigned) return;
    appKeyDevice.hasSigned = true;
    notifyListeners();
  }

  void toggleConnected(MockSigningDevice device) {
    device.isConnected = !device.isConnected;
    notifyListeners();
  }

  void signWithDevice(MockSigningDevice device) {
    if (!device.isConnected || device.hasSigned) return;
    device.hasSigned = true;
    notifyListeners();
  }

  void reset() {
    for (final d in allDevices) {
      d.isConnected = false;
      d.hasSigned = false;
    }
    selectedIds.clear();
    signingStarted = false;
    notifyListeners();
  }
}

// =============================================================================
// Scaffold
// =============================================================================

class AppKeySigningScaffold extends StatefulWidget {
  const AppKeySigningScaffold({super.key});

  @override
  State<AppKeySigningScaffold> createState() => _AppKeySigningScaffoldState();
}

class _AppKeySigningScaffoldState extends State<AppKeySigningScaffold> {
  final _controller = AppKeySigningController();
  bool _simCollapsed = false;
  Offset _simOffset = const Offset(16, 16);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final backgroundColor = theme.colorScheme.surface;
    final scrollController = ScrollController();

    return Scaffold(
      backgroundColor: theme.colorScheme.surfaceContainerLowest,
      body: ListenableBuilder(
        listenable: _controller,
        builder: (context, _) {
          // Fullscreen overlay appears when a physical device is connected and still needs to sign
          final hasConnectedUnsignedPhysical = _controller.physicalNeededFrom
              .any((d) => d.isConnected && !d.hasSigned);
          final showFullscreen = _controller.signingStarted &&
              !_controller.signingDone &&
              hasConnectedUnsignedPhysical;

          // Sim panel visible whenever signing is active and physical devices are involved
          final showSimPanel = _controller.signingStarted &&
              !_controller.signingDone &&
              _controller.physicalNeededFrom.isNotEmpty;

          return Stack(
            children: [
              Center(
                child: Dialog(
                  backgroundColor: backgroundColor,
                  clipBehavior: Clip.hardEdge,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 580),
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        MockTopBar(
                          title: const Text('Transaction Details'),
                          backgroundColor: backgroundColor,
                          scrollController: scrollController,
                          onClose: () => Navigator.pop(context),
                        ),
                        Flexible(
                          child: _AppKeySigningDialogContent(
                            scrollController: scrollController,
                            controller: _controller,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
              if (showFullscreen) _buildFullscreenOverlay(context),
              if (showSimPanel)
                Positioned(
                  left: _simOffset.dx,
                  top: _simOffset.dy,
                  child: GestureDetector(
                    onPanUpdate: (d) => setState(() => _simOffset += d.delta),
                    child: _buildSimPanel(context),
                  ),
                ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildFullscreenOverlay(BuildContext context) {
    final theme = Theme.of(context);
    final physicalNeeded = _controller.physicalNeededFrom;
    final physicalSigned = _controller.physicalGotShares;

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
              Text('Sign Transaction',
                  style: theme.textTheme.headlineSmall
                      ?.copyWith(color: Colors.white)),
              const SizedBox(height: 12),
              Text('Connect a device to sign',
                  style: theme.textTheme.bodyLarge
                      ?.copyWith(color: Colors.white70),
                  textAlign: TextAlign.center),
            ],
          ),
        ),
      ),
      persistentFooterButtons: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            OutlinedButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
            Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 12,
              children: [
                Text('Signatures',
                    style: theme.textTheme.labelMedium?.copyWith(
                        color: theme.colorScheme.onSurfaceVariant)),
                Stack(
                  alignment: Alignment.center,
                  children: [
                    SizedBox(
                      width: 36,
                      height: 36,
                      child: CircularProgressIndicator(
                        value: physicalNeeded.isEmpty
                            ? 0
                            : physicalSigned / physicalNeeded.length,
                        strokeWidth: 3,
                        backgroundColor:
                            theme.colorScheme.surfaceContainerHighest,
                        strokeCap: StrokeCap.round,
                      ),
                    ),
                    Text('$physicalSigned/${physicalNeeded.length}',
                        style: theme.textTheme.labelSmall),
                  ],
                ),
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
          listenable: _controller,
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
                    Text('Simulate',
                        style: theme.textTheme.labelLarge),
                    const Spacer(),
                    IconButton(
                      icon: const Icon(Icons.refresh, size: 18),
                      tooltip: 'Reset',
                      onPressed: _controller.reset,
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
              ..._controller.physicalDevices.map((d) {
                return ListTile(
                  dense: true,
                  leading: Icon(
                    d.isConnected ? Icons.usb : Icons.usb_off,
                    size: 20,
                  ),
                  title: Text(d.name),
                  trailing: d.hasSigned
                      ? Icon(Icons.check_circle, color: Colors.green, size: 20)
                      : Row(
                          mainAxisSize: MainAxisSize.min,
                          spacing: 4,
                          children: [
                            FilterChip(
                              label: Text(d.isConnected ? 'On' : 'Off'),
                              selected: d.isConnected,
                              onSelected: (_) =>
                                  _controller.toggleConnected(d),
                              visualDensity: VisualDensity.compact,
                            ),
                            if (d.isConnected)
                              FilledButton.tonal(
                                onPressed: () =>
                                    _controller.signWithDevice(d),
                                child: const Text('Sign'),
                              ),
                          ],
                        ),
                );
              }),
              const SizedBox(height: 8),
            ],
          ),
        ),
      ),
    );
  }
}

// =============================================================================
// Dialog content
// =============================================================================

class _AppKeySigningDialogContent extends StatelessWidget {
  final ScrollController? scrollController;
  final AppKeySigningController controller;

  const _AppKeySigningDialogContent({
    this.scrollController,
    required this.controller,
  });

  static const margin = EdgeInsets.only(left: 16.0, right: 16.0, bottom: 16.0);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        return CustomScrollView(
          controller: scrollController,
          shrinkWrap: true,
          physics: const ClampingScrollPhysics(),
          slivers: [
            SliverSafeArea(
              sliver: SliverList(
                delegate: SliverChildListDelegate.fixed([
                  Card.filled(
                    color: theme.colorScheme.surfaceContainer,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.vertical(
                        top: Radius.circular(24),
                        bottom: Radius.circular(4),
                      ),
                    ),
                    margin: margin.copyWith(bottom: 2),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8.0),
                      child: ListTile(
                        shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(12.0)),
                        contentPadding:
                            const EdgeInsets.symmetric(horizontal: 16),
                        title: Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            Text(controller.signingDone
                                ? 'Signed'
                                : controller.signingStarted
                                    ? 'Signing...'
                                    : 'Send'),
                            Expanded(
                              flex: 2,
                              child: SatoshiText(
                                value: -50000,
                                showSign: true,
                                style: theme.textTheme.bodyLarge,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  Card.filled(
                    color: theme.colorScheme.surfaceContainer,
                    margin: margin,
                    clipBehavior: Clip.hardEdge,
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.vertical(
                        top: Radius.circular(4),
                        bottom: Radius.circular(24),
                      ),
                    ),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 8.0),
                      child: _buildDetailsColumn(context),
                    ),
                  ),
                  if (!controller.signingStarted)
                    _buildSignerSelectionCard(context)
                  else
                    _buildSignAndBroadcastCard(context),
                ]),
              ),
            ),
          ],
        );
      },
    );
  }

  Widget _buildSignerSelectionCard(BuildContext context) {
    final theme = Theme.of(context);
    final remaining = controller.remaining;

    return Card.outlined(
      margin: margin,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: theme.colorScheme.outlineVariant),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          ListTile(
            dense: true,
            title: const Text('Select Signers'),
            trailing: Text('${controller.threshold} required'),
          ),
          // App Key first, pre-selected
          CheckboxListTile(
            value: controller.selectedIds.contains('app'),
            onChanged: remaining > 0 || controller.selectedIds.contains('app')
                ? (_) => controller.toggleSelected('app')
                : null,
            secondary: Icon(appKeyIcon(context)),
            title: Text(appKeyLabel(context)),
          ),
          // Physical devices
          ...controller.physicalDevices.map((device) {
            final isSelected = controller.selectedIds.contains(device.id);
            return CheckboxListTile(
              value: isSelected,
              onChanged: remaining > 0 || isSelected
                  ? (_) => controller.toggleSelected(device.id)
                  : null,
              secondary: const Icon(FrostsnapIcons.device),
              title: Text(device.name),
            );
          }),
          Padding(
            padding: const EdgeInsets.all(12.0),
            child: FilledButton(
              onPressed:
                  controller.selectionComplete ? controller.startSigning : null,
              child: Text(
                remaining > 0
                    ? 'Select $remaining more'
                    : 'Sign transaction',
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildDetailsColumn(BuildContext context) {
    const contentPadding = EdgeInsets.symmetric(horizontal: 16);
    final theme = Theme.of(context);
    return Column(
      children: [
        ListTile(
          dense: true,
          contentPadding: contentPadding,
          leading: const Text('Recipient #0'),
          title: Text(
            'bc1q xy2k gdyg jrsq tzq2 n0yr f249 3p83 kkfj hx0w lh',
            style: TextStyle(
              fontFeatures: [FontFeature.tabularFigures()],
              fontSize: 12,
            ),
            textAlign: TextAlign.end,
          ),
        ),
        ListTile(
          dense: true,
          contentPadding: contentPadding,
          leading: const Text('\u2570 Amount'),
          title: const SatoshiText(value: 50000, showSign: false),
        ),
        ListTile(
          dense: true,
          contentPadding: contentPadding,
          leading: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text('Fee '),
              Card.filled(
                color: theme.colorScheme.surfaceContainerHigh,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                      horizontal: 6.0, vertical: 2.0),
                  child:
                      Text('4.2 sat/vB', style: theme.textTheme.labelSmall),
                ),
              ),
            ],
          ),
          title: const SatoshiText(value: 590),
        ),
        ListTile(
          dense: true,
          contentPadding: contentPadding,
          leading: const Text('Txid'),
          title: Text(
            'a1b2c3d4e5f6...7890abcd',
            style: TextStyle(
              fontFeatures: [FontFeature.tabularFigures()],
              fontSize: 12,
            ),
            textAlign: TextAlign.end,
          ),
        ),
      ],
    );
  }

  Widget _buildSignAndBroadcastCard(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedCrossFade(
      firstChild: AnimatedGradientBorder(
        stretchAlongAxis: true,
        borderSize: 1.0,
        glowSize: 5.0,
        animationTime: 6,
        borderRadius: BorderRadius.circular(12.0),
        gradientColors: [
          theme.colorScheme.outlineVariant,
          theme.colorScheme.primary,
          theme.colorScheme.secondary,
          theme.colorScheme.tertiary,
        ],
        child: Card.filled(
          margin: EdgeInsets.zero,
          color: theme.colorScheme.surfaceContainerHigh,
          child: _buildSignaturesNeededColumn(context),
        ),
      ),
      secondChild: Card.filled(
        color: Colors.transparent,
        margin: EdgeInsets.zero,
        child: _buildBroadcastReadyColumn(context),
      ),
      crossFadeState: controller.signingDone
          ? CrossFadeState.showSecond
          : CrossFadeState.showFirst,
      duration: Durations.medium3,
      sizeCurve: Curves.easeInOutCubicEmphasized,
    );
  }

  Widget _buildSignaturesNeededColumn(BuildContext context) {
    final theme = Theme.of(context);
    final neededFrom = controller.neededFrom;
    final gotShares = controller.gotShares;

    final subtitle = controller.needsPhysicalDevices
        ? 'Connect a device to sign'
        : 'All signatures collected';

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ListTile(
          title: const Text('Signatures Needed'),
          subtitle: Text(subtitle),
          trailing: Stack(
            alignment: AlignmentDirectional.center,
            children: [
              CircularProgressIndicator(
                value: gotShares.length / neededFrom.length,
                backgroundColor: theme.colorScheme.surfaceContainerHighest,
                strokeCap: StrokeCap.round,
              ),
              Text('${gotShares.length}/${neededFrom.length}'),
            ],
          ),
        ),
        ...neededFrom.map((device) {
          final Widget trailing;
          if (device.hasSigned) {
            trailing = AnimatedCheckCircle(key: ValueKey('check_${device.id}'));
          } else if (device.isAppKey) {
            trailing = FilledButton.tonal(
              onPressed: () => controller.signWithAppKey(),
              child: const Text('Sign'),
            );
          } else {
            trailing = Text(
              device.isConnected ? 'Requesting Signature' : '',
              style: TextStyle(
                color: device.isConnected ? theme.colorScheme.primary : null,
              ),
            );
          }
          return ListTile(
            enabled: device.isConnected || device.isAppKey,
            leading: device.isAppKey
                ? Icon(appKeyIcon(context), size: 20)
                : null,
            title: Text(device.isAppKey
                ? appKeyLabel(context)
                : device.name),
            trailing: trailing,
          );
        }),
        const Divider(height: 0.0),
        Align(
          alignment: AlignmentDirectional.centerStart,
          child: Padding(
            padding:
                const EdgeInsets.symmetric(vertical: 4.0, horizontal: 12.0),
            child: TextButton(
              onPressed: () => Navigator.pop(context),
              style: TextButton.styleFrom(
                foregroundColor: theme.colorScheme.error,
              ),
              child: const Text('Cancel'),
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildBroadcastReadyColumn(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(16.0),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel')),
          FilledButton(onPressed: () {}, child: const Text('Broadcast')),
        ],
      ),
    );
  }
}
