import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';

class TargetDevice {
  final DeviceId id;

  TargetDevice(this.id);

  ConnectedDevice? get device => coord.getConnectedDevice(id: id);
  String? get name => device?.name;
  bool needsFirmwareUpgrade() => device?.needsFirmwareUpgrade() ?? false;

  /// Whether the device is plugged in, starting with its current state.
  Stream<bool> get connected => GlobalStreams.deviceListSubject
      .map((update) => update.state.devices.any((d) => d.id == id))
      .distinct();
}
