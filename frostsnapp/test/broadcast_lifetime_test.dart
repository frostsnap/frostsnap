import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api/broadcast_test_fixtures.dart';

import 'support/rust_lib.dart';

const _settle = Duration(milliseconds: 50);
const _cancelDeadline = Duration(seconds: 1);

class _StreamBuilderHost extends StatefulWidget {
  const _StreamBuilderHost(this.owner, {super.key});
  final TestBroadcast owner;
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
        stream: widget.owner.broadcast().watch(),
        builder: (context, _) => const SizedBox.shrink(),
      ),
    ],
  );
}

void expectConsecutive(List<int> ticks) {
  for (var i = 1; i < ticks.length; i++) {
    expect(ticks[i], ticks[0] + i, reason: 'tick #$i after ${ticks[i - 1]}');
  }
}

void main() {
  setUpAll(initRustLibForTests);

  test('cancel completes promptly and detaches the Rust sink', () async {
    final owner = TestBroadcast.create();
    final received = <void>[];
    final sub = owner.broadcast().watch().listen(received.add);

    owner.fire();
    await Future.delayed(_settle);
    expect(received.length, 1);
    expect(owner.subscriberCount(), 1);

    await sub.cancel().timeout(_cancelDeadline);
    expect(owner.subscriberCount(), 0);

    owner.fire();
    await Future.delayed(_settle);
    expect(received.length, 1, reason: 'no events after cancel');
  });

  test('nothing registers in Rust before listen', () async {
    final owner = TestBroadcast.create();
    owner.broadcast().watch();
    await Future.delayed(_settle);
    expect(owner.subscriberCount(), 0);
  });

  test('BehaviorBroadcast: a new listener gets the current value', () async {
    final owner = TestBehaviorBroadcast.create(initial: 0);
    owner.add(value: 42);
    final received = <int>[];
    final sub = owner.broadcast().watch().listen(received.add);
    await Future.delayed(_settle);
    expect(received, [42]);
    await sub.cancel().timeout(_cancelDeadline);
    expect(owner.subscriberCount(), 0);
  });

  test(
    'BehaviorBroadcast: the current value, then later adds, in order',
    () async {
      final owner = TestBehaviorBroadcast.create(initial: 1);
      final received = <int>[];
      final sub = owner.broadcast().watch().listen(received.add);
      for (var i = 2; i < 20; i++) {
        owner.add(value: i);
      }
      await Future.delayed(_settle);
      expect(received, List.generate(19, (i) => i + 1));
      await sub.cancel();
    },
  );

  test('values added from another Rust thread arrive in order', () async {
    final ticker = TestTicker.create(intervalMs: 50);
    final received = <int>[];
    final sub = ticker.broadcast().watch().listen(received.add);

    await Future.delayed(const Duration(milliseconds: 300));
    expect(received.length, greaterThanOrEqualTo(3));
    expectConsecutive(received);
    expect(ticker.subscriberCount(), 1);

    await sub.cancel().timeout(_cancelDeadline);
    expect(ticker.subscriberCount(), 0);
    final atCancel = received.length;
    await Future.delayed(const Duration(milliseconds: 150));
    expect(received.length, atCancel);
    ticker.dispose();
  });

  test('streams of different broadcasts are independent', () async {
    final fast = TestTicker.create(intervalMs: 20);
    final slow = TestTicker.create(intervalMs: 100);
    final fastTicks = <int>[];
    final slowTicks = <int>[];
    final fastSub = fast.broadcast().watch().listen(fastTicks.add);
    final slowSub = slow.broadcast().watch().listen(slowTicks.add);

    await Future.delayed(const Duration(milliseconds: 400));
    expect(slowTicks.length, lessThan(fastTicks.length));
    expectConsecutive(fastTicks);
    expectConsecutive(slowTicks);

    await fastSub.cancel().timeout(_cancelDeadline);
    expect(fast.subscriberCount(), 0);
    expect(slow.subscriberCount(), 1);
    final slowAtFastCancel = slowTicks.length;
    await Future.delayed(const Duration(milliseconds: 250));
    expect(slowTicks.length, greaterThan(slowAtFastCancel));

    await slowSub.cancel().timeout(_cancelDeadline);
    expect(slow.subscriberCount(), 0);
    fast.dispose();
    slow.dispose();
  });

  testWidgets('a StreamBuilder rebuilt with a fresh watch() leaks no sinks', (
    tester,
  ) async {
    final owner = TestBroadcast.create();
    final hostKey = GlobalKey<_StreamBuilderHostState>();
    await tester.pumpWidget(
      MaterialApp(home: _StreamBuilderHost(owner, key: hostKey)),
    );
    await tester.pumpAndSettle();
    expect(owner.subscriberCount(), 1);

    for (var i = 0; i < 5; i++) {
      hostKey.currentState!.forceRebuild();
      await tester.pumpAndSettle();
      expect(owner.subscriberCount(), 1, reason: 'after rebuild #${i + 1}');
    }

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpAndSettle();
    expect(owner.subscriberCount(), 0);
  });
}
