import 'dart:async';
import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/key_list.dart';
import 'package:flutter/services.dart';
import 'package:frostsnapp/settings.dart';
import 'package:frostsnapp/serialport.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:path_provider/path_provider.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:io';
import 'package:flutter/rendering.dart';
import 'package:wakelock_plus/wakelock_plus.dart';
import 'theme.dart';

void main() async {
  // enable this if you're trying to figure out why things are displaying in
  // certain positions/sizes
  debugPaintSizeEnabled = false;
  // dunno what this is but for some reason it's needed 🤦
  // https://stackoverflow.com/questions/57689492/flutter-unhandled-exception-servicesbinding-defaultbinarymessenger-was-accesse
  WidgetsFlutterBinding.ensureInitialized();

  String? startupError;
  Stream<String> logStream;

  // set logging up first before doing anything else

  if (Platform.isAndroid) {
    logStream =
        api.turnLogcatLoggingOn(level: LogLevel.Debug).toReplaySubject();
  } else {
    logStream =
        api.turnStderrLoggingOn(level: LogLevel.Debug).toReplaySubject();
  }

  // wait for first message to appear so that logging is working before we carry on
  await logStream.first;
  Settings? settings;

  try {
    final appDir = await getApplicationSupportDirectory();
    final appDirPath = appDir.path;
    if (Platform.isAndroid) {
      final (coord_, settings_, ffiserial) =
          await api.loadHostHandlesSerial(appDir: appDirPath);
      globalHostPortHandler = HostPortHandler(ffiserial);
      coord = coord_;
      settings = settings_;
      // check for devices that were plugged in before the app even started
      globalHostPortHandler.scanDevices();
    } else {
      final (coord_, settings_) = await api.load(appDir: appDirPath);
      coord = coord_;
      settings = settings_;
      globalHostPortHandler = HostPortHandler(null);
    }
    coord.startThread();
  } on PanicException catch (e) {
    startupError = "rust panic'd with: ${e.error}";
  } on FrbAnyhowException catch (e, stacktrace) {
    startupError = "rust error: ${e.anyhow}\n$stacktrace";
  } catch (error, stacktrace) {
    startupError = "$error\n$stacktrace";
    api.log(level: LogLevel.Info, message: "startup failed with $startupError");
    runApp(MyApp(startupError: startupError));
  }

  if (startupError != null) {
    runApp(MyApp(startupError: startupError));
  } else {
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
    // Lock orientation to portrait mode only
    SystemChrome.setPreferredOrientations([
      DeviceOrientation.portraitUp,
      DeviceOrientation.portraitDown,
    ]);

    Widget mainWidget = FrostsnapContext(logStream: logStream, child: MyApp());

    if (settings != null) {
      mainWidget = SettingsContext(settings: settings, child: mainWidget);
    }

    runApp(mainWidget);
  }
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
        appBar: FsAppBar(title: Text("Wallets")),
        body: Center(child: KeyListWithConfetti()));
  }
}

class StartupErrorWidget extends StatefulWidget {
  final String error;

  const StartupErrorWidget({Key? key, required this.error}) : super(key: key);

  @override
  State<StartupErrorWidget> createState() => _StartupErrorWidgetState();
}

class _StartupErrorWidgetState extends State<StartupErrorWidget> {
  final List<String> _logs = [];
  StreamSubscription<String>? _subscription;

  @override
  void initState() {
    super.initState();
    // Delay the context access until after the first frame
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final logStream = FrostsnapContext.of(context)?.logStream;

      if (logStream != null) {
        _subscription = logStream.listen(
          (log) {
            setState(() {
              _logs.add(log);
            });
          },
        );
      }
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  /// Combines all logs and the error message into a single string.
  String get _combinedErrorWithLogs {
    if (_logs.isEmpty) {
      return widget.error;
    }

    // Format each log entry
    final String logsText = _logs.join('\n');

    // Combine logs with the error message
    return '$logsText\n------------------\n${widget.error}';
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text('Startup Error'),
      ),
      body: Padding(
        padding: EdgeInsets.all(16.0),
        child: Center(
          child: SingleChildScrollView(
            // To handle overflow if logs are extensive
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.center,
              children: <Widget>[
                Text(
                  'STARTUP ERROR',
                  style: TextStyle(
                    fontSize: 24.0,
                    fontWeight: FontWeight.bold,
                    color: errorColor,
                  ),
                ),
                SizedBox(height: 8),
                Text(
                  "Sorry! Something has gone wrong with the app. Please report this directly to the frostsnap team.",
                  style: TextStyle(
                    fontSize: 16.0,
                    color: textColor,
                  ),
                ),
                SizedBox(height: 20),
                Container(
                  width:
                      double.infinity, // Ensure the container takes full width
                  padding: EdgeInsets.all(8.0),
                  decoration: BoxDecoration(
                    color:
                        backgroundSecondaryColor, // Replace with your `textSecondaryColor`
                    borderRadius: BorderRadius.circular(4.0),
                    border: Border.all(),
                  ),
                  child: SelectableText(
                    _combinedErrorWithLogs,
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
                    Clipboard.setData(
                        ClipboardData(text: _combinedErrorWithLogs));
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(
                        content: Text(
                            "Copied! Only send this to the Frostsnap team."),
                      ),
                    );
                  },
                  tooltip: 'Copy to Clipboard',
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class FrostsnapContext extends InheritedWidget {
  final Stream<String> logStream;

  const FrostsnapContext({
    Key? key,
    required this.logStream,
    required Widget child,
  }) : super(key: key, child: child);

  // Static method to allow easy access to the Foo instance
  static FrostsnapContext? of(BuildContext context) {
    return context.dependOnInheritedWidgetOfExactType<FrostsnapContext>();
  }

  @override
  bool updateShouldNotify(FrostsnapContext oldWidget) {
    // we never change the log stream
    return false;
  }
}
