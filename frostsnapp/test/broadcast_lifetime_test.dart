import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibraryLoaderConfig;
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api/broadcast_test.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';

class _StreamBuilderHost extends StatefulWidget {
  const _StreamBuilderHost(this.owner, {super.key});
  final TestBroadcastHandle owner;
  @override
  State<_StreamBuilderHost> createState() => _StreamBuilderHostState();
}

class _StreamBuilderHostState extends State<_StreamBuilderHost> {
  int _rebuildTick = 0;
  void forceRebuild() => setState(() => _rebuildTick++);

  @override
  Widget build(BuildContext context) => Column(
    children: [
      Text('$_rebuildTick'),
      StreamBuilder<void>(
        // Fresh leaf + fresh Stream per build — matches the planned
        // `nostrSettings.accessStructure(asRef).watch()` shape in
        // wallet.dart / settings.dart, where the owner mints a new
        // leaf on each call.
        stream: widget.owner.broadcast().watch(),
        builder: (context, _) => const SizedBox.shrink(),
      ),
    ],
  );
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

  test(
    'cancel() Future completes promptly; subscriberCount drops to 0',
    () async {
      final owner = TestBroadcastHandle.create();
      expect(owner.subscriberCount(), 0);

      final received = <void>[];
      final streamSub = owner.broadcast().watch().listen(received.add);

      await Future.delayed(const Duration(milliseconds: 50));
      owner.fire();
      await Future.delayed(const Duration(milliseconds: 50));
      expect(received.length, 1);
      expect(owner.subscriberCount(), 1);

      // The load-bearing assertion: cancel() must complete, not deadlock.
      await streamSub.cancel().timeout(const Duration(seconds: 1));
      expect(owner.subscriberCount(), 0);

      owner.fire();
      await Future.delayed(const Duration(milliseconds: 50));
      expect(received.length, 1, reason: 'no events after cancel');
    },
  );

  test('no Rust registration before .listen()', () async {
    final owner = TestBroadcastHandle.create();
    expect(owner.subscriberCount(), 0);
    owner.broadcast().watch(); // construct but never .listen
    await Future.delayed(const Duration(milliseconds: 50));
    expect(
      owner.subscriberCount(),
      0,
      reason: 'attach is deferred to first listen',
    );
  });

  test(
    'BehaviorBroadcast: fresh subscriber sees cached value on attach',
    () async {
      final owner = TestBehaviorBroadcastHandle.create();
      owner.add(value: 42);
      final received = <int>[];
      final sub = owner.broadcast().watch().listen(received.add);
      await Future.delayed(const Duration(milliseconds: 50));
      expect(received, [42], reason: 'cached value emitted on attach');
      await sub.cancel().timeout(const Duration(seconds: 1));
      expect(owner.subscriberCount(), 0);
    },
  );

  test('Ticker: live ticks from a spawned Rust thread', () async {
    final ticker = TestTickerHandle.create(intervalMs: 100);
    final received = <int>[];
    final sub = ticker.broadcast().watch().listen(received.add);

    // Let several ticks land. Each cycle is 100ms, so 450ms covers
    // ticks 0..3 with a small safety margin for thread startup.
    await Future.delayed(const Duration(milliseconds: 450));

    expect(
      received.length,
      greaterThanOrEqualTo(3),
      reason: 'should see multiple ticks within 450ms',
    );
    // Broadcast is non-replaying; we may have missed tick 0 if the Rust
    // thread had already started by the time we subscribed. Whatever the
    // first observed tick is, subsequent ticks must be strictly sequential.
    for (var i = 1; i < received.length; i++) {
      expect(
        received[i],
        received[0] + i,
        reason: 'tick #$i should follow ${received[i - 1]}',
      );
    }
    expect(ticker.subscriberCount(), 1);

    await sub.cancel().timeout(const Duration(seconds: 1));
    expect(ticker.subscriberCount(), 0);

    // After cancel, no more events arrive.
    final receivedAtCancel = received.length;
    await Future.delayed(const Duration(milliseconds: 250));
    expect(received.length, receivedAtCancel);
  });

  test('KeyedTicker: independent streams per key', () async {
    final keyed = TestKeyedTickerHandle.create();
    keyed.addTicker(key: 'fast', intervalMs: 50);
    keyed.addTicker(key: 'slow', intervalMs: 200);

    final fast = <int>[];
    final slow = <int>[];
    final fastSub = keyed.broadcast(key: 'fast').watch().listen(fast.add);
    final slowSub = keyed.broadcast(key: 'slow').watch().listen(slow.add);

    // ~500ms: fast should land ~10 ticks, slow ~2-3 ticks.
    await Future.delayed(const Duration(milliseconds: 500));

    expect(
      fast.length,
      greaterThanOrEqualTo(8),
      reason: 'fast key should tick many times in 500ms',
    );
    expect(
      slow.length,
      lessThan(fast.length),
      reason: 'slow ticker emits fewer events than fast',
    );
    // Each stream is independently monotonic from wherever it joined.
    for (var i = 1; i < fast.length; i++) {
      expect(fast[i], fast[0] + i);
    }
    for (var i = 1; i < slow.length; i++) {
      expect(slow[i], slow[0] + i);
    }

    // Per-key subscriber counts reflect only that key's subscribers.
    expect(keyed.subscriberCount(key: 'fast'), 1);
    expect(keyed.subscriberCount(key: 'slow'), 1);

    // Cancel one stream; the other keeps going.
    await fastSub.cancel().timeout(const Duration(seconds: 1));
    expect(keyed.subscriberCount(key: 'fast'), 0);
    expect(keyed.subscriberCount(key: 'slow'), 1);
    final slowAtFastCancel = slow.length;

    await Future.delayed(const Duration(milliseconds: 300));
    expect(
      slow.length,
      greaterThan(slowAtFastCancel),
      reason: 'slow ticker keeps emitting after fast cancel',
    );

    await slowSub.cancel().timeout(const Duration(seconds: 1));
    expect(keyed.subscriberCount(key: 'slow'), 0);
  });

  test(
    'BehaviorBroadcast: replay + subsequent adds delivered in order',
    () async {
      final owner = TestBehaviorBroadcastHandle.create();
      owner.add(value: 1);
      final received = <int>[];
      final sub = owner.broadcast().watch().listen(received.add);
      // attach is synchronous, so these adds happen strictly after registration.
      // This is an ordering/replay test, not a concurrency test; the lock
      // discipline in BehaviorBroadcast::register is what enforces the
      // no-tear-under-concurrent-add invariant.
      for (var i = 2; i < 20; i++) {
        owner.add(value: i);
      }
      await Future.delayed(const Duration(milliseconds: 100));
      expect(received, List.generate(19, (i) => i + 1));
      await sub.cancel();
    },
  );

  testWidgets(
    'StreamBuilder with inline .watch(): no Rust-sink leak across rebuilds',
    (tester) async {
      final owner = TestBroadcastHandle.create();
      expect(owner.subscriberCount(), 0);

      final hostKey = GlobalKey<_StreamBuilderHostState>();
      await tester.pumpWidget(
        MaterialApp(home: _StreamBuilderHost(owner, key: hostKey)),
      );
      await tester.pumpAndSettle();
      expect(owner.subscriberCount(), 1, reason: 'mount subscribes once');

      // Force several rebuilds; the inline `widget.bcast.watch()` evaluates
      // to a fresh Stream each build, so StreamBuilder cancels + resubscribes
      // every cycle. Count should net to 1, not accumulate.
      for (var i = 0; i < 5; i++) {
        hostKey.currentState!.forceRebuild();
        await tester.pumpAndSettle();
        expect(
          owner.subscriberCount(),
          1,
          reason: 'after rebuild #${i + 1}: net subscribers should be 1',
        );
      }

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
      expect(
        owner.subscriberCount(),
        0,
        reason: 'unmount cancels the subscription and detaches the sink',
      );
    },
  );
}
