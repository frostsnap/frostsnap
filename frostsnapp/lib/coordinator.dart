import 'dart:typed_data';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'package:usb_serial/usb_serial.dart';
import 'ffi.dart';
import 'serialport.dart';
import 'dart:io';

Coordinator global_coordinator = Coordinator();

class Coordinator {
  late Future<FfiCoordinator> ffi;
  Map<String, SerialPort> openPorts = {};

  Coordinator() {
    this.ffi = api.newFfiCoordinator(hostHandlesSerial: Platform.isAndroid);
    UsbSerial.usbEventStream?.listen((UsbEvent msg) {
      if (msg.event == UsbEvent.ACTION_USB_DETACHED) {
        openPorts.remove(msg.device?.deviceName);
      }
      scanDevices();
    });
    api.initEvents().forEach((event) async {
      switch (event) {
        case CoordinatorEvent_PortOpen(:final request):
          {
            try {
              var port = openPorts[request.id];
              port ??= await SerialPort.open(request.id, request.baudRate);
              openPorts[request.id] = port;
              request.satisfy();
            } catch (e) {
              request.satisfy(err: e.toString());
            }
          }
        case CoordinatorEvent_PortRead(:final request):
          {
            try {
              var port = _getPort(request.id);
              var newBytes = port.read(request.len);
              request.satisfy(bytes: newBytes);
            } catch (e) {
              request.satisfy(bytes: Uint8List(0), err: e.toString());
            }
          }
        case CoordinatorEvent_PortWrite(:final request):
          {
            try {
              var port = _getPort(request.id);
              port.write(request.bytes);
              request.satisfy();
            } catch (e) {
              request.satisfy(err: e.toString());
            }
          }
        case CoordinatorEvent_PortBytesToRead(:final request):
          {
            var port = openPorts[request.id];
            request.satisfy(bytesToRead: port?.buffer.length ?? 0);
          }
      }
    });
  }

  void scanDevices() async {
    var ctx = await ffi;
    List<UsbDevice> devices = await UsbSerial.listDevices();
    final List<PortDesc> portDescriptions = devices
        .where((device) => device.vid != null && device.pid != null)
        .map((device) =>
            PortDesc(id: device.deviceName, pid: device.pid!, vid: device.vid!))
        .toList();
    await api.announceAvailablePorts(coordinator: ctx, ports: portDescriptions);
  }

  void setDeviceLabel(String deviceId, String label) async {
    await api.setDeviceLabel(
        coordinator: await ffi, deviceId: deviceId, label: label);
  }

  Stream<List<DeviceChange>> subDeviceEvents() {
    return api.subDeviceEvents();
  }

  SerialPort _getPort(String id) {
    var port = openPorts[id];
    if (port == null) {
      throw "port $id has been disconnected";
    }
    return port;
  }
}
