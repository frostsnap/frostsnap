import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'serialport.dart';

late Coordinator coord;
late HostPortHandler globalHostPortHandler;

Stream<DeviceListUpdate> deviceListUpdateStream =
    api.subDeviceEvents().asBroadcastStream();
Stream<DeviceListState> deviceListStateStream =
    deviceListUpdateStream.map((update) => update.state).asBroadcastStream();
Stream<DeviceListChange> deviceListChangeStream = deviceListUpdateStream
    .asyncExpand((update) => Stream.fromIterable(update.changes))
    .asBroadcastStream();
