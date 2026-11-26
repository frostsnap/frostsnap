import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/firmware.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/wallet_device_list.dart';
import 'package:rxdart/rxdart.dart';

const unverified = GenuineStatus.unverified(firmwareSupportsCheck: true);

final genuine = GenuineStatus.genuine(
  certificate: GenuineCertificate(
    serial: '220825002',
    revision: '2.7-1625',
    caseColor: const CaseColor.blue(),
    provisionedAt: 1773801360,
    dsPublicKey: Uint8List(0),
  ),
);

ConnectedDevice device(
  int seed,
  String name, {
  GenuineStatus genuine = unverified,
}) {
  final id = Uint8List(33);
  id[0] = seed;
  return ConnectedDevice(
    name: name,
    firmware: DeviceFirmware(
      digest: Sha256Digest(field0: U8Array32(Uint8List(32))),
      version: const DeviceVersion.reported(
        version: VersionNumber(major: 0, minor: 5, patch: 0),
        environment: Environment.production,
      ),
    ),
    id: DeviceId(field0: U8Array33(id)),
    recoveryMode: const RecoveryMode.off(),
    genuine: genuine,
  );
}

DeviceListUpdate update(
  List<ConnectedDevice> devices, [
  List<DeviceListChange> changes = const [],
]) => DeviceListUpdate(
  changes: changes,
  state: DeviceListState(devices: devices, unlisted: const [], stateId: 0),
);

DeviceListChange change(
  DeviceListChangeKind kind,
  int index,
  ConnectedDevice d,
) => DeviceListChange(kind: kind, index: index, device: d);

Widget harness(Stream<DeviceListUpdate> updates) => MaterialApp(
  home: CustomScrollView(
    slivers: [
      SliverDeviceList(
        updates: updates,
        deviceBuilder: (context, d) => SizedBox(
          height: 50,
          child: Text(
            switch (d.genuine) {
              GenuineStatus_Genuine() => '${d.name} genuine',
              GenuineStatus_Unverified() => d.name!,
            },
          ),
        ),
        noDeviceWidget: const Text('none'),
      ),
    ],
  ),
);

double sizeFactorOf(WidgetTester tester, String name) => tester
    .widget<SizeTransition>(
      find.ancestor(of: find.text(name), matching: find.byType(SizeTransition)),
    )
    .sizeFactor
    .value;

void main() {
  final a = device(1, 'A');
  final b = device(2, 'B');

  testWidgets('the exit animation renders the removed device', (tester) async {
    final updates = BehaviorSubject.seeded(update([a, b]));
    await tester.pumpWidget(harness(updates));
    await tester.pump();

    updates.add(update([b], [change(DeviceListChangeKind.removed, 0, a)]));
    await tester.pump();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));

    expect(find.text('A'), findsOneWidget);
    expect(find.text('B'), findsOneWidget);

    await tester.pumpAndSettle();
    expect(find.text('A'), findsNothing);
    expect(find.text('B'), findsOneWidget);
  });

  testWidgets('a replayed update does not re-apply its changes', (
    tester,
  ) async {
    final updates = BehaviorSubject.seeded(
      update([a], [change(DeviceListChangeKind.added, 0, a)]),
    );
    await tester.pumpWidget(harness(updates));
    await tester.pumpAndSettle();

    expect(find.byType(SizeTransition), findsOneWidget);
    expect(find.text('A'), findsOneWidget);
  });

  testWidgets('the first insert and last removal animate', (tester) async {
    final updates = BehaviorSubject.seeded(update([]));
    await tester.pumpWidget(harness(updates));
    await tester.pump();
    expect(find.text('none'), findsOneWidget);

    updates.add(update([a], [change(DeviceListChangeKind.added, 0, a)]));
    await tester.pump();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(sizeFactorOf(tester, 'A'), inExclusiveRange(0, 1));

    await tester.pumpAndSettle();
    expect(find.text('none'), findsNothing);

    updates.add(update([], [change(DeviceListChangeKind.removed, 0, a)]));
    await tester.pump();
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    expect(sizeFactorOf(tester, 'A'), inExclusiveRange(0, 1));

    await tester.pumpAndSettle();
    expect(find.text('A'), findsNothing);
    expect(find.text('none'), findsOneWidget);
  });

  testWidgets('a rename updates the row in place', (tester) async {
    final updates = BehaviorSubject.seeded(update([a, b]));
    await tester.pumpWidget(harness(updates));
    await tester.pump();

    final renamed = device(1, 'A2');
    updates.add(
      update([renamed, b], [change(DeviceListChangeKind.named, 0, renamed)]),
    );
    await tester.pump();
    await tester.pump();

    expect(find.text('A'), findsNothing);
    expect(find.text('A2'), findsOneWidget);
    expect(find.text('B'), findsOneWidget);
  });

  testWidgets('a genuine status change updates the row in place', (
    tester,
  ) async {
    final updates = BehaviorSubject.seeded(update([a, b]));
    await tester.pumpWidget(harness(updates));
    await tester.pump();

    final verified = device(2, 'B', genuine: genuine);
    updates.add(
      update([a, verified], [
        change(DeviceListChangeKind.refreshed, 1, verified),
      ]),
    );
    await tester.pump();
    await tester.pump();

    expect(find.text('A'), findsOneWidget);
    expect(find.text('B'), findsNothing);
    expect(find.text('B genuine'), findsOneWidget);
    expect(sizeFactorOf(tester, 'B genuine'), 1);
  });
}
