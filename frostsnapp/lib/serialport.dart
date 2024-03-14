import 'dart:async';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:frostsnapp/global.dart';
import 'package:usb_serial/usb_serial.dart';
import 'package:collection/collection.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class HostPortHandler {
  Map<String, SerialPort> openPorts = {};
  final FfiSerial? ffiserial;
  StreamSubscription<PortEvent>? subscription;

  HostPortHandler(this.ffiserial) {
    if (ffiserial == null) {
      return;
    }
    UsbSerial.usbEventStream?.listen((UsbEvent msg) {
      if (msg.event == UsbEvent.ACTION_USB_DETACHED) {
        openPorts.remove(msg.device?.deviceName);
      }
      debugPrint("Scanning devices because of new USB event");
      scanDevices();
    });
    subscription = api.subPortEvents().listen((event) async {
      switch (event) {
        case PortEvent_Open(:final request):
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
        case PortEvent_Read(:final request):
          {
            try {
              var port = _getPort(request.id);
              var newBytes = port.read(request.len);
              request.satisfy(bytes: newBytes);
            } catch (e) {
              request.satisfy(bytes: Uint8List(0), err: e.toString());
            }
          }
        case PortEvent_Write(:final request):
          {
            try {
              var port = _getPort(request.id);
              port.write(request.bytes);
              request.satisfy();
            } catch (e) {
              request.satisfy(err: e.toString());
            }
          }
        case PortEvent_BytesToRead(:final request):
          {
            var port = openPorts[request.id];
            if (port == null) {
              debugPrint("port for ${request.id} no longer connected");
            }
            request.satisfy(bytesToRead: port?.buffer.length ?? 0);
          }
      }
    });

    subscription!.onError((error) {
      debugPrint("port event stream error: $error");
    });

    subscription!.onDone(() {
      debugPrint("port event stream finished (but this should never happen!)");
    });
    debugPrint("Android serial port handler started");
  }

  void scanDevices() async {
    if (ffiserial != null) {
      List<UsbDevice> devices = await UsbSerial.listDevices();
      final List<PortDesc> portDescriptions = devices
          .where((device) => device.vid != null && device.pid != null)
          .map((device) => PortDesc(
              id: device.deviceName, pid: device.pid!, vid: device.vid!))
          .toList();
      await ffiserial!.setAvailablePorts(ports: portDescriptions);
    }
  }

  SerialPort _getPort(String id) {
    var port = openPorts[id];
    if (port == null) {
      throw "port $id has been disconnected";
    }
    return port;
  }
}

class SerialPort {
  UsbPort? port = null;
  Uint8List buffer = Uint8List(0);

  static Future<SerialPort> open(String id, int baudRate) async {
    final deviceList = await UsbSerial.listDevices();
    final serialport = SerialPort();
    final device =
        deviceList.firstWhereOrNull((device) => device.deviceName == id);
    if (device == null) {
      throw "Device $id is not connected";
    } else {
      var port = await device.create();
      var opened = await port!.open();
      if (!opened) {
        throw "Couldn't open device $id";
      }

      // port.setPortParameters(baudRate, UsbPort.DATABITS_8, UsbPort.STOPBITS_1,
      //     UsbPort.PARITY_NONE);
      serialport.port = port;
      final inputStream = serialport.port!.inputStream as Stream<Uint8List>;
      inputStream.forEach((Uint8List bytes) {
        serialport.buffer = Uint8List.fromList(serialport.buffer + bytes);
      });

      return serialport;
    }
  }

  Uint8List read(int len) {
    len = min(len, buffer.length);
    var res = buffer.sublist(0, len);
    buffer = buffer.sublist(len);
    return res;
  }

  void write(Uint8List bytes) {
    port!.write(bytes);
  }
}
