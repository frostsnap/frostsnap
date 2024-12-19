import 'package:flutter/material.dart';

const Color successColor = Color.fromARGB(255, 21, 255, 0);
const Color awaitingColor = Color.fromARGB(255, 207, 124, 15);
const Color uninterestedColor = Color.fromARGB(255, 167, 160, 160);
const Color errorColor = Color.fromARGB(255, 172, 23, 23);
const Color shadowColor = Colors.black26;

const Color backgroundPrimaryColor = Color(0xFF0D121C);
const Color backgroundSecondaryColor = Color(0xFF1D2939);
const Color backgroundTertiaryColor = Color(0xFF202939);
const Color backgroundErrorColor = Color(0xFFF97066);

const Color textPrimaryColor = Colors.white;
const Color textSecondaryColor = Color(0xFF9AA4B2);
const Color textTertiaryColor = Color(0xFF202939);
const Color textErrorColor = Color(0xFFFCFCFD);

const TextStyle defaultTextStyle = TextStyle(
  color: textPrimaryColor,
);

final RoundedRectangleBorder squircle =
    RoundedRectangleBorder(borderRadius: BorderRadius.circular(16.0));

const EdgeInsets buttonPadding =
    EdgeInsets.symmetric(vertical: 12.0, horizontal: 16.0);

final ThemeData frostsnappTheme = ThemeData(
  useMaterial3: true,
  colorScheme: ColorScheme.dark(
      primary: Color(0xFF0E9384), // #0E9384
      onPrimary: Colors.white, // #FFFFFF
      secondary: Color(0xFF121926), // #121926
      onSecondary: Colors.white, // #FFFFFF
      tertiary: Color(0xFF202939), // #202939
      onTertiary: Color(0xFFFCFCFD), // #FCFCFD
      error: Color(0xFFF97066),
      onError: Color(0xFFFCFCFD),
      surface: Color(0xFF0D121C),
      onSurface: Color(0xFFFCFCFD)),
  filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(shape: squircle, padding: buttonPadding)),
  elevatedButtonTheme: ElevatedButtonThemeData(
      style: ElevatedButton.styleFrom(shape: squircle, padding: buttonPadding)),
  iconButtonTheme: IconButtonThemeData(
      style: IconButton.styleFrom(shape: squircle, padding: buttonPadding)),
  floatingActionButtonTheme: FloatingActionButtonThemeData(
    shape: squircle,
  ),
  dividerTheme: DividerThemeData(
    thickness: 1.0,
    space: 0.0,
    indent: 24.0,
    endIndent: 24.0,
    color: Color(0xFF364152),
  ),
);

class FsProgressIndicator extends StatelessWidget {
  const FsProgressIndicator({super.key});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
        height: 30.0,
        child: AspectRatio(
            aspectRatio: 1,
            child: CircularProgressIndicator.adaptive(
              valueColor: AlwaysStoppedAnimation<Color>(textSecondaryColor),
            )));
  }
}
