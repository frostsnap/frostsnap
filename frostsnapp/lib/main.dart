import 'dart:async';
import 'dart:io';

import 'package:confetti/confetti.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:google_fonts/google_fonts.dart';
import 'package:path_provider/path_provider.dart';

import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/serialport.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_list_controller.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/init.dart';
import 'package:frostsnap/src/rust/api/log.dart';
import 'package:frostsnap/src/rust/frb_generated.dart';

Future<void> main() async {
  // enable this if you're trying to figure out why things are displaying in
  // certain positions/sizes
  debugPaintSizeEnabled = false;
  // dunno what this is but for some reason it's needed 🤦
  // https://stackoverflow.com/questions/57689492/flutter-unhandled-exception-servicesbinding-defaultbinarymessenger-was-accesse
  WidgetsFlutterBinding.ensureInitialized();

  // 💡 renable if you want to mess around with different fonts
  GoogleFonts.config.allowRuntimeFetching = false;
  // 🖕 to all intellectual property but I am doing what I am told.
  LicenseRegistry.addLicense(() async* {
    final license = await rootBundle.loadString('assets/google_fonts/OFL.txt');
    yield LicenseEntryWithLineBreaks(['google_fonts'], license);
  });

  await RustLib.init();
  api = Api();

  String? startupError;
  // // set logging up first before doing anything else
  final Stream<String> logStream = api
      .turnLoggingOn(level: LogLevel.debug)
      .toReplaySubject();

  // // wait for first message to appear so that logging is working before we carry on
  await logStream.first;
  AppCtx? appCtx;

  try {
    final appDir = await getApplicationSupportDirectory();
    final appDirPath = appDir.path;
    if (Platform.isAndroid) {
      final (coord_, appCtx_, ffiserial) = await api.loadHostHandlesSerial(
        appDir: appDirPath,
      );
      globalHostPortHandler = HostPortHandler(ffiserial);
      coord = coord_;
      appCtx = appCtx_;
    } else {
      final (coord_, appCtx_) = await api.load(appDir: appDirPath);
      coord = coord_;
      appCtx = appCtx_;
      globalHostPortHandler = null;
    }
    coord.startThread();
  } on PanicException catch (e) {
    startupError = "rust panic'd with: ${e.message}";
  } on AnyhowException catch (e, stacktrace) {
    startupError = "rust error: ${e.message}\n$stacktrace";
  } catch (error, stacktrace) {
    startupError = "$error\n$stacktrace";
    log(level: LogLevel.info, message: "startup failed with $startupError");
  }

  runApp(MyApp(startupError: startupError));
}

Widget buildMainWidget(AppCtx appCtx, Stream<String> logStream) {
  return FrostsnapContext(
    appCtx: appCtx,
    logStream: logStream,
    child: SettingsContext(
      settings: appCtx.settings,
      child: SuperWalletContext(appCtx: appCtx, child: MyApp()),
    ),
  );
}

class MyApp extends StatefulWidget {
  final String? startupError;

  const MyApp({super.key, this.startupError});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  late final Future<List<void>> googleFontsPending;
  late ColorScheme colorScheme;

  void _setColorTheme() => colorScheme = ColorScheme.fromSeed(
    brightness: Brightness.dark,
    seedColor: seedColor,
  );

  @override
  void initState() {
    super.initState();
    googleFontsPending = GoogleFonts.pendingFonts([
      GoogleFonts.notoSansMono(),
      GoogleFonts.notoSansTextTheme(),
    ]);
    _setColorTheme();
  }

  @override
  void reassemble() {
    super.reassemble();
    _setColorTheme();
  }

  // This widget is the root of your application.
  @override
  Widget build(BuildContext context) {
    final baseTheme = ThemeData(useMaterial3: true, colorScheme: colorScheme);

    return FutureBuilder(
      future: googleFontsPending,
      builder: (context, snapshot) {
        if (snapshot.connectionState != ConnectionState.done) {
          return Center(child: CircularProgressIndicator());
        }
        final textTheme = GoogleFonts.notoSansTextTheme(baseTheme.textTheme);

        return MaterialApp(
          navigatorKey: rootNavKey,
          scaffoldMessengerKey: rootScaffoldMessengerKey,
          title: 'Frostsnap',
          theme: baseTheme.copyWith(
            colorScheme: colorScheme,
            textTheme: textTheme,
          ),
          home: widget.startupError == null
              ? const MyHomePage()
              : StartupErrorWidget(error: widget.startupError!),
          debugShowCheckedModeBanner: false,
        );
      },
    );
  }
}

class MyHomePage extends StatefulWidget {
  const MyHomePage({super.key});

  @override
  State<MyHomePage> createState() => _MyHomePageState();
}

class _MyHomePageState extends State<MyHomePage> {
  late final GlobalKey<ScaffoldState> scaffoldKey;
  late final WalletListController walletListController;
  late final ConfettiController confettiController;

  @override
  void initState() {
    super.initState();
    scaffoldKey = GlobalKey();
    walletListController = WalletListController(
      keyStream: GlobalStreams.keyStateSubject,
    );
    confettiController = ConfettiController(duration: Duration(seconds: 4));
  }

  @override
  void dispose() {
    confettiController.dispose();
    walletListController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return HomeContext(
      scaffoldKey: scaffoldKey,
      walletListController: walletListController,
      confettiController: confettiController,
      child: Stack(
        alignment: AlignmentDirectional.center,
        children: [
          const WalletHome(),
          Center(
            child: ConfettiWidget(
              confettiController: confettiController,
              blastDirectionality: BlastDirectionality.explosive,
              numberOfParticles: 101,
            ),
          ),
        ],
      ),
    );
  }
}

class StartupErrorWidget extends StatefulWidget {
  final String error;

  const StartupErrorWidget({super.key, required this.error});

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
        _subscription = logStream.listen((log) {
          setState(() {
            _logs.add(log);
          });
        });
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
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: Text('Startup Error')),
      body: Padding(
        padding: EdgeInsets.all(16.0),
        child: SingleChildScrollView(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.start,
            crossAxisAlignment: CrossAxisAlignment.center,
            children: <Widget>[
              Text(
                'STARTUP ERROR',
                style: theme.textTheme.titleMedium?.copyWith(
                  color: theme.colorScheme.error,
                ),
              ),
              SizedBox(height: 8),
              Text(
                "Sorry! Something has gone wrong with the app. Please report this directly to the frostsnap team.",
                style: theme.textTheme.titleMedium,
              ),
              SizedBox(height: 16),
              Container(
                padding: EdgeInsets.all(12.0),
                decoration: BoxDecoration(
                  color: theme.colorScheme.primaryContainer,
                  borderRadius: BorderRadius.circular(8.0),
                ),
                child: Row(
                  children: [
                    Icon(
                      Icons.info_outline,
                      color: theme.colorScheme.onPrimaryContainer,
                    ),
                    SizedBox(width: 12),
                    Expanded(
                      child: Text(
                        "Try upgrading to the latest version of Frostsnap. This may resolve the issue.",
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: theme.colorScheme.onPrimaryContainer,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              SizedBox(height: 12),
              Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text("Contact support: ", style: theme.textTheme.bodyMedium),
                  InkWell(
                    onTap: () {
                      Clipboard.setData(
                        ClipboardData(text: "support@frostsnap.com"),
                      );
                      ScaffoldMessenger.of(context).showSnackBar(
                        SnackBar(
                          content: Text("Email copied to clipboard"),
                          duration: Duration(seconds: 2),
                        ),
                      );
                    },
                    child: Text(
                      "support@frostsnap.com",
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: theme.colorScheme.primary,
                        decoration: TextDecoration.underline,
                      ),
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),
              Builder(
                builder: (context) {
                  const buildVersion = String.fromEnvironment(
                    'BUILD_VERSION',
                    defaultValue: 'unknown',
                  );
                  return Text(
                    "Current version: $buildVersion",
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onSurfaceVariant,
                    ),
                  );
                },
              ),
              SizedBox(height: 20),
              Container(
                width: double.infinity, // Ensure the container takes full width
                padding: EdgeInsets.all(8.0),
                decoration: BoxDecoration(
                  color: theme.colorScheme.surfaceContainer,
                  borderRadius: BorderRadius.circular(4.0),
                  border: Border.all(),
                ),
                child: SelectableText(_combinedErrorWithLogs),
              ),
              SizedBox(height: 20),
              IconButton(
                icon: Icon(Icons.content_copy),
                onPressed: () {
                  Clipboard.setData(
                    ClipboardData(text: _combinedErrorWithLogs),
                  );
                  ScaffoldMessenger.of(context).showSnackBar(
                    SnackBar(
                      content: Text(
                        "Copied! Only send this to the Frostsnap team.",
                      ),
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
