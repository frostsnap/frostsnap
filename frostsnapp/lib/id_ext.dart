import 'package:frostsnap/iterable_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';

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

extension SortByKeyShareIndex<T> on Iterable<T> {
  /// Orders a wallet's devices by key share index. A device with none, because its share was
  /// removed while a signing session still lists it, goes last rather than disappearing.
  List<T> sortByKeyShareIndex(
    AccessStructure? accessStructure,
    DeviceId Function(T) deviceId,
  ) => sortedByCachedKey<num>(
    (item) =>
        accessStructure?.getDeviceShortShareIndex(deviceId: deviceId(item)) ??
        double.infinity,
  );
}
