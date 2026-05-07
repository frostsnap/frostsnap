import 'package:frostsnap/src/rust/api.dart';

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
