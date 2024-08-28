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
      return DraggableScrollableSheet(
        initialChildSize: 0.9,
        minChildSize: 0.9,
        maxChildSize: 0.9,
        builder: (BuildContext context, ScrollController scrollController) {
          return Center(
              child: Stack(
            children: [
              Container(
                padding: EdgeInsets.all(20),
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
                      // you've got to make even an empty content have some
                      // height otherwise WEIRD things happen.
                      minHeight: MediaQuery.of(context).size.height * 0.9),
                  child: SingleChildScrollView(
                    controller: scrollController,
                    child: builder(dialogContext_),
                  ),
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
          ));
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
        color: textColor,
        fontSize: 16.0,
      ),
    ),
    backgroundColor: errorColor,
    duration: Duration(seconds: 3), // Adjust the duration as needed
  );

  ScaffoldMessenger.of(context).showSnackBar(snackBar);
}
