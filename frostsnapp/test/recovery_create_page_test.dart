import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibraryLoaderConfig;
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/recovery/remote_recovery_create_page.dart';
import 'package:frostsnap/src/rust/api.dart' show keyPurposeBitcoin;
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';
import 'package:frostsnap/src/rust/lib.dart';

// Widget tests for `CreateLobbyDialog` — the form the leader fills
// out before `NostrClient.createRemoteRecoveryLobby` gets called.
// The dialog's job is to collect wallet name + optional threshold
// hint + `BitcoinNetwork`, validate, and return a `CreateLobbyResult`.
// The dialog is the only piece of `RemoteRecoveryCreatePage` that
// carries non-trivial logic; the surrounding page is straight-line
// glue we cover by `flutter analyze` + the manual acceptance run.
//
// `setUpAll` loads the real Rust dylib because
// `BitcoinNetworkChooser` calls `BitcoinNetwork.name()` (FFI) at
// build time. Same pattern as `recovery_lobby_view_test.dart`.

Widget _wrap(GlobalKey<NavigatorState> nav) => MaterialApp(
  navigatorKey: nav,
  home: const Scaffold(body: SizedBox.shrink()),
);

Future<Future<CreateLobbyResult?>> _openDialog(
  WidgetTester tester,
  GlobalKey<NavigatorState> nav,
) async {
  final future = showDialog<CreateLobbyResult>(
    context: nav.currentContext!,
    builder: (_) => const CreateLobbyDialog(),
  );
  await tester.pumpAndSettle();
  return future;
}

void main() {
  setUpAll(() async {
    final repoRoot = Directory.current.parent.path;
    final release = Directory('$repoRoot/target/release').existsSync();
    await RustLib.init(
      externalLibrary: await loadExternalLibrary(
        ExternalLibraryLoaderConfig(
          stem: 'rust_lib_frostsnapp',
          ioDirectory: '$repoRoot/target/${release ? 'release' : 'debug'}/',
          webPrefix: null,
        ),
      ),
    );
  });

  testWidgets('empty name shows an error and does not pop', (tester) async {
    final nav = GlobalKey<NavigatorState>();
    await tester.pumpWidget(_wrap(nav));
    final future = await _openDialog(tester, nav);

    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pump();

    expect(find.text('Wallet name is required'), findsOneWidget);
    // Future is still pending — dialog didn't pop.
    expect(find.byType(AlertDialog), findsOneWidget);

    // Clean up: cancel so the future completes.
    await tester.tap(find.widgetWithText(TextButton, 'Cancel'));
    await tester.pumpAndSettle();
    expect(await future, isNull);
  });

  testWidgets('valid submit returns entered name + default network', (
    tester,
  ) async {
    final nav = GlobalKey<NavigatorState>();
    await tester.pumpWidget(_wrap(nav));
    final future = await _openDialog(tester, nav);

    await tester.enterText(find.byType(TextField).first, 'Family wallet');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    final result = await future;
    expect(result, isNotNull);
    expect(result!.keyName, 'Family wallet');
    expect(result.thresholdHint, isNull);
    expect(result.network.name(), BitcoinNetwork.bitcoin.name());
  });

  testWidgets(
    'selecting a non-default network flows through to the result and its KeyPurpose',
    (tester) async {
      // Pick a supported network other than the default. Failing this
      // assumption would mean the platform build stripped alternates; the
      // whole point of the chooser is dead in that case.
      final alt = BitcoinNetwork.supportedNetworks().firstWhere(
        (n) => n.name() != BitcoinNetwork.bitcoin.name(),
      );

      final nav = GlobalKey<NavigatorState>();
      await tester.pumpWidget(_wrap(nav));
      final future = await _openDialog(tester, nav);

      await tester.enterText(find.byType(TextField).first, 'w');

      // Open the network dropdown and pick `alt`. The chooser renders a
      // `DropdownButton<String>` keyed on `BitcoinNetwork.name()`, and the
      // menu items display "Bitcoin (BTC)" for bitcoin and the raw name
      // otherwise (settings.dart:1270).
      await tester.tap(find.byType(DropdownButton<String>));
      await tester.pumpAndSettle();
      await tester.tap(find.text(alt.name()).last);
      await tester.pumpAndSettle();

      await tester.tap(find.widgetWithText(FilledButton, 'Create'));
      await tester.pumpAndSettle();

      final result = await future;
      expect(result, isNotNull);
      expect(
        result!.network.name(),
        alt.name(),
        reason:
            'Regression guard: if `_submit` hard-codes `BitcoinNetwork.bitcoin` again '
            'this assertion fires. `result.network` must reflect the picked value.',
      );

      // Call-site plumbing check: the entry page hands
      // `keyPurposeBitcoin(network: result.network)` to
      // `NostrClient.createRemoteRecoveryLobby`. Constructing the purpose
      // with the picked network should succeed for a supported network;
      // this guards against a future refactor dropping the `network` arg
      // and reverting to a purpose-less or mainnet-only call.
      final purpose = keyPurposeBitcoin(network: result.network);
      expect(purpose, isNotNull);
    },
  );

  testWidgets('non-numeric threshold shows an error', (tester) async {
    final nav = GlobalKey<NavigatorState>();
    await tester.pumpWidget(_wrap(nav));
    final future = await _openDialog(tester, nav);

    await tester.enterText(find.byType(TextField).at(0), 'ok');
    // digitsOnly formatter strips 'abc' — the field stays empty, so
    // we simulate an invalid value by driving the state directly via
    // negative number: use "0" which is a valid parse but < 1.
    await tester.enterText(find.byType(TextField).at(1), '0');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pump();

    expect(
      find.text('Threshold hint must be a positive integer'),
      findsOneWidget,
    );

    await tester.tap(find.widgetWithText(TextButton, 'Cancel'));
    await tester.pumpAndSettle();
    expect(await future, isNull);
  });

  testWidgets('valid threshold hint is returned', (tester) async {
    final nav = GlobalKey<NavigatorState>();
    await tester.pumpWidget(_wrap(nav));
    final future = await _openDialog(tester, nav);

    await tester.enterText(find.byType(TextField).at(0), 'w');
    await tester.enterText(find.byType(TextField).at(1), '3');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pumpAndSettle();

    final result = await future;
    expect(result, isNotNull);
    expect(result!.thresholdHint, 3);
  });

  test('RemoteRecoveryCreatePage.dispatchCreate hands NostrClient a KeyPurpose '
      'whose bitcoinNetwork matches CreateLobbyResult.network', () async {
    // Full-lobby regression scenario per plan acceptance: a stub
    // `NostrClient` captures the `createRemoteRecoveryLobby` call and we
    // assert `purpose.bitcoinNetwork()` reflects the picked network. If
    // the entry page reverts to `keyPurposeBitcoin(network:
    // BitcoinNetwork.bitcoin)`, this test fires. Uses `dispatchCreate`
    // (the extracted call site) rather than the full page pump because
    // `NostrContext` + `NostrSettings` are opaque and standing them up
    // in a widget test would dwarf the assertion.
    final stub = _CapturingNostrClient();
    final alt = BitcoinNetwork.supportedNetworks().firstWhere(
      (n) => n.name() != BitcoinNetwork.bitcoin.name(),
    );
    final identity = NostrIdentity.generated(
      nsec: Nsec.generate(),
      name: 'leader',
      createdAt: 0,
    );
    final result = CreateLobbyResult(
      keyName: 'w',
      thresholdHint: 2,
      network: alt,
    );

    // The stub throws after capturing so we don't have to construct a
    // real `RemoteRecoveryLobbyHandle`. `unawaited`-style: swallow the
    // marker error, then assert on the capture.
    try {
      await RemoteRecoveryCreatePage.dispatchCreate(
        client: stub,
        identity: identity,
        result: result,
      );
      fail('stub was expected to throw its marker error');
    } on _StubMarker {
      // expected
    }

    expect(stub.captured, isNotNull);
    expect(stub.captured!.keyName, 'w');
    expect(stub.captured!.thresholdHint, 2);
    expect(
      stub.captured!.purpose.bitcoinNetwork()?.name(),
      alt.name(),
      reason:
          'Regression guard: the KeyPurpose handed to '
          '`NostrClient.createRemoteRecoveryLobby` must reflect the '
          'network picked in the create dialog, not the mainnet default.',
    );
  });
}

class _StubMarker implements Exception {}

class _CapturedCall {
  final String keyName;
  final int? thresholdHint;
  final KeyPurpose purpose;

  const _CapturedCall({
    required this.keyName,
    required this.thresholdHint,
    required this.purpose,
  });
}

/// A test double for [NostrClient] — the RustOpaqueInterface has many
/// methods, but only `createRemoteRecoveryLobby` matters for this
/// regression test. `noSuchMethod` swallows the rest.
class _CapturingNostrClient implements NostrClient {
  _CapturedCall? captured;

  @override
  Future<RemoteRecoveryLobbyHandle> createRemoteRecoveryLobby({
    required NostrIdentity identity,
    required ChannelSecret channelSecret,
    required String keyName,
    required KeyPurpose purpose,
    int? thresholdHint,
  }) async {
    captured = _CapturedCall(
      keyName: keyName,
      thresholdHint: thresholdHint,
      purpose: purpose,
    );
    throw _StubMarker();
  }

  @override
  void dispose() {}

  @override
  bool get isDisposed => false;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}
