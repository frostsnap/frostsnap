import 'package:flutter/material.dart';
import 'dart:io';
import 'dart:async';

import 'package:frostsnapp/theme.dart';

Future<T?> showDeviceActionDialog<T>({
  required BuildContext context,
  required Widget Function(BuildContext) builder,
  Future<T?>? complete,
  Function()? onCancel,
}) async {
  var canceled = false;
  BuildContext? dialogContext;

  complete?.then((result) {
    if (dialogContext != null && dialogContext!.mounted) {
      Navigator.pop(dialogContext!, result);
    }
  }).catchError((error) {
    if (!canceled) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text("ERROR: $error")),
      );
      if (dialogContext != null && dialogContext!.mounted) {
        Navigator.pop(dialogContext!);
      }
    }
  });

  final result = showModalBottomSheet<T>(
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
                    color: backgroundSecondaryColor,
                    blurRadius: 10,
                    offset: Offset(0, -2),
                  ),
                ],
              ),
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: 400.0,
                  maxHeight: MediaQuery.of(context).size.height * 0.95,
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
                  Navigator.pop(context);
                },
              ),
            ),
          ],
        ),
      );
    },
  );

  result.then((value) {
    if (value == null) {
      canceled = true;
      onCancel?.call();
    }
  });

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
    duration: Duration(seconds: 3), // Adjust the duration as needed
  );

  ScaffoldMessenger.of(context).showSnackBar(snackBar);
}

class DialogHeader extends StatelessWidget {
  final Widget child; // Content of the header

  const DialogHeader({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return Container(
        width: double.infinity,
        decoration: BoxDecoration(
          color: Colors.white, // Background color of the header
          border: Border(
            bottom: BorderSide(
              color: Colors.grey, // Color of the divider
              width: 1.0, // Thickness of the divider
            ),
          ),
        ),
        padding: EdgeInsets.only(
            top: 15.0,
            bottom: 10.0,
            left: 25.0,
            right: 25.0), // Padding for the header
        child: DefaultTextStyle(
            style: TextStyle(
              fontSize: 18.0, // Default text size
              fontWeight: FontWeight.normal, // Default text weight
              color: Colors.black, // Default text color
            ), // The content passed to the header
            child: Align(alignment: Alignment.topCenter, child: child)));
  }
}
