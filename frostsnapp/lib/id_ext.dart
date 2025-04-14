import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

import 'dart:collection';
import 'package:flutter/foundation.dart';
import 'package:frostsnapp/bridge_definitions.dart';

HashSet<DeviceId> deviceIdSet(Iterable<DeviceId> devices) {
  final set = HashSet<DeviceId>(
    equals: deviceIdEquals,
    hashCode: deviceIdHashCode,
  );
  set.addAll(devices);
  return set;
}

HashMap<DeviceId, T> deviceIdMap<T>() =>
    HashMap<DeviceId, T>(equals: deviceIdEquals, hashCode: deviceIdHashCode);

HashMap<KeyId, T> keyIdMap<T>() =>
    HashMap<KeyId, T>(equals: keyIdEquals, hashCode: keyIdHashCode);

bool deviceIdEquals(DeviceId lhs, DeviceId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());

bool keyIdEquals(KeyId lhs, KeyId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());

int deviceIdHashCode(DeviceId id) => Object.hashAll(id.field0);

int keyIdHashCode(KeyId id) => Object.hashAll(id.field0);
