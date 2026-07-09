import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibraryLoaderConfig;
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:rxdart/rxdart.dart';

// `PreparingDevicesStep` sequences the lobby's add-devices submit:
// devices must have signing-nonce streams on record BEFORE they are
// posted as ready, or the wallet can never sign
// (`NotEnoughNoncesForDevice`). The nonce request/replenish seams are
// injected so no live coordinator is needed; the real dylib is loaded
// only for `NonceReplenishState.isFinished()` (a sync FFI call the
// indicator makes on every state).

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

  DeviceId devId(int b) {
    final bytes = Uint8List(33);
    bytes[0] = b;
    return DeviceId(field0: U8Array33(bytes));
  }

  NonceReplenishState state({int done = 0, int total = 2, bool abort = false}) {
    return NonceReplenishState(
      devices: {devId(1)},
      completedStreams: done,
      totalStreams: total,
      abort: abort,
    );
  }

  // Drives the indicator through its completion animations
  // (value fill 600ms, celebration 800ms) — pumpAndSettle can't be
  // used because the progress ring pulses forever.
  Future<void> pumpThroughCelebration(WidgetTester tester) async {
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 700));
    await tester.pump(const Duration(milliseconds: 900));
    await tester.pump();
  }

  testWidgets('nonces needed: indicator shows, completion fires '
      'onComplete exactly once, protocol not cancelled', (tester) async {
    final subject = BehaviorSubject<NonceReplenishState>.seeded(state());
    var completed = 0;
    final failures = <String>[];
    var cancels = 0;

    await tester.pumpWidget(
      wrap(
        PreparingDevicesStep(
          devices: [devId(1)],
          noncesNeeded: (_) => true,
          replenishNonces: (_) => subject,
          cancelProtocol: () => cancels++,
          onComplete: () => completed++,
          onFailed: failures.add,
        ),
      ),
    );
    await tester.pump();
    expect(find.byType(NonceReplenishIndicator), findsOneWidget);
    expect(completed, 0);

    subject.add(state(done: 2));
    await pumpThroughCelebration(tester);
    expect(completed, 1);
    expect(failures, isEmpty);

    // Terminal reached: unmounting must NOT cancel (a later protocol
    // could be running by then).
    await tester.pumpWidget(const SizedBox());
    expect(cancels, 0);
    expect(completed, 1);
    await subject.close();
  });

  testWidgets('abort fires onFailed, never onComplete, no cancel', (
    tester,
  ) async {
    final subject = BehaviorSubject<NonceReplenishState>.seeded(state());
    var completed = 0;
    final failures = <String>[];
    var cancels = 0;

    await tester.pumpWidget(
      wrap(
        PreparingDevicesStep(
          devices: [devId(1)],
          noncesNeeded: (_) => true,
          replenishNonces: (_) => subject,
          cancelProtocol: () => cancels++,
          onComplete: () => completed++,
          onFailed: failures.add,
        ),
      ),
    );
    await tester.pump();

    subject.add(state(abort: true));
    await tester.pump();
    await tester.pump();
    expect(failures, hasLength(1));
    expect(completed, 0);

    await tester.pumpWidget(const SizedBox());
    expect(cancels, 0);
    await subject.close();
  });

  testWidgets('nothing requested: onComplete immediately, indicator and '
      'replenish never touched', (tester) async {
    var completed = 0;
    final failures = <String>[];
    var replenishCalls = 0;
    var cancels = 0;

    await tester.pumpWidget(
      wrap(
        PreparingDevicesStep(
          devices: [devId(1)],
          noncesNeeded: (_) => false,
          replenishNonces: (_) {
            replenishCalls++;
            return const Stream.empty();
          },
          cancelProtocol: () => cancels++,
          onComplete: () => completed++,
          onFailed: failures.add,
        ),
      ),
    );
    await tester.pump();
    expect(completed, 1);
    expect(failures, isEmpty);
    expect(find.byType(NonceReplenishIndicator), findsNothing);
    expect(replenishCalls, 0);

    await tester.pumpWidget(const SizedBox());
    expect(cancels, 0);
  });

  testWidgets('unmount mid-replenish cancels the protocol once', (
    tester,
  ) async {
    final subject = BehaviorSubject<NonceReplenishState>.seeded(state());
    var completed = 0;
    final failures = <String>[];
    var cancels = 0;

    await tester.pumpWidget(
      wrap(
        PreparingDevicesStep(
          devices: [devId(1)],
          noncesNeeded: (_) => true,
          replenishNonces: (_) => subject,
          cancelProtocol: () => cancels++,
          onComplete: () => completed++,
          onFailed: failures.add,
        ),
      ),
    );
    await tester.pump();

    await tester.pumpWidget(const SizedBox());
    expect(cancels, 1);
    expect(completed, 0);
    expect(failures, isEmpty);
    await subject.close();
  });
}
