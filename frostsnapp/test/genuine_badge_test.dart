import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/certificate_details.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

void main() {
  group('Certificate details', () {
    test('the provisioning time reads as a UTC date and time', () {
      expect(formatProvisionedAt(1773801360), '18 Mar 2026, 02:36 UTC');
      expect(formatProvisionedAt(0), '1 Jan 1970, 00:00 UTC');
    });
  });

  group('CaseColor presentation', () {
    const named = [
      CaseColor.black(),
      CaseColor.orange(),
      CaseColor.silver(),
      CaseColor.blue(),
      CaseColor.red(),
    ];
    final swatches = named.map((c) => c.swatch).nonNulls.toList();

    test('no two case colours render alike', () {
      // Colour exists so a device on screen can be matched to the one in your
      // hand; two cases sharing a swatch would defeat that.
      final colors = swatches.map((s) => s.color).toSet();
      expect(colors, hasLength(named.length));

      final labels = swatches.map((s) => s.label).toSet();
      expect(labels, hasLength(named.length));
    });

    test('every case colour is visible against the dark surface', () {
      // Black renders as a dark grey rather than true black precisely so it can
      // be told apart from "no colour known" on this theme.
      const surface = Color(0xFF141218);
      for (final swatch in swatches) {
        expect(
          (swatch.color.computeLuminance() - surface.computeLuminance()).abs(),
          greaterThan(0.01),
          reason: '${swatch.label} is indistinguishable from the background',
        );
      }
    });
  });
}
