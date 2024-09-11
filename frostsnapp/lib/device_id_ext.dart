import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

import 'dart:collection';
import 'package:flutter/foundation.dart';
import 'package:frostsnapp/bridge_definitions.dart';

HashSet<DeviceId> deviceIdSet(List<DeviceId> devices) {
  final set = HashSet<DeviceId>(
      equals: (a, b) => deviceIdEquals(a, b),
      hashCode: (a) => Object.hashAll(a.field0));

  set.addAll(devices);
  return set;
}

bool deviceIdEquals(DeviceId lhs, DeviceId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());
