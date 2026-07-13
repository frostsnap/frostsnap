import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';
import 'package:frostsnap/wallet_create.dart';

// Regression guard for PR #505's duplicate-device-name check (the false positive fixed here).
// The wallet-create form retains a device's typed name after it disconnects, but only the
// currently connected devices are participants in the keygen. `duplicateNamedDeviceIdsAmong`
// must therefore count collisions over the live participants alone — otherwise a name freed by
// a disconnected device would falsely flag a remaining device that reuses it, jamming Continue.

DeviceId deviceId(int seed) {
  final bytes = Uint8List(33);
  bytes[0] = seed; // distinct first byte => distinct id
  return DeviceId(field0: U8Array33(bytes));
}

void main() {
  group('duplicateNamedDeviceIdsAmong', () {
    final a = deviceId(1);
    final b = deviceId(2);

    Map<DeviceId, String> names(Map<DeviceId, String> entries) {
      final map = deviceIdMap<String>();
      map.addAll(entries);
      return map;
    }

    test(
      'a name freed by a disconnected device can be reused (the #505 bug)',
      () {
        // A was named "Cold", then A left the device list; B (still connected)
        // reuses "Cold". A's retained entry must not flag B.
        final participants = deviceIdSet([b]); // only B is connected now
        final dups = duplicateNamedDeviceIdsAmong(
          participants,
          names({a: 'Cold', b: 'Cold'}),
        );
        expect(dups, isEmpty);
      },
    );

    test('two currently-connected devices sharing a name are both flagged', () {
      final participants = deviceIdSet([a, b]);
      final dups = duplicateNamedDeviceIdsAmong(
        participants,
        names({a: 'Cold', b: 'Cold'}),
      );
      expect(dups.length, 2);
      expect(dups.contains(a), isTrue);
      expect(dups.contains(b), isTrue);
    });

    test('the collision is case-insensitive and trims whitespace', () {
      final participants = deviceIdSet([a, b]);
      final dups = duplicateNamedDeviceIdsAmong(
        participants,
        names({a: 'Cold', b: '  cOLD '}),
      );
      expect(dups.length, 2);
    });

    test('distinct names among connected devices are not flagged', () {
      final participants = deviceIdSet([a, b]);
      final dups = duplicateNamedDeviceIdsAmong(
        participants,
        names({a: 'Cold', b: 'Hot'}),
      );
      expect(dups, isEmpty);
    });

    test('an empty or whitespace-only name never collides', () {
      final participants = deviceIdSet([a, b]);
      final dups = duplicateNamedDeviceIdsAmong(
        participants,
        names({a: '', b: '   '}),
      );
      expect(dups, isEmpty);
    });
  });
}
