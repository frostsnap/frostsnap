import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

import 'dart:collection';
import 'package:flutter/foundation.dart';
import 'package:frostsnapp/bridge_definitions.dart';

HashSet<DeviceId> deviceIdSet(List<DeviceId> devices) {
  final set = HashSet<DeviceId>(
    equals: (a, b) => deviceIdEquals(a, b),
    hashCode: (a) => Object.hashAll(a.field0),
  );

  set.addAll(devices);
  return set;
}

Map<KeyId, T> keyIdMap<T>() {
  final map = HashMap<KeyId, T>(
    equals: (a, b) => keyIdEquals(a, b),
    hashCode: (a) => Object.hashAll(a.field0),
  );

  return map;
}

Map<AccessStructureRef, T> accessStructureRefMap<T>() {
  final map = HashMap<AccessStructureRef, T>(
    equals: (a, b) => accessStructureRefEquals(a, b),
    hashCode:
        (a) => Object.hashAll([a.keyId.field0, a.accessStructureId.field0]),
  );

  return map;
}

bool deviceIdEquals(DeviceId lhs, DeviceId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());

bool keyIdEquals(KeyId lhs, KeyId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());

bool restorationIdEquals(RestorationId lhs, RestorationId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());

bool accessStructureIdEquals(AccessStructureId lhs, AccessStructureId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());

bool accessStructureRefEquals(AccessStructureRef lhs, AccessStructureRef rhs) {
  return keyIdEquals(lhs.keyId, rhs.keyId) &&
      accessStructureIdEquals(lhs.accessStructureId, rhs.accessStructureId);
}

String printHex(List<int>? bytes) {
  if (bytes == null) {
    return "null";
  } else {
    return bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join('');
  }
}

extension KeyIdExt on KeyId {
  String toHex() {
    return field0
        .toList()
        .map((b) => b.toRadixString(16).padLeft(2, '0'))
        .join('');
  }
}

extension DeviceIdExt on DeviceId {
  String toHex() {
    return field0
        .toList()
        .map((b) => b.toRadixString(16).padLeft(2, '0'))
        .join('');
  }
}

extension RestorationIdExt on RestorationId {
  String toHex() {
    return field0
        .toList()
        .map((b) => b.toRadixString(16).padLeft(2, '0'))
        .join('');
  }
}
