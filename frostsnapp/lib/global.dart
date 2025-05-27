import 'package:frostsnapp/src/rust/api.dart';
import 'package:frostsnapp/src/rust/api/coordinator.dart';
import 'package:frostsnapp/src/rust/api/device_list.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'serialport.dart';

late Coordinator coord;
late Api api;
late HostPortHandler globalHostPortHandler;

class GlobalStreams {
  /// Gets new updates from the device list
  static final Stream<DeviceListUpdate> deviceListUpdateStream =
      coord.subDeviceEvents().asBroadcastStream();

  /// Stream of device list changes. Only emits when there is a change.
  static final Stream<DeviceListChange> deviceListChangeStream =
      deviceListUpdateStream.asyncExpand(
        (update) => Stream.fromIterable(update.changes),
      );

  /// DeviceListUpdates as a behavior subject
  static final Stream<DeviceListUpdate> deviceListSubject =
      deviceListUpdateStream.toBehaviorSubject();

  static final Stream<KeyState> keyStateSubject =
      coord.subKeyEvents().toBehaviorSubject();
}
