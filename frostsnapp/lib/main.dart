import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator_keygen.dart';
import 'package:frostsnapp/wallet.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:async';
import 'dart:io';
import 'device_list.dart';

Timer? timer;

void main() {
  runApp(const MyApp());
}

final Map<String, WidgetBuilder> routes = {
  '/home': (context) => MyHomePage(title: 'Frostsnapp'),
  '/keygen': (context) {
    final threshold = ModalRoute.of(context)?.settings.arguments as int?;
    return DoKeyGenScreen(threshold: threshold ?? 1); // default threshold
  },
  '/wallet': (context) {
    final publicKey = ModalRoute.of(context)?.settings.arguments as String?;
    return KeyDisplayPage(publicKey: publicKey ?? "missing");
  },
};

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
      onGenerateRoute: (settings) {
        if (routes.containsKey(settings.name)) {
          return MaterialPageRoute(
            builder: routes[settings.name]!,
            settings: settings,
          );
        }
        return null;
      },
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
  @override
  void initState() {
    super.initState();
    if (Platform.isAndroid) {
      api.turnLogcatLoggingOn(level: Level.Debug);
    } else {
      api.turnStderrLoggingOn(level: Level.Debug);
    }
  }

  @override
  Widget build(BuildContext context) {
    // This method is rerun every time setState is called.
    //
    // The Flutter framework has been optimized to make rerunning build methods
    // fast, so that you can just rebuild anything that needs updating rather
    // than having to individually change instances of widgets.
    return Scaffold(
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
              future: Future.wait([]),
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

                return OrientationBuilder(builder: (context, orientation) {
                  var effectiveOrientation =
                      Platform.isAndroid ? orientation : Orientation.portrait;
                  return CommonLayout(
                      child:
                          DeviceListWidget(orientation: effectiveOrientation));
                });
              })),
    );
  }
}

class CommonLayout extends StatelessWidget {
  final Widget child;

  CommonLayout({
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    return OrientationBuilder(builder: (context, orientation) {
      var effectiveOrientation =
          Platform.isAndroid ? orientation : Orientation.portrait;
      return Container(
        alignment: Alignment.centerRight,
        constraints: BoxConstraints.expand(
            height: effectiveOrientation == Orientation.landscape ? 120 : null,
            width: effectiveOrientation == Orientation.portrait ? 300 : null),
        child: child,
      );
    });
  }
}
