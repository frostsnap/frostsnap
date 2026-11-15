import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/wallet_create.dart';

void selectDevices(WalletCreateForm form, int count) {
  form.selectedDevices.clear();
  for (var i = 0; i < count; i++) {
    final bytes = Uint8List(33)..[0] = i + 1;
    form.selectedDevices.add(DeviceId(field0: U8Array33(bytes)));
  }
}

void main() {
  // Regression guard for #324: a 7-of-9 choice survived a trip back to device
  // selection, and rendering it over 3 devices tripped the Slider assertion.
  for (final count in [3, 1]) {
    test('reducing 9 devices to $count resets an invalid threshold', () {
      final form = WalletCreateForm();
      selectDevices(form, 9);
      form.threshold = 7;

      selectDevices(form, count);
      form.ensureValidThreshold();

      expect(form.threshold, recommendedThresholdFor(count));
    });
  }
}
