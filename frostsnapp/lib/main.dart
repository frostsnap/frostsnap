import 'package:flutter/material.dart';
import 'package:frostsnapp/key_list.dart';
import 'package:frostsnapp/keygen.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:async';
import 'dart:io';
import 'device_list_widget.dart';

Timer? timer;

void main() {
  if (Platform.isAndroid) {
    api.turnLogcatLoggingOn(level: Level.Debug);
    api.switchToHostHandlesSerial();
  } else {
    api.turnStderrLoggingOn(level: Level.Debug);
  }
  api.startCoordinatorThread();

  runApp(const MyApp());
}

// final Map<String, WidgetBuilder> routes = {
//   '/home': (context) => MyHomePage(title: 'Frostsnapp'),
//   '/keygen': (context) {
//     final threshold = ModalRoute.of(context)?.settings.arguments as int?;
//     return DoKeyGenScreen(threshold: threshold ?? 1); // default threshold
//   },
//   '/wallet': (context) {
//     final publicKey = ModalRoute.of(context)?.settings.arguments as String?;
//     return KeyDisplayPage(publicKey: publicKey ?? "missing");
//   },
// };

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

class MyHomePage extends StatelessWidget {
  const MyHomePage({super.key, required this.title});
  final String title;

  @override
  Widget build(BuildContext context) {
    final deviceList = DeviceListContainer(child: KeyGenDeviceList());
    return Scaffold(
        appBar: AppBar(title: Text("Key List")),
        body: Center(child: KeyListWithConfetti()));
  }
}
