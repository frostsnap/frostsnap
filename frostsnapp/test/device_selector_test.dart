import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_selector.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/signer_selection.dart';
import 'package:frostsnap/src/rust/lib.dart';

DeviceId deviceId(int seed) {
  final bytes = Uint8List(33);
  bytes[0] = seed;
  return DeviceId(field0: U8Array33(bytes));
}

Widget noBadge(DeviceId _) => const SizedBox.shrink();

SignerRow row(
  int seed, {
  String? name,
  required int shareIndex,
  bool selected = false,
  SignerRefusal? refusal,
}) => SignerRow(
  id: deviceId(seed),
  name: name,
  shareIndex: shareIndex,
  selected: selected,
  refusal: refusal,
);

void main() {
  setUpAll(() {
    caseAccentSource = (devices: const Stream.empty(), recorded: (_) => null);
  });

  Future<List<(DeviceId, bool)>> pumpRows(
    WidgetTester tester,
    List<SignerRow> rows,
  ) async {
    final changes = <(DeviceId, bool)>[];
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SignerRows(
            rows: rows,
            onChanged: (id, checked) => changes.add((id, checked)),
            badge: noBadge,
          ),
        ),
      ),
    );
    return changes;
  }

  CheckboxListTile tile(WidgetTester tester, String name) => tester.widget(
    find.ancestor(of: find.text(name), matching: find.byType(CheckboxListTile)),
  );

  group('SignerRows', () {
    testWidgets('a device with no name is shown as unknown', (tester) async {
      await pumpRows(tester, [row(1, shareIndex: 1)]);

      expect(find.text('#1 <unknown>'), findsOneWidget);
    });

    testWidgets('a device short of nonces is greyed out and says why', (
      tester,
    ) async {
      final changes = await pumpRows(tester, [
        row(1, name: 'Usable', shareIndex: 1),
        row(
          2,
          name: 'Short',
          shareIndex: 2,
          refusal: const SignerRefusal.tooFewNonces(available: 3, needed: 5),
        ),
      ]);

      expect(tile(tester, '#2 Short').onChanged, isNull);
      expect(tile(tester, '#1 Usable').onChanged, isNotNull);
      expect(find.text('has nonces for 3, this needs 5'), findsOneWidget);

      await tester.tap(find.text('#2 Short'));
      expect(changes, isEmpty);
    });

    testWidgets(
      'a second device on a selected share is greyed out and says why',
      (tester) async {
        await pumpRows(tester, [
          row(1, name: 'First', shareIndex: 1, selected: true),
          row(
            2,
            name: 'Copy',
            shareIndex: 1,
            refusal: const SignerRefusal.shareAlreadySelected(shareIndex: 1),
          ),
        ]);

        expect(tile(tester, '#1 Copy').onChanged, isNull);
        expect(find.text('#1 is already selected'), findsOneWidget);
      },
    );

    testWidgets('toggling a device reports it', (tester) async {
      final changes = await pumpRows(tester, [
        row(1, name: 'Off', shareIndex: 1),
        row(2, name: 'On', shareIndex: 2, selected: true),
      ]);

      await tester.tap(find.text('#1 Off'));
      await tester.tap(find.text('#2 On'));
      expect(changes.map((c) => c.$1), [deviceId(1), deviceId(2)]);
      expect(changes.map((c) => c.$2), [true, false]);
    });

    testWidgets('at the threshold only selected devices can change', (
      tester,
    ) async {
      await pumpRows(tester, [
        row(1, name: 'Picked', shareIndex: 1, selected: true),
        row(
          2,
          name: 'Spare',
          shareIndex: 2,
          refusal: const SignerRefusal.thresholdReached(threshold: 1),
        ),
      ]);

      expect(tile(tester, '#1 Picked').onChanged, isNotNull);
      expect(tile(tester, '#2 Spare').onChanged, isNull);
      expect(find.textContaining('has nonces for'), findsNothing);
    });
  });
}
