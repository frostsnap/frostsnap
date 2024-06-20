import 'package:flutter/material.dart';
import 'dart:io';
import 'dart:async';

Future<T?> showDeviceActionDialog<T>({
  required BuildContext context,
  required Widget content,
  Future<T?>? complete,
  Function()? onCancel,
}) async {
  var canceled = false;

  return showDialog<T>(
      barrierDismissible: false,
      context: context,
      builder: (dialogContext) {
        complete?.then((result) {
          if (dialogContext.mounted) {
            Navigator.pop(dialogContext, result);
          }
        }).catchError((error) {
          if (!canceled) {
            showErrorSnackbar(context, "ERROR: $error");
            if (dialogContext.mounted) {
              Navigator.pop(dialogContext);
            }
          }
        });
        final dialog =
            AlertDialog(content: DialogContainer(child: content), actions: [
          if (onCancel != null)
            ElevatedButton(
                onPressed: () {
                  canceled = true;
                  onCancel.call();
                  Navigator.pop(dialogContext);
                },
                child: const Text("Cancel"))
        ]);
        return dialog;
      });
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

class DialogContainer extends StatelessWidget {
  final Widget child;

  const DialogContainer({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    return Container(
        width: Platform.isAndroid ? double.maxFinite : 400.0,
        // this align thing is necessary to stop the child from expanding beyond its BoxConstraints
        child: Align(alignment: Alignment.center, child: this.child));
  }
}
