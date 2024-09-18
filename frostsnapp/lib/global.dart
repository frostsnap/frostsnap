import 'package:frostsnapp/stream_ext.dart';

import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'serialport.dart';

late Wallet wallet;
late Coordinator coord;
late HostPortHandler globalHostPortHandler;
late BitcoinContext bitcoinContext;

/// Gets new updates from the device list
Stream<DeviceListUpdate> deviceListUpdateStream =
    api.subDeviceEvents().asBroadcastStream();

/// Stream of device list changes. Only emits when there is a change.
Stream<DeviceListChange> deviceListChangeStream = deviceListUpdateStream
    .asyncExpand((update) => Stream.fromIterable(update.changes));

/// DeviceListUpdates as a behavior subject
Stream<DeviceListUpdate> deviceListSubject =
    deviceListUpdateStream.toBehaviorSubject();
