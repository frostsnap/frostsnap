import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibraryLoaderConfig;
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/recovery/remote_recovery_lobby_page.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/nostr.dart';
import 'package:frostsnap/src/rust/api/nostr/remote_recovery.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';
import 'package:frostsnap/src/rust/lib.dart';

// Widget test drives `RecoveryLobbyView` directly. The pure-UI
// widget was extracted from `RemoteRecoveryLobbyPage` precisely so
// the state → UI mapping can be tested without a live
// `RemoteRecoveryLobbyHandle` (a RustOpaqueInterface).
//
// Loads the real Rust library once so it can construct opaque
// values like `KeyPurpose` via `keyPurposeBitcoin`. Only
// NostrProfile.displayName-populated participants are used so
// `PublicKey.toNpub()` (which would also require the FFI) never
// gets called by `_ParticipantRow._displayName` — but the FFI is
// there if that path ever fires.
//
// The `shares` list is left empty in every test case: no test
// asserts against share-list rendering, and `ObservedShare`
// requires an opaque `ShareImage` we can't cheaply build outside
// a full recovery flow. Progress derives from
// `state.currentRecovery` + `state.shares.length` — a length of 0
// still exercises the "waiting for shares" branch cleanly.

PublicKey _pk(int seed) {
  final bytes = Uint8List(32);
  for (var i = 0; i < 32; i++) {
    bytes[i] = seed;
  }
  return PublicKey(field0: U8Array32(bytes));
}

EventId _eid(int seed) {
  final bytes = Uint8List(32);
  for (var i = 0; i < 32; i++) {
    bytes[i] = seed;
  }
  return EventId(field0: U8Array32(bytes));
}

AccessStructureRef _asref() => AccessStructureRef(
  keyId: KeyId(field0: U8Array32(Uint8List(32))),
  accessStructureId: AccessStructureId(field0: U8Array32(Uint8List(32))),
);

late final KeyPurpose _bitcoinRegtest;

RecoveryChannelMetadata _meta() => RecoveryChannelMetadata(
  keyName: 'Test wallet',
  purpose: _bitcoinRegtest,
  thresholdHint: 2,
);

NostrProfile _profile(String displayName) =>
    NostrProfile(displayName: displayName);

RecoveryParticipantInfo _participant({
  required int seed,
  required String name,
  List<EventId> posted = const [],
  bool left = false,
}) => RecoveryParticipantInfo(
  pubkey: _pk(seed),
  joinedAtSecs: seed,
  profile: _profile(name),
  postedShares: posted,
  left: left,
);

Widget _wrap(Widget child) => MaterialApp(home: child);

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
    _bitcoinRegtest = keyPurposeBitcoin(network: BitcoinNetwork.regtest);
  });

  final me = _pk(0x11);
  final peer1 = _pk(0x22);
  final peer2 = _pk(0x33);

  testWidgets('empty state shows loading indicator', (tester) async {
    await tester.pumpWidget(
      _wrap(
        RecoveryLobbyView(
          state: null,
          isLeader: true,
          myPubkey: me,
          inviteLink: 'frostsnap://recovery/deadbeef',
          finishing: false,
          persisting: false,
          error: null,
          recoveredRef: null,
          verificationFailed: false,
          onFinish: () async {},
          onCancel: () async {},
        ),
      ),
    );
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
  });

  testWidgets('leader Recover button disabled with no currentRecovery', (
    tester,
  ) async {
    final state = RecoveryLobbyState(
      metadata: _meta(),
      participants: {me: _participant(seed: 0x11, name: 'me')},
      shares: const [],
      currentRecovery: null,
      finished: null,
      cancelled: false,
    );
    await tester.pumpWidget(
      _wrap(
        RecoveryLobbyView(
          state: state,
          isLeader: true,
          myPubkey: me,
          inviteLink: 'frostsnap://recovery/deadbeef',
          finishing: false,
          persisting: false,
          error: null,
          recoveredRef: null,
          verificationFailed: false,
          onFinish: () async {},
          onCancel: () async {},
        ),
      ),
    );
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Recover'),
    );
    expect(button.onPressed, isNull);
  });

  testWidgets('leader Recover enables when currentRecovery is present', (
    tester,
  ) async {
    var pressed = 0;
    final state = RecoveryLobbyState(
      metadata: _meta(),
      participants: {
        me: _participant(seed: 0x11, name: 'me', posted: [_eid(1)]),
        peer1: _participant(seed: 0x22, name: 'peer 1', posted: [_eid(2)]),
        peer2: _participant(seed: 0x33, name: 'peer 2', posted: [_eid(3)]),
      },
      shares: const [],
      currentRecovery: RecoveredKey(
        accessStructureRef: _asref(),
        winningShareRefs: [_eid(1), _eid(2), _eid(3)],
      ),
      finished: null,
      cancelled: false,
    );
    await tester.pumpWidget(
      _wrap(
        RecoveryLobbyView(
          state: state,
          isLeader: true,
          myPubkey: me,
          inviteLink: 'frostsnap://recovery/deadbeef',
          finishing: false,
          persisting: false,
          error: null,
          recoveredRef: null,
          verificationFailed: false,
          onFinish: () async {
            pressed += 1;
          },
          onCancel: () async {},
        ),
      ),
    );
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Recover'),
    );
    expect(button.onPressed, isNotNull);
    await tester.tap(find.widgetWithText(FilledButton, 'Recover'));
    await tester.pump();
    expect(pressed, 1);
  });

  testWidgets(
    'leader Recover disabled when verification failed even if recovery available',
    (tester) async {
      final state = RecoveryLobbyState(
        metadata: _meta(),
        participants: {me: _participant(seed: 0x11, name: 'me')},
        shares: const [],
        currentRecovery: RecoveredKey(
          accessStructureRef: _asref(),
          winningShareRefs: [_eid(1)],
        ),
        finished: null,
        cancelled: false,
      );
      await tester.pumpWidget(
        _wrap(
          RecoveryLobbyView(
            state: state,
            isLeader: true,
            myPubkey: me,
            inviteLink: 'frostsnap://recovery/deadbeef',
            finishing: false,
            persisting: false,
            error: null,
            recoveredRef: null,
            verificationFailed: true,
            onFinish: () async {},
            onCancel: () async {},
          ),
        ),
      );
      final button = tester.widget<FilledButton>(
        find.widgetWithText(FilledButton, 'Recover'),
      );
      expect(button.onPressed, isNull);
    },
  );

  testWidgets('finished + persisting shows Persisting banner', (tester) async {
    final state = RecoveryLobbyState(
      metadata: _meta(),
      participants: {me: _participant(seed: 0x11, name: 'me')},
      shares: const [],
      currentRecovery: null,
      finished: FinishedRecovery(
        accessStructureRef: _asref(),
        shareRefs: [_eid(1)],
      ),
      cancelled: false,
    );
    await tester.pumpWidget(
      _wrap(
        RecoveryLobbyView(
          state: state,
          isLeader: true,
          myPubkey: me,
          inviteLink: 'frostsnap://recovery/deadbeef',
          finishing: false,
          persisting: true,
          error: null,
          recoveredRef: null,
          verificationFailed: false,
          onFinish: () async {},
          onCancel: () async {},
        ),
      ),
    );
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, 'Recover'),
    );
    expect(button.onPressed, isNull, reason: 'Recover disabled once finished');
    expect(find.textContaining('Persisting'), findsOneWidget);
  });

  testWidgets('recovered state shows Recovered banner', (tester) async {
    final state = RecoveryLobbyState(
      metadata: _meta(),
      participants: {me: _participant(seed: 0x11, name: 'me')},
      shares: const [],
      currentRecovery: null,
      finished: FinishedRecovery(
        accessStructureRef: _asref(),
        shareRefs: [_eid(1)],
      ),
      cancelled: false,
    );
    await tester.pumpWidget(
      _wrap(
        RecoveryLobbyView(
          state: state,
          isLeader: false,
          myPubkey: me,
          inviteLink: 'frostsnap://recovery/deadbeef',
          finishing: false,
          persisting: false,
          error: null,
          recoveredRef: _asref(),
          verificationFailed: false,
          onFinish: () async {},
          onCancel: () async {},
        ),
      ),
    );
    expect(find.textContaining('Recovered'), findsOneWidget);
  });

  testWidgets('cancelled state shows cancellation notice', (tester) async {
    final state = RecoveryLobbyState(
      metadata: _meta(),
      participants: {me: _participant(seed: 0x11, name: 'me')},
      shares: const [],
      currentRecovery: null,
      finished: null,
      cancelled: true,
    );
    await tester.pumpWidget(
      _wrap(
        RecoveryLobbyView(
          state: state,
          isLeader: false,
          myPubkey: me,
          inviteLink: 'frostsnap://recovery/deadbeef',
          finishing: false,
          persisting: false,
          error: null,
          recoveredRef: null,
          verificationFailed: false,
          onFinish: () async {},
          onCancel: () async {},
        ),
      ),
    );
    expect(find.textContaining('cancelled by leader'), findsOneWidget);
  });
}
