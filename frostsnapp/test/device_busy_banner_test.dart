import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_busy_banner.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

DeviceListUpdate _update(List<String> busyPorts) => DeviceListUpdate(
  changes: const [],
  state: DeviceListState(
    devices: const [],
    unlisted: const [],
    busyPorts: busyPorts,
    stateId: 0,
  ),
);

/// Counts its own taps, to tell whether it was rebuilt from scratch.
class _Counter extends StatefulWidget {
  const _Counter();

  @override
  State<_Counter> createState() => _CounterState();
}

class _CounterState extends State<_Counter> {
  var taps = 0;

  @override
  Widget build(BuildContext context) => Scaffold(
    body: TextButton(
      onPressed: () => setState(() => taps++),
      child: Text('taps: $taps'),
    ),
  );
}

void main() {
  test('only reports changes to whether any port is busy', () async {
    final busy = anyDevicePortBusy(
      Stream.fromIterable([
        _update([]),
        _update(['/dev/ttyACM0']),
        _update(['/dev/ttyACM0', '/dev/ttyACM1']),
        _update([]),
      ]),
    );
    expect(await busy.toList(), [false, true, false]);
  });

  testWidgets('shows while a port is busy without resetting the app', (
    tester,
  ) async {
    final busy = StreamController<bool>();
    addTearDown(busy.close);

    await tester.pumpWidget(
      MaterialApp(
        home: const _Counter(),
        builder: (context, child) =>
            DeviceBusyBanner(busy: busy.stream, child: child!),
      ),
    );
    final banner = find.text(DeviceBusyBanner.message);
    expect(banner, findsNothing);

    await tester.tap(find.text('taps: 0'));
    await tester.pump();

    busy.add(true);
    await tester.pumpAndSettle();
    expect(banner, findsOneWidget);
    expect(find.text('taps: 1'), findsOneWidget);

    busy.add(false);
    await tester.pumpAndSettle();
    expect(banner, findsNothing);
    expect(find.text('taps: 1'), findsOneWidget);
  });
}
