import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/device_colors.dart';
import 'package:frostsnap/genuine_badge.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/lib.dart';

DeviceId deviceId(int seed) {
  final bytes = Uint8List(33);
  bytes[0] = seed;
  return DeviceId(field0: U8Array33(bytes));
}

void main() {
  // The trap behind a bug where genuine status silently read "unverified" on every
  // screen that looked a device up by id: the generated `DeviceId ==` forwards to
  // `field0 ==`, and `field0` is a plain Dart list, which compares by reference.
  // Two ids decoded from separate FFI calls are never the same object, so `==` is
  // always false and `firstWhere` always throws.
  group('DeviceId equality', () {
    test('== does not compare by value, so never use it to match a device', () {
      expect(deviceId(1) == deviceId(1), isFalse);
    });

    test('deviceIdEquals compares by value', () {
      expect(deviceIdEquals(deviceId(1), deviceId(1)), isTrue);
      expect(deviceIdEquals(deviceId(1), deviceId(2)), isFalse);
    });
  });

  group('GenuineStatus presentation', () {
    const scheme = ColorScheme.dark();

    test('each state maps to the look the design note gives it', () {
      expect(const GenuineStatus.genuine().look, GenuineLook.genuine);
      for (final supported in [true, false]) {
        final expected = supported
            ? GenuineLook.unverified
            : GenuineLook.updateToVerify;
        expect(
          GenuineStatus.unattested(firmwareSupportsCheck: supported).look,
          expected,
        );
        expect(
          GenuineStatus.attested(firmwareSupportsCheck: supported).look,
          expected,
        );
      }
    });

    test('every look is distinguishable from every other', () {
      final labels = GenuineLook.values.map((l) => l.label).toSet();
      expect(labels, hasLength(GenuineLook.values.length));
      final icons = GenuineLook.values.map((l) => l.icon).toSet();
      expect(icons, hasLength(GenuineLook.values.length));
    });

    test('only genuine is coloured', () {
      expect(
        GenuineLook.genuine.color(scheme),
        isNot(GenuineLook.unverified.color(scheme)),
      );
      expect(
        GenuineLook.updateToVerify.color(scheme),
        GenuineLook.unverified.color(scheme),
      );
    });

    test('every look explains itself', () {
      for (final look in GenuineLook.values) {
        final (title, body) = look.explanation;
        expect(title, isNotEmpty, reason: '$look has no title');
        expect(body, isNotEmpty, reason: '$look has no explanation');
      }
    });
  });

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
