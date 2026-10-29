import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/threshold_selector.dart';
import 'package:frostsnap/wallet_create.dart';

void selectDevices(WalletCreateForm form, int count) {
  form.selectedDevices.clear();
  for (var i = 0; i < count; i++) {
    final bytes = Uint8List(33)..[0] = i + 1;
    form.selectedDevices.add(DeviceId(field0: U8Array33(bytes)));
  }
}

void main() {
  test('initial threshold uses the recommended majority', () {
    final form = WalletCreateForm();
    selectDevices(form, 3);

    form.ensureValidThreshold();

    expect(form.threshold, 2);
  });

  for (final count in [3, 1]) {
    testWidgets('reducing 9 devices to $count resets an invalid threshold', (
      tester,
    ) async {
      final form = WalletCreateForm();
      selectDevices(form, 9);
      form.threshold = 7;

      selectDevices(form, count);
      form.ensureValidThreshold();

      expect(form.threshold, recommendedThresholdFor(count));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ThresholdSelector(
              threshold: form.threshold!,
              totalDevices: count,
              recommendedThreshold: recommendedThresholdFor(count),
              onChanged: (_) {},
            ),
          ),
        ),
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  test('changing device count preserves a valid user choice', () {
    final form = WalletCreateForm();
    selectDevices(form, 9);
    form.threshold = 1;

    selectDevices(form, 3);
    form.ensureValidThreshold();
    expect(form.threshold, 1);

    selectDevices(form, 9);
    form.ensureValidThreshold();
    expect(form.threshold, 1);
  });
}
