import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/firmware.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/wallet_device_list.dart';

class _FakeCoordinator implements Coordinator {
  final updates = StreamController<DeviceListUpdate>();
  late DeviceListState current;

  @override
  DeviceListState deviceListState() => current;

  @override
  Stream<DeviceListUpdate> subDeviceEvents() => updates.stream;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

ConnectedDevice device(String name) {
  final id = Uint8List(33);
  id[0] = name.codeUnitAt(0);
  return ConnectedDevice(
    name: name,
    firmware: FirmwareVersion(
      digest: Sha256Digest(field0: U8Array32(Uint8List(32))),
    ),
    id: DeviceId(field0: U8Array33(id)),
    recoveryMode: const RecoveryMode.off(),
  );
}

DeviceListChange added(int stateId, int index, ConnectedDevice device) =>
    DeviceListChange(
      kind: DeviceListChangeKind.added,
      index: index,
      device: device,
      stateId: stateId,
    );

DeviceListChange removed(int stateId, int index, ConnectedDevice device) =>
    DeviceListChange(
      kind: DeviceListChangeKind.removed,
      index: index,
      device: device,
      stateId: stateId,
    );

DeviceListUpdate update(
  int stateId,
  List<ConnectedDevice> devices,
  List<DeviceListChange> changes,
) => DeviceListUpdate(
  changes: changes,
  state: DeviceListState(devices: devices, stateId: stateId),
);

void main() {
  final fake = _FakeCoordinator();
  coord = fake;
  final a = device('a'), b = device('b'), c = device('c'), d = device('d');

  // GlobalStreams is process-wide and binds to the zone that first touches it, so the cases share
  // one test body rather than each getting its own fake-async zone.
  testWidgets('a seeded list applies only the changes its seed lacks', (
    tester,
  ) async {
    final primed = GlobalStreams.deviceListSubject.listen((_) {});
    addTearDown(primed.cancel);

    Future<void> mount(DeviceListState seed) async {
      fake.current = seed;
      await tester.pumpWidget(
        MaterialApp(
          home: CustomScrollView(
            slivers: [
              SliverDeviceList(
                key: UniqueKey(),
                deviceBuilder: (context, device) => Text(device.name!),
              ),
            ],
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    Future<void> send(DeviceListUpdate update) async {
      fake.updates.add(update);
      await tester.pumpAndSettle();
    }

    void expectShown(List<String> names) {
      for (final name in names) {
        expect(find.text(name), findsOneWidget);
      }
      expect(find.byType(Text), findsNWidgets(names.length));
    }

    // Seeded after a removal: the replayed removal is already in the seed.
    await send(
      update(3, [a, b, c], [added(1, 0, a), added(2, 1, b), added(3, 2, c)]),
    );
    await send(update(4, [a, c], [removed(4, 1, b)]));
    await send(update(5, [a], [removed(5, 1, c)]));
    await mount(DeviceListState(devices: [a], stateId: 5));
    expectShown(['a']);
    await send(update(6, [a, d], [added(6, 1, d)]));
    expectShown(['a', 'd']);

    // Seeded mid-update: the seed already holds the update's first change, a removal.
    await mount(DeviceListState(devices: [a], stateId: 7));
    expectShown(['a']);
    await send(update(8, [a, b], [removed(7, 1, d), added(8, 1, b)]));
    expectShown(['a', 'b']);

    // Seeded ahead of the replay: both the replay and the in-flight update are already held.
    await mount(DeviceListState(devices: [b], stateId: 9));
    expectShown(['b']);
    await send(update(9, [b], [removed(9, 0, a)]));
    expectShown(['b']);
    await send(update(10, [], [removed(10, 0, b)]));
    expect(find.byType(Text), findsNothing);
  });
}
