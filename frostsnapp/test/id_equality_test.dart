import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/lib.dart';

Uint8List bytes(int length, int seed) =>
    Uint8List.fromList(List.generate(length, (i) => (seed + i) & 0xff));

DeviceId deviceId(int seed) => DeviceId(field0: U8Array33(bytes(33, seed)));
KeyId keyId(int seed) => KeyId(field0: U8Array32(bytes(32, seed)));
AccessStructureId accessStructureId(int seed) =>
    AccessStructureId(field0: U8Array32(bytes(32, seed)));

void main() {
  test('separately built equal ids are == with equal hashCode', () {
    expect(deviceId(1), deviceId(1));
    expect(deviceId(1).hashCode, deviceId(1).hashCode);
    expect(deviceId(1), isNot(deviceId(2)));
    expect(keyId(1), keyId(1));
    expect(keyId(1).hashCode, keyId(1).hashCode);
  });

  test('ids with the same bytes but different types are not equal', () {
    expect(keyId(1) == accessStructureId(1), isFalse);
  });

  test('ids work as Set elements and Map keys', () {
    final set = {deviceId(1), deviceId(2)};
    expect(set.contains(deviceId(1)), isTrue);
    expect(set.contains(deviceId(3)), isFalse);
    set.add(deviceId(1));
    expect(set.length, 2);

    final map = {keyId(1): 'a'};
    expect(map[keyId(1)], 'a');
    map[keyId(1)] = 'b';
    expect(map.length, 1);
  });

  test('composites containing ids compare by content', () {
    AccessStructureRef ref() => AccessStructureRef(
      keyId: keyId(1),
      accessStructureId: accessStructureId(2),
    );
    expect(ref(), ref());
    expect({ref()}.contains(ref()), isTrue);
  });
}
