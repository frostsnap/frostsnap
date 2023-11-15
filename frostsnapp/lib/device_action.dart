import 'dart:collection';
import 'package:flutter/material.dart';
import 'ffi.dart';

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
            content: Container(width: double.maxFinite, child: content),
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
