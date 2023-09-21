import 'dart:math';
import 'dart:typed_data';
import 'package:usb_serial/usb_serial.dart';
import 'package:collection/collection.dart';
import 'dart:developer' as developer;

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
