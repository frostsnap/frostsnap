import 'package:flutter/material.dart';
import 'package:frostsnapp/theme.dart';

void showErrorSnackbarTop(BuildContext context, String errorMessage) {
  final snackBar = SnackBar(
    content: Text(
      errorMessage,
      style: TextStyle(
        fontSize: 16.0,
      ),
    ),
    backgroundColor: errorColor,
    dismissDirection: DismissDirection.up,
    duration: Duration(seconds: 3), // Adjust the duration as needed
    behavior: SnackBarBehavior.floating, // Make the SnackBar float
    margin: EdgeInsets.only(
      bottom: MediaQuery.of(context).size.height - 120,
      left: 30.0,
      right: 30.0,
    ),
  );

  ScaffoldMessenger.of(context).showSnackBar(snackBar);
}

void showErrorSnackbarBottom(BuildContext context, String message) {
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Text(
        message,
        style: TextStyle(fontSize: 16.0),
      ),
      backgroundColor: errorColor,
      behavior: SnackBarBehavior.floating,
    ),
  );
}
