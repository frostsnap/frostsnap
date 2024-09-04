import 'package:flutter/material.dart';
import 'dart:io';
import 'dart:async';

import 'package:frostsnapp/theme.dart';

Future<T?> showDeviceActionDialog<T>({
  required BuildContext context,
  required Widget Function(BuildContext) builder,
  Future<T?>? complete,
}) async {
  var failed = false;
  BuildContext? dialogContext;

  complete?.then((result) {
    if (dialogContext != null && dialogContext!.mounted) {
      Navigator.pop(dialogContext!, result);
    }
  }).catchError((error) {
    if (!failed) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text("ERROR: $error")),
      );
      if (dialogContext != null && dialogContext!.mounted) {
        Navigator.pop(dialogContext!);
      }
    }
  });

  final result = await showModalBottomSheet<T>(
    context: context,
    isScrollControlled: true,
    isDismissible: false,
    backgroundColor: Colors.transparent,
    builder: (BuildContext dialogContext_) {
      dialogContext = dialogContext_;
      return Align(
        alignment: Alignment.bottomCenter,
        child: Stack(
          children: [
            Container(
              padding: EdgeInsets.only(right: 24, left: 24, top: 3),
              decoration: BoxDecoration(
                color: backgroundPrimaryColor,
                borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
                boxShadow: [
                  BoxShadow(
                    color: shadowColor,
                    blurRadius: 10,
                    offset: Offset(0, -2),
                  ),
                ],
              ),
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: 400.0,
                  maxHeight: MediaQuery.of(dialogContext_).size.height * 0.95,
                ),
                child: builder(dialogContext_),
              ),
            ),
            Positioned(
              top: 10,
              right: 10,
              child: IconButton(
                icon: Icon(Icons.close),
                onPressed: () {
                  if (dialogContext_.mounted) {
                    Navigator.pop(dialogContext_);
                  }
                },
              ),
            ),
          ],
        ),
      );
    },
  );

  dialogContext = null;
  if (result == null) {
    failed = true;
  }

  return result;
}

void showErrorSnackbar(BuildContext context, String errorMessage) {
  final snackBar = SnackBar(
    content: Text(
      errorMessage,
      style: TextStyle(
        color: textColor,
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

class DialogHeader extends StatelessWidget {
  final Widget child;

  const DialogHeader({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return Container(
        width: double.infinity,
        decoration: BoxDecoration(
          color: backgroundPrimaryColor,
          border: Border(
            bottom: BorderSide(
              color: backgroundSecondaryColor,
              width: 1.0,
            ),
          ),
        ),
        padding:
            EdgeInsets.only(top: 15.0, bottom: 10.0, left: 25.0, right: 25.0),
        child: DefaultTextStyle(
            style: TextStyle(
              fontSize: 18.0,
              fontWeight: FontWeight.normal,
              color: textColor,
            ),
            child: Align(alignment: Alignment.topCenter, child: child)));
  }
}
