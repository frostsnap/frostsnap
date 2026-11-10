import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/theme.dart';
import 'package:google_fonts/google_fonts.dart';

// The app's real fonts and theme for widget tests that measure or draw text,
// following the approach of card_screens_shots_test.dart. Without them text
// is drawn in the test font, whose glyphs are all 1 em squares.

/// The Flutter SDK's bundled Roboto and Material Icons. flutter_tester lives
/// in `<flutter>/bin/cache/artifacts/engine/<platform>/`.
String _materialFontsDir() {
  final root = Platform.environment['FLUTTER_ROOT'];
  if (root != null) {
    final dir = '$root/bin/cache/artifacts/material_fonts';
    if (Directory(dir).existsSync()) return dir;
  }
  final artifacts = File(Platform.resolvedExecutable).parent.parent.parent;
  return '${artifacts.path}/material_fonts';
}

Future<void> _loadFont(String family, List<String> files) async {
  final loader = FontLoader(family);
  for (final file in files) {
    final bytes = File(file).readAsBytesSync();
    loader.addFont(Future.value(ByteData.sublistView(bytes)));
  }
  await loader.load();
}

/// Registers the app's fonts under the family names google_fonts uses
/// (`NotoSans_regular`, `NotoSansMono_700`, ...), one file per family, as
/// google_fonts itself does in the app. Call from `setUpAll`.
Future<void> loadAppFonts() async {
  GoogleFonts.config.allowRuntimeFetching = false;
  const g = 'assets/google_fonts';
  final m = _materialFontsDir();
  final families = <String, List<String>>{
    'NotoSans_regular': ['$g/NotoSans-Regular.ttf'],
    'NotoSans_500': ['$g/NotoSans-Medium.ttf'],
    'NotoSans_600': ['$g/NotoSans-SemiBold.ttf'],
    'NotoSans_700': ['$g/NotoSans-Bold.ttf'],
    'NotoSansMono_regular': ['$g/NotoSansMono-Regular.ttf'],
    'NotoSansMono_700': ['$g/NotoSansMono-Bold.ttf'],
    'Roboto': [
      for (final w in ['Light', 'Regular', 'Medium', 'Bold', 'Black'])
        '$m/Roboto-$w.ttf',
    ],
    'MaterialIcons': ['$m/MaterialIcons-Regular.otf'],
  };
  for (final MapEntry(:key, :value) in families.entries) {
    await _loadFont(key, value);
  }
}

/// main.dart's theme, exactly.
ThemeData appTheme() {
  final colorScheme = ColorScheme.fromSeed(
    brightness: Brightness.dark,
    seedColor: seedColor,
  );
  final baseTheme = ThemeData(useMaterial3: true, colorScheme: colorScheme);
  final textTheme = GoogleFonts.notoSansTextTheme(baseTheme.textTheme);
  return baseTheme.copyWith(colorScheme: colorScheme, textTheme: textTheme);
}
