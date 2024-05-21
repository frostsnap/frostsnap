import 'package:flutter/material.dart';

const Color textColor = Color.fromARGB(255, 100, 197, 241);
const Color successColor = Color.fromARGB(255, 21, 255, 0);
const Color awaitingColor = Color.fromARGB(255, 251, 255, 0);
const Color uninterestedColor = Color.fromARGB(255, 88, 88, 88);
const Color errorColor = Color.fromARGB(255, 172, 23, 23);
const Color backgroundPrimaryColor = Color.fromARGB(255, 4, 8, 26);
const Color backgroundSecondaryColor = Color.fromARGB(255, 0, 20, 43);
const Color backgroundTertiaryColor = Color.fromARGB(255, 41, 69, 95);

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
  inputDecorationTheme: InputDecorationTheme(
      iconColor: backgroundTertiaryColor,
      labelStyle: defaultTextStyle,
      hintStyle: TextStyle(
        color: uninterestedColor,
      )),
  iconTheme: IconThemeData(color: backgroundTertiaryColor),
  checkboxTheme: CheckboxThemeData(
    checkColor:
        MaterialStateProperty.resolveWith<Color>((Set<MaterialState> states) {
      if (states.contains(MaterialState.selected)) {
        return textColor;
      }
      return textColor;
    }),
    fillColor:
        MaterialStateProperty.resolveWith<Color>((Set<MaterialState> states) {
      if (states.contains(MaterialState.selected)) {
        return backgroundTertiaryColor;
      }
      return backgroundTertiaryColor;
    }),
  ),
  appBarTheme: AppBarTheme(
      backgroundColor: backgroundSecondaryColor, foregroundColor: textColor),
  snackBarTheme: SnackBarThemeData(contentTextStyle: defaultTextStyle),
  textTheme: TextTheme(
    bodyLarge: defaultTextStyle,
    bodyMedium: defaultTextStyle,
    bodySmall: defaultTextStyle,
    labelLarge: defaultTextStyle,
    labelMedium: defaultTextStyle,
    labelSmall: defaultTextStyle,
  ),
  colorScheme: ColorScheme.fromSwatch(
    primarySwatch: primarySwatch,
    backgroundColor: backgroundPrimaryColor,
    errorColor: errorColor,
  ).copyWith(
    secondary: backgroundSecondaryColor,
  ),
  textButtonTheme: TextButtonThemeData(
    style: TextButton.styleFrom(backgroundColor: backgroundTertiaryColor),
  ),
  elevatedButtonTheme: ElevatedButtonThemeData(
    style: ElevatedButton.styleFrom(
      backgroundColor: backgroundTertiaryColor,
      foregroundColor: textColor,
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
);
