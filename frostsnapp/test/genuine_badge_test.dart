import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

void main() {
  group('CaseColor presentation', () {
    test('no two case colours render alike', () {
      // Colour exists so a device on screen can be matched to the one in your
      // hand; two cases sharing a swatch would defeat that.
      final colors = CaseColor.values.map((c) => c.color).toSet();
      expect(colors, hasLength(CaseColor.values.length));

      final labels = CaseColor.values.map((c) => c.label).toSet();
      expect(labels, hasLength(CaseColor.values.length));
    });

    test('every case colour is visible against the dark surface', () {
      // Black renders as a dark grey rather than true black precisely so it can
      // be told apart from "no colour known" on this theme.
      const surface = Color(0xFF141218);
      for (final color in CaseColor.values) {
        expect(
          (color.color.computeLuminance() - surface.computeLuminance()).abs(),
          greaterThan(0.01),
          reason: '${color.label} is indistinguishable from the background',
        );
      }
    });
  });
}
