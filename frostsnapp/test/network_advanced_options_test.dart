import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/network_advanced_options.dart';
import 'package:frostsnap/restoration/enter_wallet_name_view.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/settings.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';
import 'package:frostsnap/wallet_create.dart';

class _FakeApi implements RustLibApi {
  @override
  List<BitcoinNetwork> crateApiBitcoinBitcoinNetworkSupportedNetworks() =>
      BitcoinNetwork.values;

  @override
  bool crateApiBitcoinBitcoinNetworkIsMainnet({required BitcoinNetwork that}) =>
      that == BitcoinNetwork.bitcoin;

  @override
  String crateApiBitcoinBitcoinNetworkName({required BitcoinNetwork that}) =>
      EnumName(that).name;

  @override
  int crateApiNameKeyNameMaxLength() => 20;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _FakeSettings implements Settings {
  _FakeSettings({required this.developerMode});

  final bool developerMode;

  @override
  bool isInDeveloperMode() => developerMode;

  @override
  Stream<DeveloperSettings> subDeveloperSettings() => const Stream.empty();

  @override
  Stream<ElectrumSettings> subElectrumSettings() => const Stream.empty();

  @override
  Stream<DisplaySettings> subDisplaySettings() => const Stream.empty();

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _FakeCoordinator implements Coordinator {
  @override
  Stream<DeviceListUpdate> subDeviceEvents() => Stream.value(
    const DeviceListUpdate(
      changes: [],
      state: DeviceListState(devices: [], stateId: 0),
    ),
  );

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

Widget _app(Widget child, {bool developerMode = true}) => SettingsContext(
  settings: _FakeSettings(developerMode: developerMode),
  child: MaterialApp(home: Scaffold(body: child)),
);

Future<void> _choose(WidgetTester tester, String displayName) async {
  await tester.tap(find.text('Developer'));
  await tester.pumpAndSettle();
  await tester.tap(
    find.descendant(
      of: find.byType(SegmentedButton<BitcoinNetwork>),
      matching: find.text(displayName),
    ),
  );
  await tester.pumpAndSettle();
}

Finder _chip(String label) =>
    find.widgetWithText(InputChip, label, skipOffstage: false);

void main() {
  setUpAll(() {
    RustLib.initMock(api: _FakeApi());
    coord = _FakeCoordinator();
  });

  group('NetworkAdvancedOptions', () {
    Future<List<BitcoinNetwork>> pumpPicker(
      WidgetTester tester,
      BitcoinNetwork initial,
    ) async {
      final reported = <BitcoinNetwork>[];
      var selected = initial;
      await tester.pumpWidget(
        _app(
          StatefulBuilder(
            builder: (context, setState) => NetworkAdvancedOptions(
              selected: selected,
              onChanged: (network) {
                reported.add(network);
                setState(() => selected = network);
              },
            ),
          ),
        ),
      );
      return reported;
    }

    testWidgets('no chip on mainnet', (tester) async {
      await pumpPicker(tester, BitcoinNetwork.bitcoin);
      expect(find.byType(InputChip), findsNothing);
    });

    testWidgets('choosing a network reports it and shows its chip', (
      tester,
    ) async {
      final reported = await pumpPicker(tester, BitcoinNetwork.bitcoin);
      await _choose(tester, 'Testnet3');
      expect(reported, [BitcoinNetwork.testnet]);
      expect(_chip('Testnet3'), findsOneWidget);
    });

    testWidgets('dismissing the chip reports mainnet and hides it', (
      tester,
    ) async {
      final reported = await pumpPicker(tester, BitcoinNetwork.signet);
      expect(_chip('Signet'), findsOneWidget);
      await tester.tap(find.byIcon(Icons.clear_rounded));
      await tester.pumpAndSettle();
      expect(reported, [BitcoinNetwork.bitcoin]);
      expect(find.byType(InputChip), findsNothing);
    });
  });

  group('wallet create', () {
    testWidgets('hidden outside developer mode', (tester) async {
      await tester.pumpWidget(
        _app(const WalletCreatePage(), developerMode: false),
      );
      await tester.pumpAndSettle();
      expect(find.byType(NetworkAdvancedOptions), findsNothing);
    });

    testWidgets('choosing a network sets the wallet network', (tester) async {
      await tester.pumpWidget(_app(const WalletCreatePage()));
      await tester.pumpAndSettle();
      expect(find.byType(InputChip), findsNothing);
      await _choose(tester, 'Signet');
      expect(_chip('Signet'), findsOneWidget);
      expect(find.textContaining('(Signet)'), findsOneWidget);
    });
  });

  group('restore wallet name', () {
    Future<List<BitcoinNetwork>> pumpView(
      WidgetTester tester, {
      bool developerMode = true,
    }) async {
      final entered = <BitcoinNetwork>[];
      await tester.pumpWidget(
        _app(
          EnterWalletNameView(
            onWalletNameEntered: (_, network) => entered.add(network),
          ),
          developerMode: developerMode,
        ),
      );
      return entered;
    }

    testWidgets('hidden outside developer mode', (tester) async {
      await pumpView(tester, developerMode: false);
      expect(find.byType(NetworkAdvancedOptions), findsNothing);
    });

    testWidgets('the chosen network is submitted with the name', (
      tester,
    ) async {
      final entered = await pumpView(tester);
      expect(find.byType(InputChip), findsNothing);
      await _choose(tester, 'Regtest');
      expect(_chip('Regtest'), findsOneWidget);
      await tester.enterText(find.byType(TextFormField), 'Savings');
      await tester.pump();
      await tester.tap(find.text('Continue'));
      await tester.pump();
      expect(entered, [BitcoinNetwork.regtest]);
    });
  });
}
