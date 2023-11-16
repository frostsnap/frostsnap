import 'package:flutter/material.dart';
import 'dart:io';

Future<T?> showDeviceActionDialog<T>({
  required BuildContext context,
  required Widget content,
  required Widget title,
  required Future<T?> complete,
  Function()? onCancel,
}) async {
  return showDialog<T>(
      barrierDismissible: false,
      context: context,
      builder: (dialogContext) {
        complete.then((result) {
          if (Navigator.of(dialogContext).canPop()) {
            Navigator.pop(dialogContext, result);
          }
        });
        return AlertDialog(
            title: title,
            content: Container(
                width: Platform.isAndroid ? double.maxFinite : 400.0,
                // this align thing is necessary to stop the child from expanding beyond its BoxConstraints
                child: Align(alignment: Alignment.center, child: content)),
            actions: [
              ElevatedButton(
                  onPressed: () {
                    Navigator.pop(dialogContext);
                    onCancel?.call();
                  },
                  child: const Text("Cancel"))
            ]);
      });
}
