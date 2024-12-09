import 'package:flutter/material.dart';

void showErrorSnackbarTop(BuildContext context, String errorMessage) {
  final theme = Theme.of(context);
  final snackBar = SnackBar(
    content: Text(
      errorMessage,
      style: theme.textTheme.titleMedium,
    ),
    backgroundColor: theme.colorScheme.error,
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
  final theme = Theme.of(context);
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Text(
        message,
        style: theme.textTheme.titleMedium,
      ),
      backgroundColor: theme.colorScheme.error,
      behavior: SnackBarBehavior.floating,
    ),
  );
}
