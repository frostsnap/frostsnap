import 'package:flutter/material.dart';

const Color textColor = Color.fromARGB(255, 228, 235, 236);
const Color textSecondaryColor = Color.fromARGB(255, 179, 204, 211);
const Color successColor = Color.fromARGB(255, 21, 255, 0);
const Color awaitingColor = Color.fromARGB(255, 207, 124, 15);
const Color uninterestedColor = Color.fromARGB(255, 167, 160, 160);
const Color errorColor = Color.fromARGB(255, 172, 23, 23);
const Color shadowColor = Colors.black26;
const Color backgroundPrimaryColor = Color.fromARGB(255, 60, 107, 134);
const Color backgroundSecondaryColor = Color.fromARGB(255, 35, 66, 83);
const Color backgroundTertiaryColor = Color.fromARGB(255, 1, 60, 87);

const MaterialColor primarySwatch = MaterialColor(
  0xFF3F51B5,
  <int, Color>{
    50: backgroundSecondaryColor,
    900: backgroundPrimaryColor,
  },
);

const TextStyle defaultTextStyle = TextStyle(
  color: textColor,
);

final ThemeData frostsnappTheme = ThemeData(
  scaffoldBackgroundColor: backgroundPrimaryColor,
  inputDecorationTheme: InputDecorationTheme(
      border: OutlineInputBorder(),
      enabledBorder: OutlineInputBorder(
        borderSide: BorderSide(color: backgroundSecondaryColor),
      ),
      focusedBorder: OutlineInputBorder(
        borderSide: BorderSide(color: backgroundTertiaryColor),
      ),
      iconColor: backgroundTertiaryColor,
      labelStyle: defaultTextStyle,
      hintStyle: TextStyle(
        color: textSecondaryColor,
      )),
  iconTheme: IconThemeData(color: textSecondaryColor),
  checkboxTheme: CheckboxThemeData(
    checkColor:
        WidgetStateProperty.resolveWith<Color>((Set<WidgetState> states) {
      if (states.contains(WidgetState.selected)) {
        return textColor;
      }
      return textColor;
    }),
    fillColor:
        WidgetStateProperty.resolveWith<Color>((Set<WidgetState> states) {
      if (states.contains(WidgetState.selected)) {
        return backgroundTertiaryColor;
      }
      return backgroundTertiaryColor;
    }),
  ),
  appBarTheme: AppBarTheme(
      backgroundColor: backgroundSecondaryColor,
      foregroundColor: textColor,
      shadowColor: shadowColor,
      elevation: 6.0,
      surfaceTintColor: Colors.white),
  snackBarTheme: SnackBarThemeData(contentTextStyle: defaultTextStyle),
  textTheme: TextTheme(
    // Headline Styles
    headlineLarge: TextStyle(
      color: textColor,
      fontSize: 32.0,
      fontWeight: FontWeight.bold,
      height: 1.3,
    ),
    headlineMedium: TextStyle(
      color: textColor,
      fontSize: 28.0,
      fontWeight: FontWeight.bold,
      height: 1.3,
    ),
    headlineSmall: TextStyle(
      color: textColor,
      fontSize: 24.0,
      fontWeight: FontWeight.bold,
      height: 1.3,
    ),

    // Title Styles
    titleLarge: TextStyle(
      color: textColor,
      fontSize: 30.0,
      fontWeight: FontWeight.bold,
      height: 1.25,
    ),
    titleMedium: TextStyle(
      color: textColor,
      fontSize: 24.0,
      fontWeight: FontWeight.bold,
      height: 1.25,
    ),
    titleSmall: TextStyle(
      color: textColor,
      fontSize: 18.0,
      fontWeight: FontWeight.bold,
      height: 1.25,
    ),

    // Existing Body Styles
    bodyLarge: TextStyle(
      color: textColor,
      fontSize: 18.0,
      fontWeight: FontWeight.normal,
      height: 1.4,
    ),
    bodyMedium: TextStyle(
      color: textColor,
      fontSize: 16.0,
      fontWeight: FontWeight.normal,
      height: 1.4,
    ),
    bodySmall: TextStyle(
      color: textColor,
      fontSize: 14.0,
      fontWeight: FontWeight.normal,
      height: 1.4,
    ),

    // Existing Label Styles
    labelLarge: TextStyle(
      color: textColor,
      fontSize: 16.0,
      fontWeight: FontWeight.bold,
      height: 1.2,
    ),
    labelMedium: TextStyle(
      color: textColor,
      fontSize: 14.0,
      fontWeight: FontWeight.bold,
      height: 1.2,
    ),
    labelSmall: TextStyle(
      color: textColor,
      fontSize: 12.0,
      fontWeight: FontWeight.bold,
      height: 1.2,
    ),
  ),
  colorScheme: ColorScheme.fromSwatch(
    primarySwatch: primarySwatch,
    backgroundColor: backgroundPrimaryColor,
    errorColor: errorColor,
  ).copyWith(
    secondary: backgroundSecondaryColor,
  ),
  textButtonTheme: TextButtonThemeData(
    style: TextButton.styleFrom(
        backgroundColor: backgroundTertiaryColor, foregroundColor: textColor),
  ),
  elevatedButtonTheme: ElevatedButtonThemeData(
    style: ElevatedButton.styleFrom(
      backgroundColor: backgroundTertiaryColor,
      foregroundColor: textSecondaryColor,
      disabledBackgroundColor: uninterestedColor,
      // disabledForegroundColor: uninterestedColor
    ),
  ),
  sliderTheme: SliderThemeData(
      activeTrackColor: backgroundTertiaryColor,
      inactiveTrackColor: backgroundTertiaryColor,
      disabledActiveTrackColor: backgroundSecondaryColor,
      disabledInactiveTrackColor: backgroundSecondaryColor),
  outlinedButtonTheme: OutlinedButtonThemeData(
    style: OutlinedButton.styleFrom(
      backgroundColor: backgroundTertiaryColor,
      side: BorderSide(color: backgroundSecondaryColor),
    ),
  ),
  bottomNavigationBarTheme: BottomNavigationBarThemeData(
      selectedItemColor: textColor,
      unselectedItemColor: textSecondaryColor,
      backgroundColor: backgroundPrimaryColor),
  listTileTheme: ListTileThemeData(
    tileColor: backgroundSecondaryColor,
    textColor: textColor,
    shape: RoundedRectangleBorder(
      borderRadius: BorderRadius.circular(8.0), // Adjust the radius as needed
    ),
    minVerticalPadding: 5,
  ),
  dropdownMenuTheme:
      DropdownMenuThemeData(textStyle: TextStyle(color: textColor)),
  dividerTheme: const DividerThemeData(color: Colors.black12),
  dialogTheme: DialogTheme(
    backgroundColor: backgroundSecondaryColor,
    iconColor: backgroundTertiaryColor,
    shadowColor: shadowColor,
  ),
  switchTheme: SwitchThemeData(
    thumbColor: WidgetStateProperty.resolveWith<Color>((states) {
      if (states.contains(WidgetState.disabled)) {
        return backgroundSecondaryColor; // Disabled track color
      }
      if (states.contains(WidgetState.selected)) {
        return Colors.green; // Active thumb color
      }
      return Colors.grey.shade200; // Inactive thumb color
    }),
    trackColor: WidgetStateProperty.resolveWith<Color>((states) {
      if (states.contains(WidgetState.disabled)) {
        return backgroundTertiaryColor; // Disabled track color
      }
      if (states.contains(WidgetState.selected)) {
        return Colors.green.shade300; // Active track color
      }
      return Colors.grey.shade500; // Inactive track color
    }),
    trackOutlineColor: WidgetStateProperty.all(Colors.transparent),
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
