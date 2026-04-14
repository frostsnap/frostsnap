import 'dart:math' as math;
import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:glowy_borders/glowy_borders.dart';
import 'device_identity.dart';
import 'keygen_mockup.dart';
import 'keygen_appkey_mockup.dart';
import 'signing_appkey_mockup.dart';
import 'backup_appkey_mockup.dart';
import 'remote_keygen_mockup.dart';

const String deviceSvg = '''
<svg width="34.986689421321785mm" height="42.99022935571762mm" viewBox="0 0 34.986689421321785 42.99022935571762" xmlns="http://www.w3.org/2000/svg" version="1.1">
<g id="Binder022" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder022_f0000"  d="M -14.964990406185642 18.586987365833934 C -12.222647326109056 21.262466115746093 -6.102466677437631 21.177514052687993 0.0 21.19999999999999 C 6.121690939255835 21.173197742531492 12.231613033666312 21.274073642081408 14.969082389611895 18.60771394747341 C 17.192036356405605 16.42026948598908 17.145133620721403 12.729626455124311 17.150337936776445 9.214654998137103 C 17.15016857520842 9.062541610979817 17.149999999999988 8.910928801845651 17.15 8.759999999999998 L 17.15 -8.760000000000002 C 17.15 -8.914753765468582 17.150169136378064 -9.069446409953057 17.150338175634953 -9.2239777310766 C 17.142579739222196 -12.72727276368968 17.19842080113543 -16.18678348818877 15.049795021706236 -18.379036431181923 C 12.355533216618058 -21.090033156964946 6.171452934273059 -21.04122596621769 0.0023556717517863035 -21.0999930634531 C -6.101080496066673 -21.113188247010854 -12.221201449009136 -21.235110995509284 -14.9651450300972 -18.57374603173769 C -17.20075798684543 -16.36281066897254 -17.14189748249141 -12.78040602545065 -17.150337727621697 -9.223998902144816 C -17.1501688375326 -9.069320114984011 -17.14999999999999 -8.914616780668991 -17.15 -8.76 L -17.15 8.76 C -17.149999999999984 8.914615841852937 -17.150168835482276 9.06931823718443 -17.150337724548145 9.223997026763582 C -17.14207388097222 12.781031521903902 -17.20025878454716 16.36447394034848 -14.964990406185638 18.586987365833927 Z M -14.99999999999999 15.099999999999996 A 4 4 0 0 0 -11 19.1L 10.999999999999982 19.099999999999994 A 4 4 0 0 0 15 15.1L 14.999999999999991 -11.900000000000004 A 4 4 0 0 0 11 -15.9L -10.999999999999982 -15.900000000000002 A 4 4 0 0 0 -15 -11.9L -14.999999999999991 15.099999999999996 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.3;fill-rule: evenodd"/>
<title>b'Binder022'</title>
</g>
<g id="Binder023" transform="translate(17.493344,21.543007) scale(1,-1)">
<path id="Binder023_f0000"  d="M -10.999999999999979 19.099999999999998 A 4 4 0 0 1 -15 15.1L -15.0 -11.9 A 4 4 0 0 1 -11 -15.9L 10.999999999999982 -15.900000000000002 A 4 4 0 0 1 15 -11.9L 15.0 15.099999999999996 A 4 4 0 0 1 11 19.1L -10.99999999999998 19.099999999999998 Z " stroke="#666666" stroke-width="0.35 px" style="stroke-width:0.35;stroke-miterlimit:4;stroke-dasharray:none;stroke-linecap:square;fill:#888888;fill-opacity:0.1;fill-rule: evenodd"/>
<title>b'Binder023'</title>
</g>
</svg>
''';

const seedColor = Color(0xFF1595B2);
const double iconSize = 20.0;

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await DeviceIdentity.init();
  runApp(const MockupApp());
}

class MockupApp extends StatelessWidget {
  const MockupApp({super.key});

  @override
  Widget build(BuildContext context) {
    final colorScheme = ColorScheme.fromSeed(
      brightness: Brightness.dark,
      seedColor: seedColor,
    );
    final baseTheme = ThemeData(useMaterial3: true, colorScheme: colorScheme);

    return MaterialApp(
      title: 'Frostsnap Mockups',
      theme: baseTheme,
      debugShowCheckedModeBanner: false,
      home: const MockupHome(),
    );
  }
}

// =============================================================================
// Shared device state controller
// =============================================================================

class MockDevice {
  final String id;
  final String name;
  bool isConnected;
  bool hasSigned;

  MockDevice({
    required this.id,
    required this.name,
    this.isConnected = false,
    this.hasSigned = false,
  });
}

class DeviceSimController extends ChangeNotifier {
  final int threshold = 2;

  final List<MockDevice> allDevices = [
    MockDevice(id: '1', name: 'Living Room Device'),
    MockDevice(id: '2', name: 'Office Safe'),
    MockDevice(id: '3', name: 'Bank Vault'),
  ];

  final Set<String> selectedIds = {};
  bool signingStarted = false;

  List<MockDevice> get neededFrom =>
      allDevices.where((d) => selectedIds.contains(d.id)).toList();

  bool get selectionComplete => selectedIds.length >= threshold;

  int get remaining => threshold - selectedIds.length;

  Set<String> get gotShares =>
      {for (final d in neededFrom) if (d.hasSigned) d.id};

  bool get signingDone => signingStarted && gotShares.length >= neededFrom.length;

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
    notifyListeners();
  }

  void toggleConnected(MockDevice device) {
    device.isConnected = !device.isConnected;
    notifyListeners();
    if (device.isConnected && !device.hasSigned) {
      Future.delayed(const Duration(seconds: 2), () {
        if (device.isConnected && !device.hasSigned) {
          device.hasSigned = true;
          notifyListeners();
        }
      });
    }
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
// Home page
// =============================================================================

class MockupHome extends StatefulWidget {
  const MockupHome({super.key});

  @override
  State<MockupHome> createState() => _MockupHomeState();
}

class _MockupHomeState extends State<MockupHome> {
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Mockups')),
      body: ListView(
        children: [
          ListTile(
            title: const Text('Signing Dialog'),
            subtitle: const Text('Transaction signing with device progress'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => _openSigningMockup(context),
          ),
          ListTile(
            title: const Text('Keygen Workflow'),
            subtitle: const Text('Full wallet creation flow'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => _openKeygenMockup(context),
          ),
          ListTile(
            title: const Text('Keygen + App Key'),
            subtitle: const Text('Wallet creation with App Key option'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => _openAppKeyKeygenMockup(context),
          ),
          ListTile(
            title: const Text('Signing + App Key'),
            subtitle: const Text('Transaction signing with App Key'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => Navigator.push(
              context,
              MaterialPageRoute(
                  builder: (_) => const AppKeySigningScaffold()),
            ),
          ),
          ListTile(
            title: const Text('Backup + App Key'),
            subtitle: const Text('Backup workflow with App Key'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => Navigator.push(
              context,
              MaterialPageRoute(
                  builder: (_) => const BackupAppKeyScaffold()),
            ),
          ),
          const Divider(),
          ListTile(
            title: const Text('Remote Keygen'),
            subtitle: const Text('Organisation wallet creation flow'),
            trailing: const Icon(Icons.chevron_right),
            onTap: () => Navigator.push(
              context,
              MaterialPageRoute(
                  builder: (_) => const RemoteKeygenMockupScaffold()),
            ),
          ),
        ],
      ),
    );
  }
}

void _openAppKeyKeygenMockup(BuildContext context) {
  Navigator.push(
    context,
    MaterialPageRoute(builder: (_) => const AppKeyKeygenScaffold()),
  );
}

class AppKeyKeygenScaffold extends StatefulWidget {
  const AppKeyKeygenScaffold({super.key});

  @override
  State<AppKeyKeygenScaffold> createState() => _AppKeyKeygenScaffoldState();
}

class _AppKeyKeygenScaffoldState extends State<AppKeyKeygenScaffold> {
  final _ctrl = AppKeyKeygenController();
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
      body: Stack(
        children: [
          Center(
            child: Dialog(
              backgroundColor: backgroundColor,
              clipBehavior: Clip.hardEdge,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 580),
                child: AppKeyKeygenPage(controller: _ctrl),
              ),
            ),
          ),
          if (_ctrl.step == AppKeyKeygenStep.generating)
            _buildFullscreenActionOverlay(context),
          if (_ctrl.step == AppKeyKeygenStep.generating)
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

  Widget _buildFullscreenActionOverlay(BuildContext context) {
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
              Text('Security Check',
                  style: theme.textTheme.headlineSmall
                      ?.copyWith(color: Colors.white)),
              const SizedBox(height: 24),
              Text(
                  _ctrl.includeAppKey
                      ? 'Confirm that this code is shown on all hardware devices'
                      : 'Confirm that this code is shown on all devices',
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
                      Text(
                          '${_ctrl.threshold}-of-${_ctrl.totalDeviceCount}',
                          style: theme.textTheme.labelLarge),
                      Text(_ctrl.sessionHash,
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
              onPressed: () => _ctrl.back(context),
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
                  progress: _ctrl.ackedDevices.length,
                  total: _ctrl.connectedDeviceCount,
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
        width: 300,
        child: Column(
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
                  Text('Simulate Device Confirm',
                      style: theme.textTheme.labelLarge),
                  const Spacer(),
                  IconButton(
                    icon: const Icon(Icons.minimize, size: 18),
                    tooltip: 'Collapse',
                    onPressed: () => setState(() => _simCollapsed = true),
                    visualDensity: VisualDensity.compact,
                  ),
                ],
              ),
            ),
            // Physical devices
            ...List.generate(_ctrl.connectedDeviceCount, (i) {
              final acked = _ctrl.ackedDevices.contains(i);
              final name = _ctrl.deviceNames[i]?.isNotEmpty == true
                  ? _ctrl.deviceNames[i]!
                  : 'Device ${i + 1}';
              return ListTile(
                dense: true,
                leading: Icon(Icons.key, size: 20),
                title: Text(name),
                trailing: acked
                    ? Icon(Icons.check_circle, color: Colors.green, size: 20)
                    : FilledButton.tonal(
                        onPressed: () => _ctrl.ackDevice(i),
                        child: const Text('Confirm'),
                      ),
              );
            }),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }
}

void _openKeygenMockup(BuildContext context) {
  Navigator.push(
    context,
    MaterialPageRoute(builder: (_) => const KeygenMockupScaffold()),
  );
}

class KeygenMockupScaffold extends StatefulWidget {
  const KeygenMockupScaffold({super.key});

  @override
  State<KeygenMockupScaffold> createState() => _KeygenMockupScaffoldState();
}

class _KeygenMockupScaffoldState extends State<KeygenMockupScaffold> {
  final _ctrl = KeygenController();
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
      body: Stack(
        children: [
          Center(
            child: Dialog(
              backgroundColor: backgroundColor,
              clipBehavior: Clip.hardEdge,
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 580),
                child: KeygenMockupPage(controller: _ctrl),
              ),
            ),
          ),
          // Fullscreen action dialog during generating step
          if (_ctrl.step == KeygenStep.generating)
            _buildFullscreenActionOverlay(context),
          // Floating sim panel — only visible during generating step
          if (_ctrl.step == KeygenStep.generating)
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

  Widget _buildFullscreenActionOverlay(BuildContext context) {
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
              Text('Security Check',
                  style: theme.textTheme.headlineSmall?.copyWith(
                      color: Colors.white)),
              const SizedBox(height: 24),
              Text('Confirm that this code is shown on all devices',
                  style: theme.textTheme.bodyLarge?.copyWith(
                      color: Colors.white70),
                  textAlign: TextAlign.center),
              const SizedBox(height: 16),
              Card.filled(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text('${_ctrl.threshold}-of-${_ctrl.deviceCount}',
                          style: theme.textTheme.labelLarge),
                      Text(_ctrl.sessionHash,
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
              onPressed: () {
                _ctrl.ackedDevices.clear();
                _ctrl.back(context);
              },
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
                  progress: _ctrl.acksReceived,
                  total: _ctrl.deviceCount,
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
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
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
        child: Column(
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
                  Text('Simulate Device Confirm',
                      style: theme.textTheme.labelLarge),
                  const Spacer(),
                  IconButton(
                    icon: const Icon(Icons.minimize, size: 18),
                    tooltip: 'Collapse',
                    onPressed: () => setState(() => _simCollapsed = true),
                    visualDensity: VisualDensity.compact,
                  ),
                ],
              ),
            ),
            ...List.generate(_ctrl.connectedDeviceCount, (i) {
              final acked = _ctrl.ackedDevices.contains(i);
              final name = _ctrl.deviceNames[i] ?? 'Device ${i + 1}';
              return ListTile(
                dense: true,
                leading: Icon(Icons.key, size: 20),
                title: Text(name),
                trailing: acked
                    ? Icon(Icons.check_circle, color: Colors.green, size: 20)
                    : FilledButton.tonal(
                        onPressed: () => _ctrl.ackDevice(i),
                        child: const Text('Confirm'),
                      ),
              );
            }),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }
}

// =============================================================================
// Remote Keygen scaffold
// =============================================================================

class RemoteKeygenMockupScaffold extends StatefulWidget {
  const RemoteKeygenMockupScaffold({super.key});

  @override
  State<RemoteKeygenMockupScaffold> createState() =>
      _RemoteKeygenMockupScaffoldState();
}

class _RemoteKeygenMockupScaffoldState
    extends State<RemoteKeygenMockupScaffold> {
  final _ctrl = RemoteKeygenController();
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
    final showSim = _ctrl.step == RemoteKeygenStep.lobby ||
        _ctrl.step == RemoteKeygenStep.generating;

    return Scaffold(
      backgroundColor: theme.colorScheme.surfaceContainerLowest,
      body: Stack(
        children: [
          if (_ctrl.step == RemoteKeygenStep.generating)
            _buildFullscreenActionOverlay(context)
          else
            Center(
              child: Dialog(
                backgroundColor: backgroundColor,
                clipBehavior: Clip.hardEdge,
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 580),
                  child: RemoteKeygenPage(controller: _ctrl),
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

  Widget _buildFullscreenActionOverlay(BuildContext context) {
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
              Text('Security Check',
                  style: theme.textTheme.headlineSmall?.copyWith(
                      color: Colors.white)),
              const SizedBox(height: 24),
              Text(
                  'Confirm that this code is shown on all your devices',
                  style: theme.textTheme.bodyLarge?.copyWith(
                      color: Colors.white70),
                  textAlign: TextAlign.center),
              const SizedBox(height: 16),
              Card.filled(
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                          '${_ctrl.threshold}-of-${_ctrl.totalDeviceCount}',
                          style: theme.textTheme.labelLarge),
                      Text(_ctrl.sessionHash,
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
              onPressed: () {
                _ctrl.ackedDevices.clear();
                _ctrl.back(context);
              },
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
                  progress: _ctrl.acksReceived,
                  total: _ctrl.namedLocalDeviceCount,
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
        width: 280,
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
            if (_ctrl.step == RemoteKeygenStep.lobby) ...[
              ListTile(
                dense: true,
                leading: const Icon(Icons.usb_rounded, size: 20),
                title: const Text('Plug in device'),
                trailing: FilledButton.tonal(
                  onPressed: () => _ctrl.plugInDevice(),
                  child: const Text('Plug in'),
                ),
              ),
              const Divider(height: 8),
              ListTile(
                dense: true,
                leading: const Icon(Icons.person_add, size: 20),
                title: const Text('Alice (1 device)'),
                trailing: FilledButton.tonal(
                  onPressed: () => _ctrl.simulateParticipantJoin('Alice'),
                  child: const Text('Join'),
                ),
              ),
              ListTile(
                dense: true,
                leading: const Icon(Icons.person_add, size: 20),
                title: const Text('Bob (2 devices)'),
                trailing: FilledButton.tonal(
                  onPressed: () => _ctrl.simulateParticipantJoin(
                      'Bob', deviceCount: 2),
                  child: const Text('Join'),
                ),
              ),
            ],
            if (_ctrl.step == RemoteKeygenStep.generating)
              ...List.generate(_ctrl.namedLocalDeviceCount, (i) {
                final acked = _ctrl.ackedDevices.contains(i);
                final name = _ctrl.localDevices
                    .where((d) => d.isNamed)
                    .toList()[i]
                    .name!;
                return ListTile(
                  dense: true,
                  leading: const Icon(Icons.key, size: 20),
                  title: Text(name),
                  trailing: acked
                      ? const Icon(Icons.check_circle,
                          color: Colors.green, size: 20)
                      : FilledButton.tonal(
                          onPressed: () => _ctrl.ackDevice(i),
                          child: const Text('Confirm'),
                        ),
                );
              }),
            const SizedBox(height: 8),
          ],
        ),
      ),
    );
  }
}

void _openSigningMockup(BuildContext context) {
  Navigator.push(
    context,
    MaterialPageRoute(builder: (_) => const SigningMockupScaffold()),
  );
}

// =============================================================================
// Scaffold that hosts both the dialog and the floating sim panel
// =============================================================================

class SigningMockupScaffold extends StatefulWidget {
  const SigningMockupScaffold({super.key});

  @override
  State<SigningMockupScaffold> createState() => _SigningMockupScaffoldState();
}

class _SigningMockupScaffoldState extends State<SigningMockupScaffold> {
  final _controller = DeviceSimController();
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
      body: Stack(
        children: [
          // The dialog, centered, as it would appear in the real app
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
                      child: SigningDialogContent(
                        scrollController: scrollController,
                        controller: _controller,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),

          // Floating simulate panel
          Positioned(
            left: _simOffset.dx,
            top: _simOffset.dy,
            child: GestureDetector(
              onPanUpdate: (details) {
                setState(() => _simOffset += details.delta);
              },
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
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
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
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
                child: Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: _controller.allDevices.map((d) {
                    return FilterChip(
                      label: Text(d.name),
                      selected: d.isConnected,
                      onSelected: _controller.signingStarted
                          ? (_) => _controller.toggleConnected(d)
                          : null,
                      avatar: Icon(
                        d.isConnected ? Icons.usb : Icons.usb_off,
                      ),
                    );
                  }).toList(),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// =============================================================================
// TopBar
// =============================================================================

class MockTopBar extends StatelessWidget {
  final Widget? title;
  final Color? backgroundColor;
  final ScrollController? scrollController;
  final VoidCallback? onClose;

  const MockTopBar({
    super.key,
    this.title,
    this.backgroundColor,
    this.scrollController,
    this.onClose,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final headline = Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      child: Row(
        spacing: 20,
        children: [
          Expanded(
            child: DefaultTextStyle(
              style: theme.textTheme.titleLarge!,
              child: title ?? const SizedBox.shrink(),
            ),
          ),
          IconButton(
            onPressed: onClose ?? () => Navigator.pop(context),
            icon: const Icon(Icons.close),
            style: IconButton.styleFrom(
              backgroundColor: theme.colorScheme.surfaceContainerHighest,
            ),
          ),
        ],
      ),
    );

    Widget divider = const SizedBox(height: 1);
    if (scrollController != null) {
      divider = ListenableBuilder(
        listenable: scrollController!,
        builder: (context, _) {
          return AnimatedCrossFade(
            firstChild: const Divider(height: 1),
            secondChild: const SizedBox(height: 1),
            crossFadeState:
                scrollController!.hasClients && scrollController!.offset > 0
                    ? CrossFadeState.showFirst
                    : CrossFadeState.showSecond,
            duration: Durations.short3,
          );
        },
      );
    }

    return Material(
      color: backgroundColor,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const SizedBox(height: 8),
          headline,
          divider,
        ],
      ),
    );
  }
}

// =============================================================================
// SatoshiText
// =============================================================================

class SatoshiText extends StatelessWidget {
  final int value;
  final bool showSign;
  final TextStyle? style;

  const SatoshiText({
    super.key,
    required this.value,
    this.showSign = false,
    this.style,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final baseStyle = DefaultTextStyle.of(context).style.merge(style).copyWith(
      fontFeatures: [FontFeature.slashedZero(), FontFeature.tabularFigures()],
    );
    final disabledColor = theme.disabledColor;

    final btcString = (value / 100000000.0).toStringAsFixed(8);
    final parts = btcString.replaceFirst('-', '').split('.');
    final sign = value.isNegative ? '-' : (showSign ? '+' : '\u00A0');
    final formatted =
        '$sign ${parts[0]}.${parts[1].substring(0, 2)} ${parts[1].substring(2, 5)} ${parts[1].substring(5)} \u20BF';

    var activeIndex = formatted.indexOf(RegExp(r'[1-9]'));
    if (activeIndex == -1) activeIndex = formatted.length - 1;

    return Text.rich(
      TextSpan(
        children: formatted.characters.indexed.map((elem) {
          final (i, char) = elem;
          final isActive = i >= activeIndex || char == '+' || char == '-';
          return TextSpan(
            text: char,
            style: baseStyle.copyWith(
              color: isActive ? null : disabledColor,
            ),
          );
        }).toList(),
      ),
      textAlign: TextAlign.right,
    );
  }
}

// =============================================================================
// Dialog content: tx cards + signing card (reads from shared controller)
// =============================================================================

class SigningDialogContent extends StatelessWidget {
  final ScrollController? scrollController;
  final DeviceSimController controller;

  const SigningDialogContent({
    super.key,
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
                  // --- Top card: status + amount ---
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

                  // --- Bottom card: tx details ---
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

                  // --- Signer selection OR signing progress ---
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
          ...controller.allDevices.map((device) {
            final isSelected = controller.selectedIds.contains(device.id);
            return CheckboxListTile(
              value: isSelected,
              onChanged: remaining > 0 || isSelected
                  ? (_) => controller.toggleSelected(device.id)
                  : null,
              secondary: const Icon(Icons.key),
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

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        ListTile(
          title: const Text('Signatures Needed'),
          subtitle: const Text('Connect a device to sign'),
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
          } else {
            trailing = Text(
              device.isConnected ? 'Requesting Signature' : '',
              style: TextStyle(
                color: device.isConnected ? theme.colorScheme.primary : null,
              ),
            );
          }
          return ListTile(
            enabled: device.isConnected,
            title: Text(device.name),
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

// =============================================================================
// AnimatedCheckCircle (standalone copy)
// =============================================================================

class _CirclePainter extends CustomPainter {
  final double progress;
  final ThemeData theme;

  _CirclePainter(this.progress, this.theme);

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = theme.colorScheme.primary
      ..strokeWidth = 2.0
      ..style = PaintingStyle.stroke;

    canvas.drawArc(
      Rect.fromCenter(
        center: Offset(size.width / 2, size.height / 2),
        width: size.width,
        height: size.height,
      ),
      -math.pi / 2,
      2 * math.pi * progress,
      false,
      paint,
    );
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => true;
}

class AnimatedCheckCircle extends StatefulWidget {
  final double size;

  const AnimatedCheckCircle({super.key, this.size = iconSize});

  @override
  State<AnimatedCheckCircle> createState() => _AnimatedCheckCircleState();
}

class _AnimatedCheckCircleState extends State<AnimatedCheckCircle>
    with SingleTickerProviderStateMixin {
  late AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 500),
    );
    _controller.forward();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Stack(
      alignment: Alignment.center,
      children: [
        Icon(Icons.check, size: widget.size, color: theme.colorScheme.primary),
        AnimatedBuilder(
          animation: _controller,
          builder: (context, child) {
            return CustomPaint(
              painter: _CirclePainter(_controller.value, theme),
              size: Size(widget.size, widget.size),
            );
          },
        ),
      ],
    );
  }
}
