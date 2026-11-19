// Renders every phone screen of the 64-word split card (design §5.2 to §5.4)
// to PNG for review, with the keygen, checklist and wallet screens. It is not a regression test: it is skipped unless SHOTS_DIR is
// given, and then it writes one PNG per screen into that directory.
//
//   flutter test test/card_screens_shots_test.dart \
//     --dart-define=SHOTS_DIR=/absolute/path/to/shots
//
// Phone size: 412x915 logical pixels at 2x. The app's own dark theme
// (main.dart) and its bundled Noto fonts, loaded under the family names
// google_fonts gives them, so text renders as it does in the app.
//
// No native library is loaded: the bridge is mocked with `RustLib.initMock`
// and `coord` is a fake, so nothing here can reach a device. A bridge call the
// fakes don't answer throws, naming the call.

import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:collection/collection.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration/device_discovery.dart';
import 'package:frostsnap/restoration/enter_card_view.dart';
import 'package:frostsnap/restoration/phone_half_entry.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/backup_run.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/firmware.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/src/rust/api/super_wallet.dart';
import 'package:frostsnap/src/rust/api/verifiable_card.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/verifiable_card/card_flow.dart';
import 'package:frostsnap/verifiable_card/card_flow_preview.dart';
import 'package:frostsnap/verifiable_card/card_lines.dart';
import 'package:frostsnap/verifiable_card/not_checked_dialog.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_create.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:rxdart/rxdart.dart';

import 'support/card_flow_pages.dart';

const _shotsDirDefine = String.fromEnvironment('SHOTS_DIR');
final String _shotsDir = _shotsDirDefine.isNotEmpty
    ? _shotsDirDefine
    : (Platform.environment['SHOTS_DIR'] ?? '');
final bool _skip = _shotsDir.isEmpty;

// ---------------------------------------------------------------------------
// Fonts and theme
// ---------------------------------------------------------------------------

/// The Flutter SDK's bundled Roboto and Material Icons. flutter_tester lives
/// in `<flutter>/bin/cache/artifacts/engine/<platform>/`.
String _materialFontsDir() {
  final root = Platform.environment['FLUTTER_ROOT'];
  if (root != null) {
    final dir = '$root/bin/cache/artifacts/material_fonts';
    if (Directory(dir).existsSync()) return dir;
  }
  final artifacts = File(Platform.resolvedExecutable).parent.parent.parent;
  return '${artifacts.path}/material_fonts';
}

Future<void> _loadFont(String family, List<String> files) async {
  final loader = FontLoader(family);
  for (final file in files) {
    final bytes = File(file).readAsBytesSync();
    loader.addFont(Future.value(ByteData.sublistView(bytes)));
  }
  await loader.load();
}

/// Registers the app's fonts under the family names google_fonts uses
/// (`NotoSans_regular`, `NotoSans_500`, ...), one file per family, as
/// google_fonts itself does in the app.
Future<Set<String>> _loadFonts() async {
  const g = 'assets/google_fonts';
  final m = _materialFontsDir();
  final families = <String, List<String>>{
    'NotoSans_regular': ['$g/NotoSans-Regular.ttf'],
    'NotoSans_500': ['$g/NotoSans-Medium.ttf'],
    'NotoSans_600': ['$g/NotoSans-SemiBold.ttf'],
    'NotoSans_700': ['$g/NotoSans-Bold.ttf'],
    'NotoSansMono_regular': ['$g/NotoSansMono-Regular.ttf'],
    'NotoSansMono_700': ['$g/NotoSansMono-Bold.ttf'],
    // Text that doesn't come from the text theme falls back to the platform
    // font: Roboto on Android.
    'Roboto': [
      for (final w in ['Light', 'Regular', 'Medium', 'Bold', 'Black'])
        '$m/Roboto-$w.ttf',
    ],
    'MaterialIcons': ['$m/MaterialIcons-Regular.otf'],
  };
  for (final MapEntry(:key, :value) in families.entries) {
    await _loadFont(key, value);
  }
  return families.keys.toSet();
}

/// main.dart's theme, exactly.
ThemeData _appTheme() {
  final colorScheme = ColorScheme.fromSeed(
    brightness: Brightness.dark,
    seedColor: seedColor,
  );
  final baseTheme = ThemeData(useMaterial3: true, colorScheme: colorScheme);
  final textTheme = GoogleFonts.notoSansTextTheme(baseTheme.textTheme);
  return baseTheme.copyWith(colorScheme: colorScheme, textTheme: textTheme);
}

late final ThemeData _theme;

// ---------------------------------------------------------------------------
// Fake bridge and coordinator
// ---------------------------------------------------------------------------

String _member(Invocation i) {
  final s = i.memberName.toString(); // Symbol("name")
  return s.substring('Symbol("'.length, s.length - 2);
}

U8Array32 _bytes32(int seed) => U8Array32(Uint8List(32)..[0] = seed);

DeviceId _deviceId(int seed) =>
    DeviceId(field0: U8Array33(Uint8List(33)..[0] = seed));

final _keyId = KeyId(field0: _bytes32(7));
final _masterAppkey = MasterAppkey(field0: U8Array65(Uint8List(65)..[0] = 7));
final _accessStructureRef = AccessStructureRef(
  keyId: _keyId,
  accessStructureId: AccessStructureId(field0: _bytes32(9)),
);

final _firmware = FirmwareVersion(digest: Sha256Digest(field0: _bytes32(1)));

ConnectedDevice _device(DeviceId id, {String? name}) => ConnectedDevice(
  name: name,
  firmware: _firmware,
  id: id,
  recoveryMode: const RecoveryMode.off(),
  genuine: const GenuineStatus.unverified(firmwareSupportsCheck: true),
);

/// Answers the few free functions and plain-struct methods the screens call.
class _FakeApi implements RustLibApi {
  @override
  dynamic noSuchMethod(Invocation i) {
    final name = _member(i);
    final that = i.namedArguments[#that];
    switch (name) {
      case 'crateApiNameKeyNameMaxLength':
        return 15;
      case 'crateApiNameDeviceNameMaxLength':
        return 14;
      case 'crateApiMasterAppkeyKeyId':
        return _keyId;
      case 'crateApiDeviceListConnectedDeviceNeedsFirmwareUpgrade':
        return false;
      case 'crateApiDeviceListConnectedDeviceFirmwareUpgradeEligibility':
        return const FirmwareUpgradeEligibility.upToDate();
      case 'crateApiDeviceListConnectedDeviceReady':
        return true;
      case 'crateApiBitcoinBitcoinNetworkIsMainnet':
        return that == BitcoinNetwork.bitcoin;
      case 'crateApiBitcoinBitcoinNetworkName':
        return (that as BitcoinNetwork).toString().split('.').last;
      case 'crateApiDeviceListDeviceListStateGetDevice':
        final state = that as DeviceListState;
        final id = i.namedArguments[#id] as DeviceId;
        return state.devices.firstWhereOrNull((d) => d.id == id);
    }
    throw UnimplementedError('shots: the fake bridge has no $name');
  }
}

class _FakeCoordinator implements Coordinator {
  List<ConnectedDevice> devices = [];
  int _stateId = 0;
  final _deviceEvents = StreamController<DeviceListUpdate>.broadcast(
    sync: true,
  );
  bool supportsCard = true;
  Set<DeviceId> suspect = {};
  Map<DeviceId, String> names = {};
  FrostKey? frostKey;
  bool usesCard = true;

  DeviceListState get _state =>
      DeviceListState(devices: devices, unlisted: const [], stateId: _stateId);

  void setDevices(List<ConnectedDevice> list) {
    devices = list;
    _stateId += 1;
    _deviceEvents.add(DeviceListUpdate(changes: const [], state: _state));
  }

  @override
  DeviceListState deviceListState() => _state;

  @override
  Stream<DeviceListUpdate> subDeviceEvents() => _deviceEvents.stream;

  @override
  ConnectedDevice? getConnectedDevice({required DeviceId id}) =>
      devices.firstWhereOrNull((d) => d.id == id);

  @override
  String? getDeviceName({required DeviceId id}) => names[id];

  @override
  bool devicesSupportCard({required List<DeviceId> devices}) => supportsCard;

  @override
  bool deviceIsSuspect({required DeviceId deviceId}) =>
      suspect.contains(deviceId);

  @override
  bool genuineCheckEnabled() => false;

  @override
  bool keyUsesCard({required KeyId keyId}) => usesCard;

  @override
  FrostKey? getFrostKey({required KeyId keyId}) => frostKey;

  @override
  NonceRequest createNonceRequest({required List<DeviceId> devices}) =>
      _FakeNonceRequest();

  @override
  Future<void> updateNamePreview({
    required DeviceId id,
    required String name,
  }) async {}

  @override
  Future<void> sendCancel({required DeviceId id}) async {}

  @override
  Future<void> cancelProtocol() async {}

  @override
  Stream<WaitForSingleDeviceState> waitForSingleDevice() =>
      Stream.value(const WaitForSingleDeviceState.noDevice());

  @override
  dynamic noSuchMethod(Invocation i) => throw UnimplementedError(
    'shots: the fake coordinator has no ${_member(i)}',
  );
}

class _FakeNonceRequest implements NonceRequest {
  @override
  bool someNoncesRequested() => false;

  @override
  dynamic noSuchMethod(Invocation i) => throw UnimplementedError(_member(i));
}

class _FakeAccessStructure implements AccessStructure {
  final List<DeviceId> deviceIds;
  final int t;

  _FakeAccessStructure(this.deviceIds, this.t);

  @override
  int threshold() => t;

  @override
  List<DeviceId> devices() => deviceIds;

  @override
  int? getDeviceShortShareIndex({required DeviceId deviceId}) {
    final i = deviceIds.indexOf(deviceId);
    return i < 0 ? null : i + 1;
  }

  @override
  AccessStructureRef accessStructureRef() => _accessStructureRef;

  @override
  MasterAppkey masterAppkey() => _masterAppkey;

  @override
  dynamic noSuchMethod(Invocation i) =>
      throw UnimplementedError('shots: fake access structure: ${_member(i)}');
}

class _FakeFrostKey implements FrostKey {
  final AccessStructure accessStructure;

  _FakeFrostKey(this.accessStructure);

  @override
  List<AccessStructure> accessStructures() => [accessStructure];

  @override
  KeyId keyId() => _keyId;

  @override
  String keyName() => 'Family savings';

  @override
  MasterAppkey masterAppkey() => _masterAppkey;

  @override
  dynamic noSuchMethod(Invocation i) =>
      throw UnimplementedError('shots: fake frost key: ${_member(i)}');
}

class _FakeSuperWallet implements SuperWallet {
  @override
  dynamic noSuchMethod(Invocation i) =>
      throw UnimplementedError('shots: fake super wallet: ${_member(i)}');
}

final _fake = _FakeCoordinator();

// ---------------------------------------------------------------------------
// Card flow driver
// ---------------------------------------------------------------------------

/// A card flow driver the test moves by hand. Every action is a no-op: the
/// test emits the next stage itself.
class _ShotDriver implements CardFlowDriver {
  final bool plugged;
  final _states = StreamController<CardFlowState>();
  bool _left = false;

  _ShotDriver({this.plugged = true});

  void emit(CardFlowState state) {
    if (!_left) _states.add(state);
  }

  void fail(Object error) {
    if (!_left) _states.addError(error);
  }

  @override
  Stream<CardFlowState> start() => _states.stream;

  @override
  Stream<bool> deviceConnected() => Stream.value(plugged);

  @override
  int get sealSize => fakeSealSize;

  @override
  Future<void> deviceHalfWritten() async {}

  @override
  Future<void> writeUnverified() async {}

  @override
  Future<void> phoneNext() async {}

  @override
  Future<void> phoneBack() async {}

  @override
  Future<void> phoneHalfWritten() async {}

  @override
  Future<bool> selfCheckAnswer(String word) async => true;

  @override
  Future<void> markBackupComplete() async {}

  @override
  Future<void> finalBegin() async {}

  @override
  Future<void> answer(CardAnswer answer) async {}

  @override
  Future<CardFlowState?> answerInterruption(bool causedByUser) async => null;

  @override
  Future<void> recordChanged() async {}

  @override
  void leave() {
    _left = true;
    _states.close();
  }
}

CardFlowState _cs(
  CardFlowStage stage, {
  bool connected = true,
  int points = fakeSealSize,
  int proofs = 0,
  int boxes = 0,
  bool started = false,
  bool halves = false,
  bool deviceDone = false,
  bool unverified = false,
}) => cardFlowState(
  stage,
  connected: connected,
  points: points,
  proofs: proofs,
  boxes: boxes,
  started: started,
  halves: halves,
  deviceDone: deviceDone,
  unverified: unverified,
);

CardFlowStage _question(
  int k,
  int wordNumber,
  List<String> options, {
  bool secondLook = false,
}) => CardFlowStage.question(
  k: k,
  wordNumber: wordNumber,
  options: options,
  secondLook: secondLook,
);

CardFlowStage _selfCheck(
  int k,
  int total,
  int wordNumber,
  List<String> options, {
  bool wrong = false,
  bool recheck = false,
}) => CardFlowStage.selfCheck(
  k: k,
  total: total,
  wordNumber: wordNumber,
  options: options,
  wrong: wrong,
  recheck: recheck,
);

/// Words 33–64 on page [page] (three a page, the last two).
List<String> _pageWords(int page) =>
    fakePhoneHalf.sublist(page * 3, (page * 3 + 3).clamp(0, 32));

/// A card restore driver the test moves by hand.
class _ShotRestoreDriver implements CardRestoreDriver {
  final _states = StreamController<EnterPhysicalBackupState>();

  void show(CardRestoreView view) => _states.add(
    EnterPhysicalBackupState(deviceId: _d1, saved: false, cardRestore: view),
  );

  @override
  Stream<EnterPhysicalBackupState> start() => _states.stream;

  @override
  Future<void> sendPhoneHalf(List<String?> words) async {}

  @override
  Stream<bool> deviceConnected() => Stream.value(true);
}

// ---------------------------------------------------------------------------
// Host and capture
// ---------------------------------------------------------------------------

final _rootKey = GlobalKey();
late BuildContext _homeContext;
var _shotNumber = 0;

Future<void> _pumpHost(WidgetTester tester, {Widget? home}) async {
  tester.view.physicalSize = const Size(824, 1830);
  tester.view.devicePixelRatio = 2;
  // Android's status and gesture bars.
  tester.view.padding = const FakeViewPadding(top: 48, bottom: 48);
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    RepaintBoundary(
      key: _rootKey,
      child: MaterialApp(
        theme: _theme,
        debugShowCheckedModeBanner: false,
        home: Builder(
          builder: (context) {
            _homeContext = context;
            return home ?? const Scaffold();
          },
        ),
      ),
    ),
  );
}

/// Fixed-step pumps: the screens hold progress indicators, so pumpAndSettle
/// would never return.
Future<void> _settle(
  WidgetTester tester, [
  Duration duration = const Duration(milliseconds: 900),
]) async {
  const step = Duration(milliseconds: 50);
  for (var t = Duration.zero; t < duration; t += step) {
    await tester.pump(step);
  }
}

/// Lets real async work finish (asset reads, image and SVG decodes), then
/// draws the result.
Future<void> _settleAssets(WidgetTester tester) async {
  for (var i = 0; i < 4; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 60)),
    );
    await tester.pump();
  }
}

Future<String> _shot(WidgetTester tester, String id, String short) async {
  await _settleAssets(tester);
  _shotNumber += 1;
  final name = '${_shotNumber.toString().padLeft(2, '0')}_${id}_$short';
  final boundary = tester.renderObject<RenderRepaintBoundary>(
    find.byKey(_rootKey),
  );
  await tester.runAsync(() async {
    final image = await boundary.toImage(
      pixelRatio: tester.view.devicePixelRatio,
    );
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    image.dispose();
    File('$_shotsDir/$name.png').writeAsBytesSync(data!.buffer.asUint8List());
  });
  // ignore: avoid_print
  print('shot: $name');
  return name;
}

/// Shoots the screen, and if [scrollable]'s content runs past the screen,
/// shoots it again scrolled to the end.
Future<void> _shotWithScroll(
  WidgetTester tester,
  String id,
  String short,
  Finder scrollable,
) async {
  await _shot(tester, id, short);
  final state = tester.state<ScrollableState>(scrollable.first);
  final position = state.position;
  if (position.maxScrollExtent <= 0) return;
  position.jumpTo(position.maxScrollExtent);
  await tester.pump();
  await _shot(tester, id, '${short}_scrolled');
  position.jumpTo(0);
  await tester.pump();
}

Finder get _flowScrollable => find.descendant(
  of: find.byType(VerifiableCardFlow),
  matching: find.byType(Scrollable),
);

Future<_ShotDriver> _openFlow(
  WidgetTester tester,
  CardFlowState first, {
  bool plugged = true,
}) async {
  await _pumpHost(tester);
  final driver = _ShotDriver(plugged: plugged);
  unawaited(VerifiableCardFlow.show(_homeContext, driver: driver));
  await tester.pump();
  driver.emit(first);
  await _settle(tester);
  return driver;
}

Future<void> _emit(
  WidgetTester tester,
  _ShotDriver driver,
  CardFlowState state,
) async {
  driver.emit(state);
  await _settle(tester);
}

/// Unmounts the tree and runs the post-frame callbacks that dispose the
/// device overlays, so no overlay outlives its test.
Future<void> _teardown(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox());
  await tester.pump();
  await tester.pump();
}

/// What a recovery view sits in on a phone: `RecoveryFlowWithDiscovery`'s
/// switcher, then `WalletRecoveryFlow`'s, which on compact is all it draws
/// (recovery_flow.dart:666-668). Both default to a centring Stack.
Widget _recoveryFrame(Widget view) => AnimatedSwitcher(
  duration: Durations.medium4,
  child: KeyedSubtree(
    key: const ValueKey('recovery_flow'),
    child: AnimatedSwitcher(
      duration: Durations.medium4,
      reverseDuration: Duration.zero,
      child: view,
    ),
  ),
);

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

final _d1 = _deviceId(1);
final _d2 = _deviceId(2);
final _d3 = _deviceId(3);
final _d4 = _deviceId(4);

/// Steps the real wallet-create page through name and devices to the
/// threshold step.
Future<void> _toThresholdStep(WidgetTester tester) async {
  await _openWalletCreate(tester);
  await _nameDevices(tester, ['Kitchen', 'Office', 'Safe']);
  await tester.tap(
    find.widgetWithText(FilledButton, 'Continue with 3 devices'),
  );
  await _settle(tester);
  expect(find.text('Choose threshold'), findsWidgets);
}

Future<void> _openWalletCreate(WidgetTester tester) async {
  _fake.setDevices([_device(_d1), _device(_d2), _device(_d3)]);
  await _pumpHost(tester);
  await tester.runAsync(() => Future<void>.delayed(Duration.zero));
  unawaited(
    MaybeFullscreenDialog.show(
      context: _homeContext,
      child: const WalletCreatePage(),
    ),
  );
  await _settle(tester);
  await tester.enterText(find.byType(TextField), 'Family savings');
  await tester.pump();
  await tester.tap(find.widgetWithText(FilledButton, 'Next'));
  await _settle(tester);
}

Future<void> _nameDevices(WidgetTester tester, List<String> names) async {
  final fields = find.byType(TextField);
  for (var i = 0; i < names.length; i++) {
    await tester.enterText(fields.at(i), names[i]);
    await tester.pump();
  }
  // Close the keyboard focus so no caret shows.
  FocusManager.instance.primaryFocus?.unfocus();
  await _settle(tester, const Duration(milliseconds: 300));
}

Finder get _createScrollable => find.descendant(
  of: find.byType(WalletCreatePage),
  matching: find.byType(Scrollable),
);

/// A 2-of-4 card wallet with one share in each card status.
_FakeAccessStructure _setUpCardWallet() {
  final accessStructure = _FakeAccessStructure([_d1, _d2, _d3, _d4], 2);
  _fake
    ..frostKey = _FakeFrostKey(accessStructure)
    ..usesCard = true
    ..names = {_d1: 'Kitchen', _d2: 'Office', _d3: 'Safe', _d4: 'Spare'};
  return accessStructure;
}

BackupRun _cardBackupRun() => BackupRun(
  devices: [
    BackupDevice(
      deviceId: _d1,
      shareIndex: 1,
      complete: true,
      cardStatus: CardBackupStatus.checked,
      cardNextStep: null,
    ),
    BackupDevice(
      deviceId: _d2,
      shareIndex: 2,
      complete: false,
      cardStatus: CardBackupStatus.notChecked,
      cardNextStep: null,
    ),
    BackupDevice(
      deviceId: _d3,
      shareIndex: 3,
      complete: false,
      cardStatus: CardBackupStatus.noCard,
      cardNextStep: null,
    ),
    BackupDevice(
      deviceId: _d4,
      shareIndex: 4,
      complete: false,
      cardStatus: CardBackupStatus.failed,
      cardNextStep: NextStep.withoutThisDevice,
    ),
  ],
);

/// Owner B15: the same wallet on an install that didn't create it: a key
/// restored from its card (no record), an unverified card being written, and
/// one written.
BackupRun _otherInstallBackupRun() => BackupRun(
  devices: [
    BackupDevice(
      deviceId: _d1,
      shareIndex: 1,
      complete: null,
      cardStatus: null,
      cardNextStep: null,
    ),
    BackupDevice(
      deviceId: _d2,
      shareIndex: 2,
      complete: null,
      cardStatus: CardBackupStatus.notChecked,
      cardNextStep: null,
    ),
    BackupDevice(
      deviceId: _d3,
      shareIndex: 3,
      complete: null,
      cardStatus: CardBackupStatus.unverified,
      cardNextStep: null,
    ),
    BackupDevice(
      deviceId: _d4,
      shareIndex: 4,
      complete: null,
      cardStatus: null,
      cardNextStep: null,
    ),
  ],
);

WalletContext _walletContext(Widget child, {BackupRun? backupRun}) =>
    WalletContext(
      wallet: Wallet(
        superWallet: _FakeSuperWallet(),
        masterAppkey: _masterAppkey,
      ),
      txStream: const Stream.empty(),
      backupStream: BehaviorSubject.seeded(backupRun ?? _cardBackupRun()),
      signingSessionSignals: const Stream.empty(),
      child: child,
    );

// ---------------------------------------------------------------------------
// Tests, in flow order: shots are numbered in the order they are taken.
// ---------------------------------------------------------------------------

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    if (_skip) return;
    Directory(_shotsDir).createSync(recursive: true);
    final loaded = await _loadFonts();
    GoogleFonts.config.allowRuntimeFetching = false;
    _theme = _appTheme();
    // Every family the text theme names must be one we loaded, or its text
    // would fall back to the test font's boxes.
    final used = <String>{
      for (final style in [
        _theme.textTheme.displayLarge,
        _theme.textTheme.displayMedium,
        _theme.textTheme.displaySmall,
        _theme.textTheme.headlineLarge,
        _theme.textTheme.headlineMedium,
        _theme.textTheme.headlineSmall,
        _theme.textTheme.titleLarge,
        _theme.textTheme.titleMedium,
        _theme.textTheme.titleSmall,
        _theme.textTheme.bodyLarge,
        _theme.textTheme.bodyMedium,
        _theme.textTheme.bodySmall,
        _theme.textTheme.labelLarge,
        _theme.textTheme.labelMedium,
        _theme.textTheme.labelSmall,
        monospaceTextStyle,
      ])
        if (style?.fontFamily != null) style!.fontFamily!,
    };
    final missing = used.difference(loaded);
    if (missing.isNotEmpty) fail('fonts not loaded: $missing');

    RustLib.initMock(api: _FakeApi());
    coord = _fake;
    // Create the app's device-list subject now, outside any test's fake
    // clock, and give it a first list.
    GlobalStreams.deviceListSubject;
    _fake.setDevices([]);
    await Future<void>.delayed(Duration.zero);
  });

  // ------------------------------------------------------------- keygen --

  testWidgets('keygen: the card note, then threshold 1', (tester) async {
    _fake
      ..supportsCard = true
      ..suspect = {};
    await _toThresholdStep(tester);
    await _shotWithScroll(tester, 'K1', 'keygen_card_note', _createScrollable);

    // Threshold 1 of 3: any one key can spend, so no card.
    tester.widget<Slider>(find.byType(Slider)).onChanged!(1);
    await _settle(tester);
    await _shot(tester, 'K2', 'keygen_card_disabled_threshold1');
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('keygen: card option disabled by firmware', (tester) async {
    _fake
      ..supportsCard = false
      ..suspect = {};
    await _toThresholdStep(tester);
    await _shot(tester, 'K2', 'keygen_card_disabled_firmware');
    _fake.supportsCard = true;
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('keygen: a suspect device blocks the devices step', (
    tester,
  ) async {
    _fake
      ..supportsCard = true
      ..suspect = {_d3};
    await _openWalletCreate(tester);
    await _nameDevices(tester, ['Kitchen', 'Office']);
    await _shot(tester, 'K3', 'keygen_suspect_device');
    _fake.suspect = {};
    await _teardown(tester);
  }, skip: _skip);

  // ------------------------------------------------- checklist and wallet --

  testWidgets('backup checklist: a row in each card status', (tester) async {
    final accessStructure = _setUpCardWallet();
    _fake.setDevices([_device(_d3, name: 'Safe')]);
    await _pumpHost(tester);
    await tester.runAsync(() => Future<void>.delayed(Duration.zero));
    unawaited(
      MaybeFullscreenDialog.show(
        context: _homeContext,
        child: _walletContext(
          BackupChecklist(accessStructure: accessStructure, showAppBar: true),
        ),
      ),
    );
    await _settle(tester);
    // Owner B10: the failed row says what to do, never a dead end.
    expect(
      find.text('Set this wallet up again without this device.'),
      findsOne,
    );
    await _shotWithScroll(
      tester,
      'C1',
      'checklist_card_statuses',
      find.descendant(
        of: find.byType(BackupChecklist),
        matching: find.byType(Scrollable),
      ),
    );
    await _teardown(tester);
  }, skip: _skip);

  // Owner B15: the checklist on an install that didn't create the wallet.
  testWidgets('backup checklist: on an install that didn\'t create the '
      'wallet', (tester) async {
    final accessStructure = _setUpCardWallet();
    _fake.usesCard = false;
    _fake.setDevices([_device(_d4, name: 'Spare')]);
    await _pumpHost(tester);
    await tester.runAsync(() => Future<void>.delayed(Duration.zero));
    unawaited(
      MaybeFullscreenDialog.show(
        context: _homeContext,
        child: _walletContext(
          BackupChecklist(accessStructure: accessStructure, showAppBar: true),
          backupRun: _otherInstallBackupRun(),
        ),
      ),
    );
    await _settle(tester);
    expect(find.text(unverifiedRow), findsOne);
    expect(find.text(unverifiedChecklistLine), findsOne);
    expect(find.widgetWithText(FilledButton, 'Backup'), findsOne);
    await _shotWithScroll(
      tester,
      'C2',
      'checklist_unverified_install',
      find.descendant(
        of: find.byType(BackupChecklist),
        matching: find.byType(Scrollable),
      ),
    );
    _fake.usesCard = true;
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('wallet: banner, then the receive lock', (tester) async {
    final accessStructure = _setUpCardWallet();
    _fake.setDevices([]);
    await _pumpHost(
      tester,
      home: _walletContext(
        Scaffold(
          appBar: AppBar(title: const Text('Family savings')),
          body: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              BackupWarningBanner(frostKey: _fake.frostKey!, shrink: false),
            ],
          ),
          bottomNavigationBar: BottomAppBar(
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceEvenly,
              children: [
                TextButton.icon(
                  onPressed: () {},
                  icon: const Icon(Icons.south_east),
                  label: const Text('Receive'),
                ),
                TextButton.icon(
                  onPressed: () {},
                  icon: const Icon(Icons.north_east),
                  label: const Text('Send'),
                ),
              ],
            ),
          ),
        ),
      ),
    );
    await _settle(tester);
    await _shot(tester, 'W1', 'wallet_banner_not_checked');

    unawaited(showCardNotCheckedDialog(_homeContext, accessStructure));
    await _settle(tester);
    await _shot(tester, 'W2', 'receive_lock_dialog');
    await _teardown(tester);
  }, skip: _skip);

  // ---------------------------------------------------------- card flow --

  testWidgets('card flow: P1 intro and P2', (tester) async {
    final driver = await _openFlow(
      tester,
      _cs(const CardFlowStage.starting(), points: 0),
    );
    await _shotWithScroll(tester, 'P1', 'intro', _flowScrollable);

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.pinning(), points: 155),
    );
    await _shot(tester, 'P1', 'pinning');

    await _emit(tester, driver, _cs(const CardFlowStage.deviceHalf()));
    await _shot(tester, 'P2', 'write_words_1_32_on_device');

    await _emit(tester, driver, _cs(const CardFlowStage.confirmDeviceHalf()));
    await _shotWithScroll(
      tester,
      'P2c',
      'confirm_words_1_32_written',
      _flowScrollable,
    );
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('card flow: P1 on the desktop', (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    try {
      await _openFlow(tester, _cs(const CardFlowStage.preparingCard()));
      await _shotWithScroll(tester, 'P1', 'intro_desktop', _flowScrollable);
      await _teardown(tester);
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  }, skip: _skip);

  testWidgets('card flow: writing words 33–64 and the self-check', (
    tester,
  ) async {
    final driver = await _openFlow(
      tester,
      _cs(
        CardFlowStage.phoneWords(page: 0, firstWord: 33, words: _pageWords(0)),
        proofs: 40,
      ),
    );
    await _shot(tester, 'P3', 'words_33_35');

    await _emit(
      tester,
      driver,
      _cs(
        CardFlowStage.phoneWords(
          page: 10,
          firstWord: 63,
          words: _pageWords(10),
        ),
        proofs: 160,
      ),
    );
    await _shot(tester, 'P3', 'words_63_64');

    await _emit(
      tester,
      driver,
      _cs(CardFlowStage.phoneAllWords(words: fakePhoneHalf), proofs: 190),
    );
    await _shotWithScroll(tester, 'P4', 'all_words', _flowScrollable);

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.phoneConfirm(), proofs: 220),
    );
    await _shot(tester, 'P5', 'confirm_written');

    await _emit(
      tester,
      driver,
      _cs(_selfCheck(0, 32, 33, ['SIZE', 'SIDE', 'SISTER']), proofs: 250),
    );
    await _shot(tester, 'P6', 'self_check');

    await _emit(
      tester,
      driver,
      _cs(
        _selfCheck(4, 32, 37, ['TRIBE', 'TRIBUTE', 'BRIBE'], wrong: true),
        proofs: fakeSealSize,
      ),
    );
    await _shot(tester, 'P6', 'self_check_again_after_wrong');

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.proving(), halves: true, proofs: 280),
    );
    await _shotWithScroll(
      tester,
      'P7',
      'checked_continue_waits_for_proofs',
      _flowScrollable,
    );
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('card flow: P8 to P12', (tester) async {
    final driver = await _openFlow(
      tester,
      _cs(const CardFlowStage.proving(), proofs: 155),
    );
    await _shot(tester, 'Pproof', 'checking_your_card');

    await _emit(
      tester,
      driver,
      _cs(
        const CardFlowStage.readyForFinalCheck(),
        proofs: fakeSealSize,
        halves: true,
      ),
    );
    await _shotWithScroll(tester, 'P8', 'final_check_intro', _flowScrollable);

    // Owner B12: N is out (with the confirm of words 1–32), so no Stop.
    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.deviceRound(), started: true),
    );
    expect(find.widgetWithText(TextButton, 'Stop'), findsNothing);
    await _shot(tester, 'P9', 'answer_on_your_device');

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.preparingBox(k: 1), started: true, boxes: 1),
    );
    await _shot(tester, 'P10', 'preparing_question_2');

    await _emit(
      tester,
      driver,
      _cs(_question(1, 7, ['SIZE', 'SIDE', 'SISTER']), started: true, boxes: 1),
    );
    await _shotWithScroll(tester, 'P11', 'question_word_7', _flowScrollable);

    await _emit(
      tester,
      driver,
      _cs(
        _question(1, 7, ['SIZE', 'SIDE', 'SISTER'], secondLook: true),
        started: true,
        boxes: 1,
      ),
    );
    await _shotWithScroll(tester, 'P11', 'second_look', _flowScrollable);

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.roundSent(), started: true, boxes: 5),
    );
    await _shot(tester, 'P12', 'round_sent');

    await _emit(tester, driver, _cs(const CardFlowStage.passed(), boxes: 5));
    await _settle(tester, const Duration(milliseconds: 800));
    await _shot(tester, 'P12', 'backup_checked');
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('card flow: P15 a later check', (tester) async {
    final driver = await _openFlow(
      tester,
      _cs(const CardFlowStage.recheckWaiting()),
    );
    await _shot(tester, 'P15', 'recheck_waiting');

    await _emit(
      tester,
      driver,
      _cs(_selfCheck(33, 37, 7, ['ZOO', 'ZONE', 'ZERO'], recheck: true)),
    );
    await _shot(tester, 'P15', 'recheck_quiz');

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.recheckPassed(), deviceDone: true),
    );
    await _shot(tester, 'P15', 'recheck_passed');

    await _emit(tester, driver, _cs(const CardFlowStage.recheckUnavailable()));
    await _shot(tester, 'P15', 'recheck_unavailable');
    await _teardown(tester);
  }, skip: _skip);

  for (final (id, short, stage, connected) in [
    for (final kind in CardAlarmKind.values)
      (
        'P13',
        'alarm_${kind.name}',
        CardFlowStage.failed(alarm: previewAlarm(kind)),
        true,
      ),
    ('P13', 'different_card', const CardFlowStage.differentCard(), true),
    (
      'P13',
      'refused',
      const CardFlowStage.refused(
        message:
            "This device's key was restored, or made before backup cards, so it has no card to write. Keep the backup it has.",
      ),
      true,
    ),
    (
      'P13',
      'started_on_another_phone',
      const CardFlowStage.startedOnAnotherPhone(),
      true,
    ),
    ('P14', 'unplugged', const CardFlowStage.paused(), false),
    // Owner B9: the question after an interruption of the check.
    (
      'P16',
      'interrupted_disconnected',
      const CardFlowStage.checkInterrupted(disconnected: true, resumesLeft: 3),
      false,
    ),
    (
      'P16',
      'interrupted_on_opening',
      const CardFlowStage.checkInterrupted(disconnected: false, resumesLeft: 3),
      true,
    ),
    // Review B15 protocol F2: the question before the last resume.
    (
      'P16',
      'interrupted_last_resume',
      const CardFlowStage.checkInterrupted(disconnected: true, resumesLeft: 1),
      false,
    ),
    // Owner R2-2: the question after the last resume; either answer ends it.
    (
      'P16',
      'interrupted_too_often',
      const CardFlowStage.checkInterrupted(disconnected: true, resumesLeft: 0),
      false,
    ),
    ('Pstore', 'storage_error', const CardFlowStage.storageError(), true),
  ]) {
    testWidgets('card flow: $id $short', (tester) async {
      await _openFlow(
        tester,
        _cs(stage, connected: connected),
        plugged: connected,
      );
      await _shotWithScroll(tester, id, short, _flowScrollable);
      await _teardown(tester);
    }, skip: _skip);
  }

  // Owner B15: an install that didn't create the wallet writes the card
  // unverified. DRAFT COPY.
  testWidgets('card flow: U1 to U4, an unverified card', (tester) async {
    final driver = await _openFlow(
      tester,
      _cs(const CardFlowStage.unverifiedIntro(), unverified: true),
    );
    await _shotWithScroll(tester, 'U1', 'unverified_intro', _flowScrollable);

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.deviceHalf(), unverified: true),
    );
    await _shot(tester, 'U2', 'unverified_words_1_32_on_device');

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.confirmDeviceHalf(), unverified: true),
    );
    await _shotWithScroll(
      tester,
      'U2c',
      'unverified_confirm_words_1_32',
      _flowScrollable,
    );

    await _emit(
      tester,
      driver,
      _cs(
        CardFlowStage.phoneWords(page: 0, firstWord: 33, words: _pageWords(0)),
        unverified: true,
      ),
    );
    await _shot(tester, 'U3', 'unverified_words_33_35');

    await _emit(
      tester,
      driver,
      _cs(const CardFlowStage.unverifiedWritten(), unverified: true),
    );
    await _shotWithScroll(tester, 'U4', 'unverified_written', _flowScrollable);
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('card flow: paused, plugged back in', (tester) async {
    await _openFlow(
      tester,
      _cs(const CardFlowStage.paused(), connected: false),
    );
    await _shot(tester, 'P14', 'plugged_back_in');
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('card flow: start error', (tester) async {
    await _pumpHost(tester);
    final driver = _ShotDriver();
    unawaited(VerifiableCardFlow.show(_homeContext, driver: driver));
    await tester.pump();
    driver.fail(AnyhowException('Plug in this device first'));
    await _settle(tester);
    await _shot(tester, 'Pstart', 'start_error');
    await _teardown(tester);
  }, skip: _skip);

  // ------------------------------------------------------------ restore --

  testWidgets('restore: discovery hint', (tester) async {
    _fake.setDevices([]);
    await _pumpHost(tester);
    unawaited(
      MaybeFullscreenDialog.show(
        context: _homeContext,
        barrierDismissible: true,
        child: RecoveryFlowWithDiscovery(
          recoveryContext: RecoveryContext.newRestoration(),
        ),
      ),
    );
    await _settle(tester);
    await _shot(tester, 'RS0', 'restore_plug_in_hint');
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('restore: words 33–64 here, E1 to E5', (tester) async {
    await _pumpHost(tester);
    final driver = _ShotRestoreDriver();
    final failures = RestoreFailures();
    const failureKey = 'shots';
    unawaited(
      MaybeFullscreenDialog.show(
        context: _homeContext,
        child: _recoveryFrame(
          EnterCardView(
            deviceName: 'Kitchen',
            driver: driver,
            lookup: WordLookup(isWord: (_) => true),
            onFinished: (_) {},
            onCancel: () {},
            onError: (_) {},
            failures: failures,
            failureKey: failureKey,
          ),
        ),
      ),
    );
    driver.show(const CardRestoreView.waitingForDevice());
    await _settle(tester, const Duration(milliseconds: 1500));
    await _shot(tester, 'E1', 'restore_steps');

    driver.show(const CardRestoreView.enterHalves(deviceHalfIn: true));
    for (var i = 0; i < 8; i++) {
      await tester.enterText(
        find.byKey(ValueKey('word-${33 + i}')),
        fakePhoneHalf[i],
      );
    }
    await tester.enterText(find.byKey(const ValueKey('word-41')), 'ZOOO');
    FocusManager.instance.primaryFocus?.unfocus();
    await _settle(tester);
    await _shot(tester, 'E2', 'restore_entry_typed');

    for (var i = 0; i < 32; i++) {
      await tester.enterText(
        find.byKey(ValueKey('word-${33 + i}')),
        fakePhoneHalf[i],
      );
    }
    FocusManager.instance.primaryFocus?.unfocus();
    await _settle(tester);
    await tester.ensureVisible(find.byKey(const ValueKey('send-half')));
    await tester.tap(find.byKey(const ValueKey('send-half')));
    driver.show(const CardRestoreView.waitingForDeviceHalf());
    await _settle(tester);
    _scrollTop(tester, find.byType(EnterCardView));
    await _shot(tester, 'E4', 'restore_sent');

    driver.show(const CardRestoreView.checking());
    await _settle(tester);
    await _shot(tester, 'E4', 'restore_checking');

    driver.show(const CardRestoreView.couldNotRead());
    await _settle(tester);
    await _shot(tester, 'E5', 'restore_could_not_read');

    // The four failure steps (owner B14): the app's own count moves step 1
    // on to step 2; only the user's answers move steps 2 and 3 on.
    final restoreScroll = find.descendant(
      of: find.byType(EnterCardView),
      matching: find.byType(Scrollable),
    );
    driver.show(const CardRestoreView.checking());
    await _settle(tester);
    driver.show(const CardRestoreView.couldNotRead());
    await _settle(tester);
    expect(failures.step(failureKey), RestoreFailureStep.tryOtherApp);
    _scrollTop(tester, find.byType(EnterCardView));
    await _shot(tester, 'E5', 'restore_try_other_app');
    for (final (button, short) in [
      ("I've already tried it in a different app.", 'restore_try_other_device'),
      ("I don't have another Frostsnap device", 'restore_offline_recovery'),
    ]) {
      await tester.ensureVisible(find.text(button));
      await tester.tap(find.text(button));
      await _settle(tester);
      _scrollTop(tester, find.byType(EnterCardView));
      await _shotWithScroll(tester, 'E5', short, restoreScroll);
    }
    expect(failures.step(failureKey), RestoreFailureStep.offlineRecovery);
    await _teardown(tester);
  }, skip: _skip);

  testWidgets('restore: E6 the card read, finish on the device', (
    tester,
  ) async {
    await _pumpHost(tester);
    final driver = _ShotRestoreDriver();
    unawaited(
      MaybeFullscreenDialog.show(
        context: _homeContext,
        child: _recoveryFrame(
          EnterCardView(
            deviceName: 'Kitchen',
            driver: driver,
            lookup: WordLookup(isWord: (_) => true),
            onFinished: (_) {},
            onCancel: () {},
            onError: (_) {},
          ),
        ),
      ),
    );
    driver.show(const CardRestoreView.finishOnDevice());
    await _settle(tester, const Duration(milliseconds: 1500));
    await _shot(tester, 'E6', 'restore_finish_on_device');
    await _teardown(tester);
  }, skip: _skip);

  // Owner B11: a genuine card for another wallet is its own page.
  testWidgets('restore: E7 another wallet\'s card', (tester) async {
    await _pumpHost(tester);
    final driver = _ShotRestoreDriver();
    unawaited(
      MaybeFullscreenDialog.show(
        context: _homeContext,
        child: _recoveryFrame(
          EnterCardView(
            deviceName: 'Kitchen',
            driver: driver,
            lookup: WordLookup(isWord: (_) => true),
            onFinished: (_) {},
            onCancel: () {},
            onError: (_) {},
          ),
        ),
      ),
    );
    driver.show(const CardRestoreView.wrongWallet());
    await _settle(tester, const Duration(milliseconds: 1500));
    expect(find.text(wrongWalletLine), findsOne);
    await _shot(tester, 'E7', 'restore_wrong_wallet');
    await _teardown(tester);
  }, skip: _skip);
}

void _scrollTop(WidgetTester tester, Finder of) {
  final finder = find.descendant(of: of, matching: find.byType(Scrollable));
  if (finder.evaluate().isEmpty) return;
  tester.state<ScrollableState>(finder.first).position.jumpTo(0);
}
