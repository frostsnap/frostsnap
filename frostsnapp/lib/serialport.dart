import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/src/rust/api/port.dart';

class HostPortHandler {
  // Let's us call functionality like list ports and open port
  static const MethodChannel _mainChannel = MethodChannel(
    'com.frostsnap.cdc_acm_plugin/main',
  );
  // For attached/detached events. We need to know when things are detached so
  // we can remove them from the list. We don't really care about attached
  // events because they only matter once we get permission for it.
  static const EventChannel _systemUsbEventsChannel = EventChannel(
    'com.frostsnap.cdc_acm_plugin/system_usb_events',
  );
  // Gets the notification for when the user approves our app as the one that
  // should open the device OR when a device that has already been approved is
  // plugged in.
  static const MethodChannel _usbPermissionsChannel = MethodChannel(
    'com.frostsnap/usb_permissions_channel',
  );

  final FfiSerial ffiserial;
  StreamSubscription<PortOpen>? _rustPortEventsSubscription;
  StreamSubscription<UsbSystemEvent>? _systemUsbEventsSubscription;
  final Map<String, PortDesc> _approvedSystemDevices = {};
  static Stream<UsbSystemEvent>? _systemEventsStream;

  HostPortHandler(this.ffiserial) {
    _usbPermissionsChannel.setMethodCallHandler(_handleNativeDeviceApproved);
    _systemUsbEventsSubscription = getSystemUsbEventsStream().listen(
      _handleSystemUsbEvent,
    );

    _rustPortEventsSubscription = ffiserial.subOpenRequests().listen((
      request,
    ) async {
      final portIdToOpen = request.id;
      debugPrint("HostPortHandler: rust request we open $portIdToOpen");
      try {
        final fd = await openDeviceAndGetFd(request.id);
        debugPrint(
          "HostPortHandler: Native device opened. FD: $fd for $portIdToOpen",
        );
        request.satisfy(fd: fd);
      } catch (e, s) {
        debugPrint(
          "HostPortHandler: Error opening port for '$portIdToOpen': $e\n$s",
        );
        if (e.toString().contains("DEVICE_NOT_FOUND")) {
          // This means the device is gone but we didn't receive the detached event yet.
          // We will soon though.
          _approvedSystemDevices.remove(request.id);
          _updateFfiAvailablePorts();
        }
        request.satisfy(fd: -1, err: e.toString());
      }
    });

    debugPrint("HostPortHandler: listening for open requests");
  }

  void _handleSystemUsbEvent(UsbSystemEvent systemEvent) {
    final id = systemEvent.device.id;
    debugPrint(
      "HostPortHandler: System USB Event: ${systemEvent.type} for $id",
    );
    if (systemEvent.type == UsbSystemEventType.detached) {
      _approvedSystemDevices.remove(id);
      _updateFfiAvailablePorts();
    } else if (systemEvent.type == UsbSystemEventType.attached) {
      // device attached but not yet approved so there's nothing much we need to do.
    }
  }

  Future<dynamic> _handleNativeDeviceApproved(MethodCall call) async {
    if (call.method == 'onUsbDeviceAttached') {
      final details = call.arguments as Map<dynamic, dynamic>?;
      if (details != null) {
        final vid = details['vid'] as int;
        final pid = details['pid'] as int;
        final id = details['id'] as String;
        _approvedSystemDevices[id] = PortDesc(pid: pid, vid: vid, id: id);
        _updateFfiAvailablePorts();
        return "Dart: Processed system-approved device $id";
      }
    }
    throw MissingPluginException();
  }

  void _updateFfiAvailablePorts() {
    ffiserial.setAvailablePorts(ports: _approvedSystemDevices.values.toList());
  }

  void dispose() {
    _rustPortEventsSubscription?.cancel();
    _systemUsbEventsSubscription?.cancel();
    _usbPermissionsChannel.setMethodCallHandler(null);
    _approvedSystemDevices.clear();
    _updateFfiAvailablePorts();
  }

  static Stream<UsbSystemEvent> getSystemUsbEventsStream() {
    _systemEventsStream ??= _systemUsbEventsChannel
        .receiveBroadcastStream()
        .map(
          (eventMap) =>
              UsbSystemEvent.fromMap(eventMap as Map<dynamic, dynamic>),
        );
    return _systemEventsStream!;
  }

  static Future<List<PortDesc>> listDevices() async {
    try {
      final devices = await _mainChannel.invokeMethod<List<dynamic>>(
        'listDevices',
      );
      return devices
              ?.map(
                (map) => PortDesc(
                  id: map["id"] as String,
                  vid: map["vid"] as int,
                  pid: map["pid"] as int,
                ),
              )
              .toList() ??
          [];
    } catch (e) {
      debugPrint("HostPortHandler.listDevices Error: $e");
      return [];
    }
  }

  static Future<int> openDeviceAndGetFd(String id) async {
    final result = await _mainChannel.invokeMethod<Map<dynamic, dynamic>>(
      'openDeviceAndGetFd',
      {'id': id},
    );
    return result!['fd'] as int;
  }

  static Future<void> closeDevice(String portId) async {
    if (portId.isEmpty) {
      debugPrint("HostPortHandler.closeDevice: portId is empty.");
      return;
    }
    try {
      await _mainChannel.invokeMethod('closeDevice', {'portId': portId});
      debugPrint("HostPortHandler.closeDevice called for $portId");
    } catch (e) {
      debugPrint("HostPortHandler.closeDevice Error for $portId: $e");
    }
  }
}

enum UsbSystemEventType { attached, detached }

class UsbSystemEvent {
  final UsbSystemEventType type;
  final PortDesc device;

  UsbSystemEvent({required this.type, required this.device});

  factory UsbSystemEvent.fromMap(Map<dynamic, dynamic> map) {
    return UsbSystemEvent(
      type: (map['event'] as String) == 'attached'
          ? UsbSystemEventType.attached
          : UsbSystemEventType.detached,
      device: PortDesc(
        id: map["device"]["id"] as String,
        vid: map["device"]["vid"] as int,
        pid: map["device"]["pid"] as int,
      ),
    );
  }
}
