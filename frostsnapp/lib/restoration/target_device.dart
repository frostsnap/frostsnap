import 'dart:async';

import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

class TargetDevice {
  final ConnectedDevice device;
  final Completer<void> _disconnectionCompleter = Completer<void>();
  StreamSubscription<DeviceListUpdate>? _subscription;

  TargetDevice(this.device) {
    _subscription = GlobalStreams.deviceListSubject.listen((update) {
      final stillConnected = update.state.devices.any(
        (d) => deviceIdEquals(d.id, device.id),
      );
      if (!stillConnected) {
        if (!_disconnectionCompleter.isCompleted) {
          _disconnectionCompleter.complete();
        }
        dispose();
      }
    });
  }

  DeviceId get id => device.id;
  String? get name => device.name;
  bool needsFirmwareUpgrade() => device.needsFirmwareUpgrade();

  Future<void> get onDisconnected => _disconnectionCompleter.future;

  void dispose() {
    if (!_disconnectionCompleter.isCompleted) {
      _disconnectionCompleter.complete();
    }
    _subscription?.cancel();
    _subscription = null;
  }
}
