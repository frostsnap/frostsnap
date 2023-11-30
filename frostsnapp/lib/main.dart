import 'package:flutter/material.dart';
import 'package:frostsnapp/key_list.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:io';
import 'package:flutter/rendering.dart';

void main() {
  // enable this if you're trying to figure out why things are displaying in
  // certain positions/sizes
  debugPaintSizeEnabled = false;
  if (Platform.isAndroid) {
    api.turnLogcatLoggingOn(level: Level.Debug);
    api.switchToHostHandlesSerial();
  } else {
    api.turnStderrLoggingOn(level: Level.Debug);
  }
  api.startCoordinatorThread();

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
        colorScheme: ColorScheme.fromSwatch(
          primarySwatch: Colors.blue,
          backgroundColor: Colors.white,
          errorColor: Colors.red,
        ).copyWith(
          secondary: Colors.blueAccent,
        ),
        textButtonTheme: TextButtonThemeData(
          style: TextButton.styleFrom(backgroundColor: Colors.blueAccent),
        ),
        elevatedButtonTheme: ElevatedButtonThemeData(
          style: ElevatedButton.styleFrom(
            backgroundColor: Colors.blue,
            foregroundColor: Colors.white,
          ),
        ),
        outlinedButtonTheme: OutlinedButtonThemeData(
          style: OutlinedButton.styleFrom(
            backgroundColor: Colors.blue,
            side: BorderSide(color: Colors.blue),
          ),
        ),
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
    return Scaffold(
        appBar: AppBar(title: Text("Key List")),
        body: Center(child: KeyListWithConfetti()));
  }
}
