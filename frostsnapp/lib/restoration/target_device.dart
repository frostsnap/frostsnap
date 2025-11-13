import 'dart:async';

import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

class TargetDevice {
  final DeviceId id;
  final List<StreamSubscription<DeviceListUpdate>> _subscriptions = [];

  TargetDevice(this.id);

  ConnectedDevice? get device => coord.getConnectedDevice(id: id);
  String? get name => device?.name;
  bool needsFirmwareUpgrade() => device?.needsFirmwareUpgrade() ?? false;

  Future<void> onDisconnected() {
    final completer = Completer<void>();
    final subscription = GlobalStreams.deviceListSubject.listen((update) {
      final stillConnected = update.state.devices.any(
        (d) => deviceIdEquals(d.id, id),
      );
      if (!stillConnected && !completer.isCompleted) {
        completer.complete();
      }
    });
    _subscriptions.add(subscription);
    return completer.future;
  }

  Future<void> waitForReconnection() {
    final completer = Completer<void>();
    final subscription = GlobalStreams.deviceListSubject.listen((update) {
      final isReconnected = update.state.devices.any(
        (d) => deviceIdEquals(d.id, id),
      );
      if (isReconnected && !completer.isCompleted) {
        completer.complete();
      }
    });
    _subscriptions.add(subscription);
    return completer.future;
  }

  void dispose() {
    for (final subscription in _subscriptions) {
      subscription.cancel();
    }
    _subscriptions.clear();
  }
}
