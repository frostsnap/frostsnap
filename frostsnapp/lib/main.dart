import 'package:flutter/material.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/key_list.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/serialport.dart';
import 'package:path_provider/path_provider.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:io';
import 'package:flutter/rendering.dart';
import 'package:wakelock_plus/wakelock_plus.dart';

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
      api.turnLogcatLoggingOn(level: LogLevel.Debug);
    } else {
      api.turnStderrLoggingOn(level: LogLevel.Debug);
    }
    final appDir = await getApplicationSupportDirectory();
    final dbFile = '${appDir.path}/frostsnap.sqlite';
    if (Platform.isAndroid) {
      final (coord_, ffiserial, wallet_, bitcoinContext_) =
          await api.loadHostHandlesSerial(dbFile: dbFile);
      globalHostPortHandler = HostPortHandler(ffiserial);
      coord = coord_;
      wallet = wallet_;
      bitcoinContext = bitcoinContext_;
      // check for devices that were plugged in before the app even started
      globalHostPortHandler.scanDevices();
    } else {
      final (coord_, wallet_, bitcoinContext_) = await api.load(dbFile: dbFile);
      globalHostPortHandler = HostPortHandler(null);
      coord = coord_;
      wallet = wallet_;
      bitcoinContext = bitcoinContext_;
    }
    coord.startThread();
  } catch (error, stacktrace) {
    print("$error");
    print("$stacktrace");
    startupError = "$error\n$stacktrace";
  }

  // Lock orientation to portrait mode only
  SystemChrome.setPreferredOrientations([
    DeviceOrientation.portraitUp,
    DeviceOrientation.portraitDown,
  ]);

  // we want to stop the app from sleeping on mobile if there's a device plugged in.
  deviceListSubject.forEach((update) {
    if (Platform.isLinux) {
      return; // not supported by wakelock
    }
    if (update.state.devices.isNotEmpty) {
      WakelockPlus.enable();
    } else {
      WakelockPlus.disable();
    }
  });
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
        theme: ThemeData(
            dividerTheme: const DividerThemeData(color: Colors.black12),
            appBarTheme: AppBarTheme(
                shadowColor: Colors.black,
                elevation: 6.0,
                surfaceTintColor: Colors.white),
            colorScheme: ColorScheme.fromSwatch(
              primarySwatch: Colors.blue,
              backgroundColor: Colors.white,
              accentColor: Colors.blueAccent,
              errorColor: Colors.red,
            ),
            textButtonTheme: TextButtonThemeData(
              style: TextButton.styleFrom(
                  backgroundColor: Colors.blueAccent,
                  foregroundColor: Colors.white),
            ),
            elevatedButtonTheme: ElevatedButtonThemeData(
              style: ElevatedButton.styleFrom(
                backgroundColor: Colors.blue,
                foregroundColor: Colors.white,
              ),
            ),
            inputDecorationTheme: InputDecorationTheme(
              border: OutlineInputBorder(), // Apply border globally
              enabledBorder: OutlineInputBorder(
                borderSide: BorderSide(color: Colors.grey),
              ),
              focusedBorder: OutlineInputBorder(
                borderSide: BorderSide(color: Colors.blue),
              ),
            )),
        home: startupError == null
            ? const MyHomePage(title: 'Frostsnapp')
            : StartupErrorWidget(error: startupError!));
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
                  color: Colors.red,
                ),
              ),
              SizedBox(height: 8),
              Text(
                'Please report this to the frostsnap team',
                style: TextStyle(
                  fontSize: 16.0,
                  color: Colors.black54,
                ),
              ),
              SizedBox(height: 20),
              Container(
                padding: EdgeInsets.all(8.0),
                decoration: BoxDecoration(
                  color: Colors.grey[200],
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(color: Colors.grey[400]!),
                ),
                child: SelectableText(
                  error,
                  style: TextStyle(
                    fontFamily: 'Courier', // Monospaced font
                    color: Colors.black,
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
