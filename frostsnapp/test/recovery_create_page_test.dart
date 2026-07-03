import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibraryLoaderConfig;
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/recovery/remote_recovery_page.dart';
import 'package:frostsnap/restoration/enter_threshold_view.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';
import 'package:frostsnap/src/rust/lib.dart';

// The create flow reuses local recovery's step views. This file
// covers the remote-specific seams:
//
// - `EnterThresholdView` interaction (context-free, so driven
//   directly): the "I'm not sure" / "I know the threshold" selector
//   maps to `thresholdHint = null` / N. The view is the same widget
//   local recovery renders; the wallet-name step
//   (`EnterWalletNameView`) needs a live `SettingsContext` for its
//   dev-mode network gate and is already exercised by local
//   recovery, so it isn't re-pumped here.
// - `RemoteRecoveryPage.dispatchCreate`: a stub `NostrClient`
//   captures the `createRemoteRecoveryLobby` call, guarding that
//   `CreateLobbyResult.network` flows into the `KeyPurpose` (not a
//   mainnet hard-code).
//
// `setUpAll` loads the real Rust dylib for the FFI types
// (`keyPurposeBitcoin`, `Nsec.generate`).

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

  Widget wrap(Widget child) => MaterialApp(home: Scaffold(body: child));

  testWidgets("threshold step defaults to I'm-not-sure → null hint", (
    tester,
  ) async {
    int? submitted = -1; // sentinel: distinguish "null" from "not called"
    var called = false;
    await tester.pumpWidget(
      wrap(
        EnterThresholdView(
          onSubmit: (threshold) {
            called = true;
            submitted = threshold;
          },
        ),
      ),
    );
    tester
        .state<EnterThresholdViewState>(find.byType(EnterThresholdView))
        .submit();
    expect(called, isTrue);
    expect(submitted, isNull);
  });

  testWidgets('threshold step with I-know-the-threshold returns N', (
    tester,
  ) async {
    int? submitted;
    await tester.pumpWidget(
      wrap(EnterThresholdView(onSubmit: (threshold) => submitted = threshold)),
    );
    await tester.tap(find.text('I know the threshold'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), '3');
    tester
        .state<EnterThresholdViewState>(find.byType(EnterThresholdView))
        .submit();
    expect(submitted, 3);
  });

  testWidgets('threshold step blocks empty value when I-know is selected', (
    tester,
  ) async {
    var called = false;
    await tester.pumpWidget(
      wrap(EnterThresholdView(onSubmit: (_) => called = true)),
    );
    await tester.tap(find.text('I know the threshold'));
    await tester.pumpAndSettle();
    tester
        .state<EnterThresholdViewState>(find.byType(EnterThresholdView))
        .submit();
    await tester.pump();
    expect(called, isFalse);
    expect(find.text('Please enter a threshold'), findsOneWidget);
  });

  test('RemoteRecoveryPage.dispatchCreate hands NostrClient a KeyPurpose '
      'whose bitcoinNetwork matches CreateLobbyResult.network', () async {
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
          'network picked in the create flow, not the mainnet default.',
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
