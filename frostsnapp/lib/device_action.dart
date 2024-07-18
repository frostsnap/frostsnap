import 'package:flutter/material.dart';
import 'dart:io';
import 'dart:async';

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
      return DraggableScrollableSheet(
        initialChildSize: 0.9,
        minChildSize: 0.9,
        maxChildSize: 0.9,
        builder: (BuildContext context, ScrollController scrollController) {
          return Center(
              child: ConstrainedBox(
                  constraints: BoxConstraints(
                    maxWidth: 400.0,
                  ),
                  child: Stack(children: [
                    Container(
                      margin: EdgeInsets.only(
                          top: 50), // Adjust this for the small space above
                      padding: EdgeInsets.all(20),
                      decoration: BoxDecoration(
                        color: Colors.white,
                        borderRadius: BorderRadius.only(
                          topLeft: Radius.circular(20),
                          topRight: Radius.circular(20),
                        ),
                        boxShadow: [
                          BoxShadow(
                            color: Colors.black26,
                            blurRadius: 10,
                            offset: Offset(0, -2),
                          ),
                        ],
                      ),
                      child: Align(
                          alignment: Alignment.center,
                          child: builder(dialogContext_)),
                    ),
                    Positioned(
                      top: 55,
                      right: 10,
                      child: IconButton.outlined(
                        icon: Icon(Icons.close),
                        onPressed: () {
                          canceled = true;
                          Navigator.pop(context);
                        },
                      ),
                    ),
                  ])));
        },
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
        color: Colors.white, // White text color
        fontSize: 16.0,
      ),
    ),
    backgroundColor: Colors.red, // Red background color
    duration: Duration(seconds: 3), // Adjust the duration as needed
  );

  ScaffoldMessenger.of(context).showSnackBar(snackBar);
}
