import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

import 'dart:collection';
import 'package:flutter/foundation.dart';
import 'package:frostsnapp/bridge_definitions.dart';

Stream<DeviceListUpdate> deviceListUpdateStream =
    api.subDeviceEvents().asBroadcastStream();
Stream<DeviceListState> deviceListStateStream =
    deviceListUpdateStream.map((update) => update.state).asBroadcastStream();
Stream<DeviceListChange> deviceListChangeStream = deviceListUpdateStream
    .asyncExpand((update) => Stream.fromIterable(update.changes))
    .asBroadcastStream();

HashSet<DeviceId> deviceIdSet() {
  return HashSet<DeviceId>(
      equals: (a, b) => listEquals(a.field0, b.field0),
      hashCode: (a) => Object.hashAll(a.field0));
}

bool deviceIdEquals(DeviceId lhs, DeviceId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());
