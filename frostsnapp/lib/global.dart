import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

late Coordinator coord;

Stream<DeviceListUpdate> deviceListUpdateStream =
    api.subDeviceEvents().asBroadcastStream();
Stream<DeviceListState> deviceListStateStream =
    deviceListUpdateStream.map((update) => update.state).asBroadcastStream();
Stream<DeviceListChange> deviceListChangeStream = deviceListUpdateStream
    .asyncExpand((update) => Stream.fromIterable(update.changes))
    .asBroadcastStream();
