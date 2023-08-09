import 'package:flutter/material.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:async';
import 'dart:developer' as developer;
import 'dart:io';
import 'package:usb_serial/usb_serial.dart';
import 'package:flutter/services.dart';
import 'serialport.dart';
import 'device_list.dart';

Timer? timer;

void main() {
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({Key? key}) : super(key: key);

  // This widget is the root of your application.
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Frostsnapp',
      theme: ThemeData(
        // This is the theme of your application.
        //
        // Try running your application with "flutter run". You'll see the
        // application has a blue toolbar. Then, without quitting the app, try
        // changing the primarySwatch below to Colors.green and then invoke
        // "hot reload" (press "r" in the console where you ran "flutter run",
        // or simply save your changes to "hot reload" in a Flutter IDE).
        // Notice that the counter didn't reset back to zero; the application
        // is not restarted.
        primarySwatch: Colors.blue,
      ),
      home: const MyHomePage(title: 'Frostsnapp'),
    );
  }
}

class MyHomePage extends StatefulWidget {
  const MyHomePage({Key? key, required this.title}) : super(key: key);

  // This widget is the home page of your application. It is stateful, meaning
  // that it has a State object (defined below) that contains fields that affect
  // how it looks.

  // This class is the configuration for the state. It holds the values (in this
  // case the title) provided by the parent (in this case the App widget) and
  // used by the build method of the State. Fields in a Widget subclass are
  // always marked "final".

  final String title;

  @override
  State<MyHomePage> createState() => _MyHomePageState();
}

class _MyHomePageState extends State<MyHomePage> {
  // These futures belong to the state and are only initialized once,
  // in the initState method.
  late Future<FfiCoordinator> ffi;
  FfiCoordinator? coordinator;
  Map<String, SerialPort> openPorts = {};

  @override
  void initState() {
    super.initState();
    ffi = api.newFfiCoordinator(hostHandlesSerial: Platform.isAndroid);

    if (Platform.isAndroid) {
      api.turnLogcatLoggingOn(level: Level.Debug);
      UsbSerial.usbEventStream?.listen((UsbEvent msg) {
        if (msg.event == UsbEvent.ACTION_USB_DETACHED) {
          openPorts.remove(msg.device?.deviceName);
        }
        announceDevices();
      });
    } else {
      api.turnStderrLoggingOn(level: Level.Debug);
    }
  }

  SerialPort _getPort(String id) {
    var port = openPorts[id];
    if (port == null) {
      throw "port $id has been disconnected";
    }
    return port;
  }

  void announceDevices() async {
    var ctx = await ffi;
    List<UsbDevice> devices = await UsbSerial.listDevices();
    final List<PortDesc> portDescriptions = devices
        .where((device) => device.vid != null && device.pid != null)
        .map((device) =>
            PortDesc(id: device.deviceName, pid: device.pid!, vid: device.vid!))
        .toList();
    await api.announceAvailablePorts(coordinator: ctx, ports: portDescriptions);
  }

  @override
  Widget build(BuildContext context) {
    // This method is rerun every time setState is called.
    //
    // The Flutter framework has been optimized to make rerunning build methods
    // fast, so that you can just rebuild anything that needs updating rather
    // than having to individually change instances of widgets.
    return Scaffold(
      appBar: AppBar(
        // Here we take the value from the MyHomePage object that was created by
        // the App.build method, and use it to set our appbar title.
        title: Text(widget.title),
      ),
      body: Center(
          // Center is a layout widget. It takes a single child and positions it
          // in the middle of the parent.
          child: FutureBuilder<List<dynamic>>(
        // To render the results of a Future, a FutureBuilder is used which
        // turns a Future into an AsyncSnapshot, which can be used to
        // extract the error state, the loading state and the data if
        // available.
        //
        // Here, the generic type that the FutureBuilder manages is
        // explicitly named, because if omitted the snapshot will have the
        // type of AsyncSnapshot<Object?>.

        // We await two unrelated futures here, so the type has to be
        // List<dynamic>.
        future: Future.wait([this.ffi]),
        builder: (context, snap) {
          final style = Theme.of(context).textTheme.headlineMedium;
          if (snap.error != null) {
            // An error has been encountered, so give an appropriate response and
            // pass the error details to an unobstructive tooltip.
            debugPrint(snap.error.toString());
            return Tooltip(
              message: snap.error.toString(),
              child: Text('ERROR', style: style),
            );
          }

          // Guard return here, the data is not ready yet.
          final data = snap.data;
          if (data == null) return const CircularProgressIndicator();
          final coordinator = data[0];

          final deviceEvents = api.subDeviceEvents();
          api.initEvents().forEach((event) async {
            switch (event) {
              case CoordinatorEvent_PortOpen(:final request):
                {
                  try {
                    var port = openPorts[request.id];
                    port ??=
                        await SerialPort.open(request.id, request.baudRate);
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
          return Container(
              width: 400,
              alignment: Alignment.bottomLeft,
              child: DeviceListWidget(
                  coordinator: coordinator, deviceEvents: deviceEvents));
        },
      )),
    );
  }
}
