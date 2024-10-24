import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/theme.dart';
import 'package:frostsnapp/hex.dart';

Future<void> doBackupWorkflow(BuildContext context,
    {required List<DeviceId> devices,
    required AccessStructure accessStructure}) async {
  for (final deviceId in devices) {
    if (context.mounted) {
      final confirmed = await showDeviceBackupDialog(context,
          deviceId: deviceId,
          accessStructureRef: accessStructure.accessStructureRef());

      if (confirmed && context.mounted) {
        await showBackupInstructionsDialog(context,
            accessStructure: accessStructure);
      }
    }
    await coord.cancelProtocol();
  }
}

Future<bool> showDeviceBackupDialog(BuildContext context,
    {required DeviceId deviceId,
    required AccessStructureRef accessStructureRef}) async {
  final result = await showDeviceActionDialog<bool>(
    context: context,
    complete: coord
        .displayBackup(id: deviceId, accessStructureRef: accessStructureRef)
        .first,
    builder: (context) {
      return Column(children: [
        DialogHeader(child: Text("Confirm on device to show backup")),
        DeviceListWithIcons(
          iconAssigner: (context, id) {
            if (deviceIdEquals(deviceId, id)) {
              final Widget icon = ConfirmPrompt();
              return (null, icon);
            } else {
              return (null, null);
            }
          },
        ),
      ]);
    },
  );

  final confirmed = result == true;

  return confirmed;
}

Future<void> showBackupInstructionsDialog(
  BuildContext context, {
  required AccessStructure accessStructure,
}) async {
  return showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
          actions: [
            ElevatedButton(
              child: Text("I have written down my backup"),
              onPressed: () {
                Navigator.pop(context);
              },
            ),
          ],
          content: SizedBox(
            width: Platform.isAndroid ? double.maxFinite : 400.0,
            child: Align(
              alignment: Alignment.center,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text.rich(TextSpan(
                      text:
                          "Write down each device's backup for this key onto separate pieces of paper. Each piece of paper should look like this with every ",
                      children: const [
                        TextSpan(
                          text: 'X',
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        TextSpan(
                          text: ' replaced with the character shown on screen.',
                        )
                      ])),
                  SizedBox(height: 8),
                  Divider(),
                  Center(
                    child: Text.rich(TextSpan(
                      text: 'frost[',
                      children: const <TextSpan>[
                        TextSpan(
                          text: 'X',
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        TextSpan(
                          text: ']',
                        ),
                      ],
                      style: TextStyle(
                          fontFamily: 'Courier',
                          color: textSecondaryColor,
                          fontSize: 20),
                    )),
                  ),
                  Center(
                    child: Text(
                      "xxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xx",
                      style: TextStyle(
                          fontFamily: 'Courier',
                          fontSize: 20,
                          fontWeight: FontWeight.bold,
                          color: textSecondaryColor),
                    ),
                  ),
                  Center(
                      child: Text(
                    "Identifier: ${toSpacedHex(Uint8List.fromList(accessStructure.id().field0.sublist(0, 4)))}",
                    style: TextStyle(fontFamily: 'Courier', fontSize: 18),
                  )),
                  Divider(),
                  SizedBox(height: 16),
                  Text(
                      "Alongside each backup, also record the identifier above."),
                  SizedBox(height: 8),
                  Text(
                      "This identifier is useful for knowing that these share backups belong to the same key and are compatibile."),
                  SizedBox(height: 24),
                  Text(
                      "Any ${accessStructure.threshold()} of these backups will provide complete control over this key."),
                  SizedBox(height: 8),
                  Text(
                      "You should store these backups securely in separate locations."),
                ],
              ),
            ),
          ));
    },
  );
}
