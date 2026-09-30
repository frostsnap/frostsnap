import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_selector.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';

DeviceId deviceId(int seed) {
  final bytes = Uint8List(33);
  bytes[0] = seed;
  return DeviceId(field0: U8Array33(bytes));
}

void main() {
  final usable = deviceId(1);
  final exhausted = deviceId(2);
  final devices = [
    DeviceItem(id: usable, name: 'Usable', shareIndex: 1),
    DeviceItem(
      id: exhausted,
      name: 'Exhausted',
      shareIndex: 2,
      hasNonces: false,
    ),
  ];

  Future<List<(DeviceId, bool)>> pumpList(
    WidgetTester tester, {
    Set<DeviceId>? selected,
    bool canSelectMore = true,
  }) async {
    final changes = <(DeviceId, bool)>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: DeviceSelectorList(
            devices: devices,
            selected: selected ?? deviceIdSet([]),
            canSelectMore: canSelectMore,
            onChanged: (id, checked) => changes.add((id, checked)),
          ),
        ),
      ),
    );
    return changes;
  }

  CheckboxListTile tile(WidgetTester tester, String name) => tester.widget(
    find.ancestor(of: find.text(name), matching: find.byType(CheckboxListTile)),
  );

  group('DeviceSelectorList', () {
    testWidgets('a device without a share index shows just its name', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: DeviceSelectorList(
              devices: [DeviceItem(id: usable, name: 'Usable')],
              selected: deviceIdSet([]),
              onChanged: (_, _) {},
            ),
          ),
        ),
      );

      expect(find.text('Usable'), findsOneWidget);
    });

    testWidgets('a device with no nonces is disabled and says why', (
      tester,
    ) async {
      final changes = await pumpList(tester);

      expect(tile(tester, '#2 Exhausted').onChanged, isNull);
      expect(tile(tester, '#1 Usable').onChanged, isNotNull);
      expect(
        find.text('no nonces remaining or too many signing sessions'),
        findsOneWidget,
      );

      await tester.tap(find.text('#2 Exhausted'));
      expect(changes, isEmpty);
    });

    testWidgets('toggling a device reports it', (tester) async {
      final changes = await pumpList(tester);

      await tester.tap(find.text('#1 Usable'));
      expect(changes.length, 1);
      expect(deviceIdEquals(changes.single.$1, usable), isTrue);
      expect(changes.single.$2, isTrue);
    });

    testWidgets('once enough are selected only selected devices can change', (
      tester,
    ) async {
      await pumpList(
        tester,
        selected: deviceIdSet([exhausted]),
        canSelectMore: false,
      );

      expect(tile(tester, '#1 Usable').onChanged, isNull);
    });
  });
}
