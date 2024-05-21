import 'package:flutter/material.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/key_list.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/serialport.dart';
import 'package:path_provider/path_provider.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:io';
import 'package:flutter/rendering.dart';
import 'theme.dart';

void main() async {
  // enable this if you're trying to figure out why things are displaying in
  // certain positions/sizes
  debugPaintSizeEnabled = false;
  // dunno what this is but for some reason it's needed 🤦
  // https://stackoverflow.com/questions/57689492/flutter-unhandled-exception-servicesbinding-defaultbinarymessenger-was-accesse
  WidgetsFlutterBinding.ensureInitialized();

  String? startupError;

  try {
    // set logging up first before doing anything else
    if (Platform.isAndroid) {
      api.turnLogcatLoggingOn(level: Level.Debug);
    } else {
      api.turnStderrLoggingOn(level: Level.Debug);
    }
    final appDir = await getApplicationSupportDirectory();
    final dbFile = '${appDir.path}/frostsnap.db';
    if (Platform.isAndroid) {
      final (coord_, ffiserial, wallet_) =
          await api.loadHostHandlesSerial(dbFile: dbFile);
      globalHostPortHandler = HostPortHandler(ffiserial);
      coord = coord_;
      wallet = wallet_;
      // check for devices that were plugged in before the app even started
      globalHostPortHandler.scanDevices();
    } else {
      final (coord_, wallet_) = await api.load(dbFile: dbFile);
      globalHostPortHandler = HostPortHandler(null);
      coord = coord_;
      wallet = wallet_;
    }
    coord.startThread();
  } catch (error, stacktrace) {
    print("$error");
    print("$stacktrace");
    startupError = "$error\n$stacktrace";
  }
  deviceListSubject;
  runApp(MyApp(startupError: startupError));
}

class MyApp extends StatelessWidget {
  final String? startupError;

  const MyApp({Key? key, this.startupError}) : super(key: key);

  // This widget is the root of your application.
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
        title: 'Frostsnapp',
        theme: frostsnappTheme,
        home: startupError == null
            ? const MyHomePage(title: 'Frostsnapp')
            : StartupErrorWidget(error: startupError!),
        debugShowCheckedModeBanner: false);
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

class StartupErrorWidget extends StatelessWidget {
  final String error;

  const StartupErrorWidget({Key? key, required this.error}) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('Startup Error'),
      ),
      body: Padding(
        padding: EdgeInsets.all(16.0),
        child: Center(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.center,
            children: <Widget>[
              Text(
                'ERROR',
                style: TextStyle(
                  fontSize: 24.0,
                  fontWeight: FontWeight.bold,
                  color: errorColor,
                ),
              ),
              SizedBox(height: 8),
              Text(
                'Please report this to the frostsnap team',
                style: TextStyle(
                  fontSize: 16.0,
                  color: textColor,
                ),
              ),
              SizedBox(height: 20),
              Container(
                padding: EdgeInsets.all(8.0),
                decoration: BoxDecoration(
                  color: backgroundSecondaryColor,
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(color: backgroundPrimaryColor),
                ),
                child: SelectableText(
                  error,
                  style: TextStyle(
                    fontFamily: 'Courier', // Monospaced font
                    color: textColor,
                  ),
                ),
              ),
              SizedBox(height: 20),
              IconButton(
                icon: Icon(Icons.content_copy),
                onPressed: () {
                  Clipboard.setData(ClipboardData(text: error));
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(
                      content: Text('Error message copied to clipboard!'),
                    ),
                  );
                },
                tooltip: 'Copy to Clipboard',
              ),
            ],
          ),
        ),
      ),
    );
  }
}
