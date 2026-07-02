import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibraryLoaderConfig;
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/recovery/remote_recovery_page.dart';
import 'package:frostsnap/src/rust/api.dart' show keyPurposeBitcoin;
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';
import 'package:frostsnap/src/rust/lib.dart';

// Widget tests for `CreateLobbyForm` — the ceremony step the leader
// fills out before `NostrClient.createRemoteRecoveryLobby` gets
// called. The form's job is to collect wallet name + optional
// threshold hint + `BitcoinNetwork`, validate, and surface a
// `CreateLobbyResult` through `onSubmit`.
//
// `setUpAll` loads the real Rust dylib because
// `NetworkAdvancedOptions` calls `BitcoinNetwork.name()` (FFI) at
// build time. Same pattern as `recovery_lobby_view_test.dart`.

Widget _wrap(Widget child) => MaterialApp(home: Scaffold(body: child));

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

  testWidgets('empty name shows an error and does not submit', (tester) async {
    CreateLobbyResult? submitted;
    await tester.pumpWidget(
      _wrap(CreateLobbyForm(onSubmit: (r) => submitted = r)),
    );

    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pump();

    expect(find.text('Wallet name is required'), findsOneWidget);
    expect(submitted, isNull);
  });

  testWidgets('valid submit returns entered name + default network', (
    tester,
  ) async {
    CreateLobbyResult? submitted;
    await tester.pumpWidget(
      _wrap(CreateLobbyForm(onSubmit: (r) => submitted = r)),
    );

    await tester.enterText(find.byType(TextField).first, 'Family wallet');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pump();

    expect(submitted, isNotNull);
    expect(submitted!.keyName, 'Family wallet');
    expect(submitted!.thresholdHint, isNull);
    expect(submitted!.network.name(), BitcoinNetwork.bitcoin.name());
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

      CreateLobbyResult? submitted;
      await tester.pumpWidget(
        _wrap(CreateLobbyForm(onSubmit: (r) => submitted = r)),
      );

      await tester.enterText(find.byType(TextField).first, 'w');

      // Reveal the network picker (NetworkAdvancedOptions hides the
      // SegmentedButton behind the 'Developer' toggle) and pick `alt`.
      await tester.tap(find.text('Developer'));
      await tester.pumpAndSettle();
      await tester.tap(find.text(alt.name()).last);
      await tester.pumpAndSettle();

      await tester.tap(find.widgetWithText(FilledButton, 'Create'));
      await tester.pump();

      expect(submitted, isNotNull);
      expect(
        submitted!.network.name(),
        alt.name(),
        reason:
            'Regression guard: if `_submit` hard-codes `BitcoinNetwork.bitcoin` again '
            'this assertion fires. `result.network` must reflect the picked value.',
      );

      final purpose = keyPurposeBitcoin(network: submitted!.network);
      expect(purpose, isNotNull);
    },
  );

  testWidgets('zero threshold shows an error', (tester) async {
    CreateLobbyResult? submitted;
    await tester.pumpWidget(
      _wrap(CreateLobbyForm(onSubmit: (r) => submitted = r)),
    );

    await tester.enterText(find.byType(TextField).at(0), 'ok');
    // digitsOnly formatter strips letters; "0" is a valid parse but < 1.
    await tester.enterText(find.byType(TextField).at(1), '0');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pump();

    expect(
      find.text('Threshold hint must be a positive integer'),
      findsOneWidget,
    );
    expect(submitted, isNull);
  });

  testWidgets('valid threshold hint is returned', (tester) async {
    CreateLobbyResult? submitted;
    await tester.pumpWidget(
      _wrap(CreateLobbyForm(onSubmit: (r) => submitted = r)),
    );

    await tester.enterText(find.byType(TextField).at(0), 'w');
    await tester.enterText(find.byType(TextField).at(1), '3');
    await tester.tap(find.widgetWithText(FilledButton, 'Create'));
    await tester.pump();

    expect(submitted, isNotNull);
    expect(submitted!.thresholdHint, 3);
  });

  testWidgets('busy form does not submit', (tester) async {
    CreateLobbyResult? submitted;
    await tester.pumpWidget(
      _wrap(CreateLobbyForm(busy: true, onSubmit: (r) => submitted = r)),
    );

    await tester.enterText(find.byType(TextField).first, 'w');
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Create'),
    );
    expect(button.onPressed, isNull);
    expect(submitted, isNull);
  });

  test('RemoteRecoveryPage.dispatchCreate hands NostrClient a KeyPurpose '
      'whose bitcoinNetwork matches CreateLobbyResult.network', () async {
    // Full-lobby regression scenario per plan acceptance: a stub
    // `NostrClient` captures the `createRemoteRecoveryLobby` call and we
    // assert `purpose.bitcoinNetwork()` reflects the picked network. If
    // the create page reverts to `keyPurposeBitcoin(network:
    // BitcoinNetwork.bitcoin)`, this test fires.
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
    // real `RemoteRecoveryLobbyHandle`.
    try {
      await RemoteRecoveryPage.dispatchCreate(
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
          'network picked in the create form, not the mainnet default.',
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
