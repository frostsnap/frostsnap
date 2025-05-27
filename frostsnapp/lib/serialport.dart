import 'dart:async';
import 'dart:math';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart'; // Required for MethodChannel
import 'package:frostsnapp/src/rust/api/port.dart'; // Assuming PortDesc is here
import 'package:usb_serial/usb_serial.dart'; // Assuming UsbDevice and UsbEvent are here
import 'package:collection/collection.dart';

class HostPortHandler {
  Map<String, SerialPort> openPorts = {};
  final FfiSerial? ffiserial;
  StreamSubscription<PortEvent>? _rustPortEventsSubscription;
  StreamSubscription<UsbEvent>? _usbSystemEventsSubscription;

  // MethodChannel for communication from MainActivity.kt
  // Make sure this string EXACTLY matches the one in MainActivity.kt
  static const _usbDeviceChannel = MethodChannel(
    'com.example.frostsnapp/usb_device_channel',
  );

  // Stores devices that have been explicitly approved via MainActivity's notification
  final Map<String, PortDesc> _approvedDevices = {};

  HostPortHandler(this.ffiserial) {
    if (ffiserial == null) {
      debugPrint(
        "HostPortHandler: FfiSerial is null, USB functionality will be limited.",
      );
      return;
    }

    // 1. Listen for native calls from MainActivity
    _usbDeviceChannel.setMethodCallHandler(_handleNativeDeviceAttached);
    debugPrint(
      "HostPortHandler: MethodCallHandler set up for USB device attachments from native.",
    );

    // 2. Listen for general USB attach/detach events from the usb_serial plugin
    _usbSystemEventsSubscription = UsbSerial.usbEventStream?.listen((
      UsbEvent msg,
    ) {
      if (msg.device == null) {
        debugPrint(
          "HostPortHandler: Received USB event with null device or deviceName.",
        );
        return;
      }
      final deviceName = msg.device!.deviceName;

      if (msg.event == UsbEvent.ACTION_USB_DETACHED) {
        debugPrint("HostPortHandler: USB DETACHED event for $deviceName");
        openPorts
            .remove(deviceName)
            ?.close(); // Close and remove from open ports
        _approvedDevices.remove(deviceName); // Remove from approved list
        _updateFfiAvailablePorts(); // Update FFI layer
      } else if (msg.event == UsbEvent.ACTION_USB_ATTACHED) {
        // We DON'T automatically add this device.
        // We wait for MainActivity to notify us via MethodChannel if this device
        // was launched via device_filter and the user confirmed the system dialog.
        debugPrint(
          "HostPortHandler: General USB ATTACHED event for $deviceName. Waiting for potential native confirmation if it was a filtered device.",
        );
        // If this device was NOT from a device_filter (e.g. user plugs in a random USB serial device),
        // and you still want to handle it, you might need a different flow here.
        // For now, we are focusing on the device_filter initiated flow.
      }
    });

    // 3. Listen to port events from Rust/FFI
    _rustPortEventsSubscription = subPortEvents().listen((event) async {
      switch (event) {
        case PortEvent_Open(:final request):
          {
            try {
              // Check if the device was approved via MainActivity
              final approvedDeviceDesc = _approvedDevices[request.id];
              if (approvedDeviceDesc == null) {
                throw "Device ${request.id} has not been approved for connection via system dialog.";
              }

              var port = openPorts[request.id];
              // Pass the PortDesc (which has VID/PID) from the approved list
              port ??= await SerialPort.open(
                request.id,
                request.baudRate,
                approvedDeviceDesc,
              );
              openPorts[request.id] = port;
              request.satisfy();
            } catch (e, s) {
              debugPrint(
                "HostPortHandler: Error opening port ${request.id}: $e\n$s",
              );
              request.satisfy(err: e.toString());
            }
          }
        case PortEvent_Read(
          :final request,
        ): // Your existing Read, Write, BytesToRead logic
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
              await port.write(request.bytes); // Assuming write can be async
              request.satisfy();
            } catch (e) {
              request.satisfy(err: e.toString());
            }
          }
        case PortEvent_BytesToRead(:final request):
          {
            var port = openPorts[request.id];
            if (port == null) {
              debugPrint(
                "port for ${request.id} no longer connected (for BytesToRead)",
              );
            }
            request.satisfy(bytesToRead: port?.buffer.length ?? 0);
          }
      }
    });

    _rustPortEventsSubscription!.onError((error, stackTrace) {
      debugPrint(
        "HostPortHandler: Rust port event stream error: $error\n$stackTrace",
      );
    });

    _rustPortEventsSubscription!.onDone(() {
      debugPrint(
        "HostPortHandler: Rust port event stream finished (this should ideally not happen if app is running).",
      );
    });

    // Initial update to FFI layer (might be an empty list at startup)
    _updateFfiAvailablePorts();

    debugPrint("HostPortHandler: Android serial port handler started.");
  }

  // Handles method calls from MainActivity.kt
  Future<dynamic> _handleNativeDeviceAttached(MethodCall call) async {
    debugPrint(
      "HostPortHandler: Received method call from native: ${call.method}",
    );
    switch (call.method) {
      case 'onUsbDeviceAttached':
        final Map<dynamic, dynamic>? deviceDetails = call.arguments as Map?;
        if (deviceDetails != null) {
          final int? vid = deviceDetails['vid'] as int?;
          final int? pid = deviceDetails['pid'] as int?;
          final String? deviceName =
              deviceDetails['deviceName'] as String?; // This is used as 'id'

          if (vid != null && pid != null && deviceName != null) {
            debugPrint(
              "HostPortHandler: USB Device Attached via native call: Name: $deviceName, VID: $vid, PID: $pid",
            );

            final newPortDesc = PortDesc(
              id: deviceName, // device.deviceName is used as the ID
              pid: pid,
              vid: vid,
            );

            // Add to our list of approved devices and update FFI
            _approvedDevices[deviceName] = newPortDesc;
            _updateFfiAvailablePorts();

            return "Dart: Successfully processed attached device $deviceName"; // Confirmation for native side
          } else {
            final errorMsg =
                "HostPortHandler: Insufficient device details from native for onUsbDeviceAttached. Details: $deviceDetails";
            debugPrint(errorMsg);
            throw PlatformException(code: "ARGUMENT_ERROR", message: errorMsg);
          }
        } else {
          const errorMsg =
              "HostPortHandler: No arguments received for onUsbDeviceAttached.";
          debugPrint(errorMsg);
          throw PlatformException(code: "ARGUMENT_ERROR", message: errorMsg);
        }
      default:
        final errorMsg =
            "HostPortHandler: Unknown method from native: ${call.method}";
        debugPrint(errorMsg);
        throw MissingPluginException(errorMsg);
    }
  }

  // Updates the FFI layer with the current list of approved devices
  void _updateFfiAvailablePorts() {
    if (ffiserial != null) {
      final List<PortDesc> portDescriptions = _approvedDevices.values.toList();
      debugPrint(
        "HostPortHandler: Updating FFI with available ports: ${portDescriptions.map((p) => p.id).join(', ')}",
      );
      ffiserial!.setAvailablePorts(ports: portDescriptions);
    }
  }

  // The old scanDevices() is removed as we now rely on native notifications for approved devices.
  // If you need a way to list devices that were "always allowed" at startup without a new intent,
  // that would require a more complex mechanism, potentially involving the plugin checking permissions
  // for all listed devices (which usb_serial doesn't easily expose without trying to open).

  SerialPort _getPort(String id) {
    var port = openPorts[id];
    if (port == null) {
      throw "HostPortHandler: Port '$id' is not open or has been disconnected.";
    }
    return port;
  }

  void dispose() {
    debugPrint("HostPortHandler: Disposing...");
    _rustPortEventsSubscription?.cancel();
    _usbSystemEventsSubscription?.cancel();
    _usbDeviceChannel.setMethodCallHandler(
      null,
    ); // Important to clear the handler
    openPorts.forEach((_, port) => port.close());
    openPorts.clear();
    _approvedDevices.clear();
    _updateFfiAvailablePorts(); // Inform FFI that ports are gone
  }
}

class SerialPort {
  UsbPort? _flutterUsbPort; // Renamed from 'port' to be more specific
  Uint8List buffer = Uint8List(0);
  final PortDesc _portDesc; // Store the original PortDesc for reference

  // Private constructor, called by the static open method
  SerialPort._internal(this._portDesc);

  static Future<SerialPort> open(
    String id,
    int baudRate,
    PortDesc approvedDeviceDesc,
  ) async {
    // 'id' is device.deviceName. 'approvedDeviceDesc' comes from our _approvedDevices list.
    debugPrint(
      "SerialPort: Attempting to open '$id' (VID:${approvedDeviceDesc.vid}, PID:${approvedDeviceDesc.pid}) with baud: $baudRate",
    );

    final serialPort = SerialPort._internal(approvedDeviceDesc);

    // We need to find the UsbDevice object from the plugin's list to call create() on it.
    // MainActivity already gave us the green light for this device (VID/PID/deviceName).
    List<UsbDevice> devices = await UsbSerial.listDevices();
    UsbDevice? deviceToOpen = devices.firstWhereOrNull(
      (d) =>
          d.deviceName == id &&
          d.vid == approvedDeviceDesc.vid &&
          d.pid == approvedDeviceDesc.pid,
    );

    if (deviceToOpen == null) {
      throw "SerialPort: Device '$id' (VID:${approvedDeviceDesc.vid}, PID:${approvedDeviceDesc.pid}) not found in UsbSerial.listDevices(). Was it detached?";
    }

    // This is the critical point: deviceToOpen.create() calls the native plugin.
    // If MainActivity's flow worked (user clicked "OK", session permission granted),
    // this should ideally NOT show another permission dialog.
    serialPort._flutterUsbPort = await deviceToOpen.create();
    debugPrint(
      "SerialPort: usb_serial plugin's device.create() called for '$id'.",
    );

    if (serialPort._flutterUsbPort == null) {
      throw "SerialPort: device.create() returned null for '$id'. Plugin failed to create port.";
    }

    var opened = await serialPort._flutterUsbPort!.open();
    if (!opened) {
      await serialPort.close(); // Ensure resources are released if open fails
      throw "SerialPort: Couldn't open UsbPort for device '$id' via plugin (open() returned false).";
    }
    debugPrint("SerialPort: UsbPort opened for '$id' via plugin.");

    final inputStream = serialPort._flutterUsbPort!.inputStream;
    if (inputStream == null) {
      await serialPort.close();
      throw "SerialPort: Input stream is null after opening port for '$id'.";
    }

    inputStream.listen(
      (Uint8List bytes) {
        // A more robust way to concatenate Uint8List
        var newBuffer = Uint8List(serialPort.buffer.length + bytes.length);
        newBuffer.setAll(0, serialPort.buffer);
        newBuffer.setAll(serialPort.buffer.length, bytes);
        serialPort.buffer = newBuffer;
        // You might want to notify your FFI/Rust layer here that new data is available
        // if it doesn't poll `BytesToRead`.
      },
      onError: (error) {
        debugPrint("SerialPort: Error on input stream for '$id': $error");
        serialPort.close(); // Close port on stream error
      },
      onDone: () {
        debugPrint("SerialPort: Input stream done for '$id'.");
        serialPort.close(); // Port is no longer usable if stream is done
      },
      cancelOnError: true, // Automatically cancels subscription on error
    );

    return serialPort;
  }

  Uint8List read(int len) {
    if (_flutterUsbPort == null)
      throw "SerialPort: Port '$id' not open for read.";
    len = min(len, buffer.length);
    var res = buffer.sublist(0, len);
    buffer = buffer.sublist(len);
    return res;
  }

  Future<void> write(Uint8List bytes) async {
    if (_flutterUsbPort == null)
      throw "SerialPort: Port '$id' not open for write.";
    try {
      await _flutterUsbPort!.write(bytes);
    } catch (e) {
      debugPrint("SerialPort: Error writing to port '$id': $e");
      // Rethrow or handle as appropriate
      rethrow;
    }
  }

  Future<void> close() async {
    if (_flutterUsbPort != null) {
      try {
        await _flutterUsbPort!.close();
        debugPrint("SerialPort: UsbPort closed for '${_portDesc.id}'.");
      } catch (e) {
        debugPrint(
          "SerialPort: Error closing UsbPort for '${_portDesc.id}': $e",
        );
      } finally {
        _flutterUsbPort = null;
      }
    }
    buffer = Uint8List(0); // Clear buffer
  }

  // Helper to get the ID, useful for error messages if _portDesc is available
  String get id => _portDesc.id;
}
