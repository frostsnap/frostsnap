import 'package:frostsnapp/stream_ext.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'serialport.dart';

late Coordinator coord;
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
