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
import 'package:wakelock_plus/wakelock_plus.dart';

import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/serialport.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet.dart';
import 'package:frostsnap/wallet_list_controller.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/init.dart';
import 'package:frostsnap/src/rust/api/database_encryption.dart';
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

  final Stream<String> logStream = api
      .turnLoggingOn(level: LogLevel.debug)
      .toReplaySubject();
  await logStream.first;

  String? appDirPath;
  if (!Platform.isAndroid) {
    final appDir = await getApplicationSupportDirectory();
    appDirPath = appDir.path;
  }

  runApp(FrostsnapAppInitializer(appDirPath: appDirPath, logStream: logStream));
}

Future<Widget> initializeApp({
  String? password,
  Stream<String>? logStream,
}) async {
  String? startupError;

  // Use provided logStream or create new one
  final Stream<String> logStreamToUse =
      logStream ?? api.turnLoggingOn(level: LogLevel.debug).toReplaySubject();

  // If we created a new stream, wait for first message
  if (logStream == null) {
    await logStreamToUse.first;
  }

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
      final (coord_, appCtx_) = await api.load(
        appDir: appDirPath,
        password: password,
      );
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

  if (startupError != null) {
    return FrostsnapApp(startupError: startupError);
  } else {
    GlobalStreams.deviceListSubject.forEach((update) {
      // If we detect a device that's in recovery mode we should tell it to exit
      // ASAP. Right now we don't confirm with the user this action but maybe in
      // the future we will.
      for (var change in update.changes) {
        if (change.kind == DeviceListChangeKind.recoveryMode &&
            change.device.recoveryMode) {
          SecureKeyProvider.getEncryptionKey().then((encryptionKey) {
            coord.exitRecoveryMode(
              deviceId: change.device.id,
              encryptionKey: encryptionKey,
            );
          });
        }
      }

      // we want to stop the app from sleeping on mobile if there's a device plugged in.
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
    SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
    SystemChrome.setSystemUIOverlayStyle(
      SystemUiOverlayStyle(systemNavigationBarColor: Colors.transparent),
    );

    final mainWidget = buildMainWidget(appCtx!, logStreamToUse);
    return mainWidget;
  }
}

Widget buildMainWidget(AppCtx appCtx, Stream<String> logStream) {
  return FrostsnapContext(
    appCtx: appCtx,
    logStream: logStream,
    child: SettingsContext(
      settings: appCtx.settings,
      child: SuperWalletContext(appCtx: appCtx, child: FrostsnapApp()),
    ),
  );
}

class FrostsnapApp extends StatefulWidget {
  final String? startupError;

  const FrostsnapApp({super.key, this.startupError});

  @override
  State<FrostsnapApp> createState() => _FrostsnapAppState();
}

class _FrostsnapAppState extends State<FrostsnapApp> {
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
              ? const FrostsnapAppHomePage()
              : StartupErrorWidget(error: widget.startupError!),
          debugShowCheckedModeBanner: false,
        );
      },
    );
  }
}

class FrostsnapAppHomePage extends StatefulWidget {
  const FrostsnapAppHomePage({super.key});

  @override
  State<FrostsnapAppHomePage> createState() => _FrostsnapAppHomePageState();
}

class _FrostsnapAppHomePageState extends State<FrostsnapAppHomePage> {
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
        child: Center(
          child: SingleChildScrollView(
            // To handle overflow if logs are extensive
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
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
                SizedBox(height: 20),
                Container(
                  width:
                      double.infinity, // Ensure the container takes full width
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
      ),
    );
  }
}

class FrostsnapAppInitializer extends StatefulWidget {
  final String? appDirPath;
  final Stream<String> logStream;

  const FrostsnapAppInitializer({
    super.key,
    this.appDirPath,
    required this.logStream,
  });

  @override
  State<FrostsnapAppInitializer> createState() =>
      _FrostsnapAppInitializerState();
}

class _FrostsnapAppInitializerState extends State<FrostsnapAppInitializer> {
  String? _password;
  bool _needsPassword = false;
  bool _isLoading = true;
  Widget? _mainWidget;

  @override
  void initState() {
    super.initState();
    _initializeApp();
  }

  Future<void> _initializeApp() async {
    if (widget.appDirPath != null && !Platform.isAndroid) {
      final databaseState = await api.getDatabaseState(
        appDir: widget.appDirPath!,
      );
      if (databaseState == DbEncryptionState.existingEncrypted) {
        setState(() {
          _needsPassword = true;
          _isLoading = false;
        });
        return;
      }
      // If encrypted with empty password, set password to empty string
      if (databaseState == DbEncryptionState.existingEncryptedEmpty) {
        _password = "";
      }
    }

    await _loadMainApp();
  }

  Future<void> _loadMainApp() async {
    try {
      // Use the existing initializeApp function with password parameter
      final appWidget = await initializeApp(
        password: _password,
        logStream: widget.logStream,
      );

      setState(() {
        _mainWidget = appWidget;
        _isLoading = false;
        _needsPassword = false;
      });
    } catch (e) {
      // Handle startup errors by showing the error widget
      setState(() {
        _mainWidget = FrostsnapApp(startupError: e.toString());
        _isLoading = false;
      });
    }
  }

  Future<void> _onPasswordSubmitted(String password) async {
    try {
      if (widget.appDirPath != null) {
        await api.attemptDatabasePassword(
          appDir: widget.appDirPath!,
          password: password,
        );
      }

      setState(() {
        _password = password;
        _isLoading = true;
      });

      await _loadMainApp();
    } catch (e) {
      rethrow; // Let PasswordScreen handle the error display
    }
  }

  @override
  Widget build(BuildContext context) {
    // basic theme without awaiting custom fonts
    final colorScheme = ColorScheme.fromSeed(
      brightness: Brightness.dark,
      seedColor: seedColor,
    );
    final baseTheme = ThemeData(useMaterial3: true, colorScheme: colorScheme);
    final theme = baseTheme.copyWith(colorScheme: colorScheme);

    if (_isLoading) {
      return MaterialApp(
        theme: theme,
        home: Scaffold(body: Center(child: CircularProgressIndicator())),
      );
    }

    if (_needsPassword) {
      return MaterialApp(
        theme: theme,
        home: PasswordScreen(
          appDirPath: widget.appDirPath!,
          onPasswordSubmitted: _onPasswordSubmitted,
        ),
      );
    }

    return _mainWidget ??
        MaterialApp(
          theme: theme,
          home: Scaffold(body: Center(child: Text('Loading...'))),
        );
  }
}

class PasswordScreen extends StatefulWidget {
  final String appDirPath;
  final Future<void> Function(String) onPasswordSubmitted;

  const PasswordScreen({
    super.key,
    required this.appDirPath,
    required this.onPasswordSubmitted,
  });

  @override
  State<PasswordScreen> createState() => _PasswordScreenState();
}

class _PasswordScreenState extends State<PasswordScreen> {
  final _passwordController = TextEditingController();
  String? _error;
  bool _loading = false;

  Future<void> _attemptUnlock() async {
    setState(() {
      _loading = true;
      _error = null;
    });

    try {
      await widget.onPasswordSubmitted(_passwordController.text);
    } catch (e) {
      setState(() {
        _error = "Incorrect password";
        _loading = false;
      });
      _passwordController.clear();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Padding(
          padding: EdgeInsets.all(32),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Icon(Icons.lock, size: 64),
              SizedBox(height: 24),
              Text(
                "App Password Required",
                style: Theme.of(context).textTheme.headlineSmall,
                textAlign: TextAlign.center,
              ),
              SizedBox(height: 32),
              if (_error != null) ...[
                Container(
                  padding: EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color: Theme.of(context).colorScheme.errorContainer,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Text(
                    _error!,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.onErrorContainer,
                    ),
                  ),
                ),
                SizedBox(height: 16),
              ],
              TextField(
                controller: _passwordController,
                obscureText: true,
                autofocus: true,
                enabled: !_loading,
                decoration: InputDecoration(
                  border: OutlineInputBorder(),
                  labelText: 'Password',
                  prefixIcon: Icon(Icons.lock_outline),
                ),
                onSubmitted: _loading ? null : (_) => _attemptUnlock(),
              ),
              SizedBox(height: 24),
              FilledButton(
                onPressed: _loading ? null : _attemptUnlock,
                child: _loading
                    ? SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : Text('Unlock'),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
