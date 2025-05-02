import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/id_ext.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/snackbar.dart';

Future<bool> backupDeviceDialog(
  BuildContext context, {
  required DeviceId deviceId,
  required AccessStructure accessStructure,
}) async {
  Future<bool> displayBackupOnDevice() async {
    final displayStream =
        coord
            .displayBackup(
              id: deviceId,
              accessStructureRef: accessStructure.accessStructureRef(),
            )
            .asBroadcastStream();
    final deviceName = coord.getDeviceName(id: deviceId);
    final confirmed = await showDeviceActionDialog<bool>(
      context: context,
      complete: displayStream.first,
      builder: (context) {
        return Column(
          children: [
            DialogHeader(
              child: Text("Connect $deviceName to display its backup"),
            ),
            Expanded(
              child: DeviceListWithIcons(
                iconAssigner: (context, id) {
                  if (deviceIdEquals(deviceId, id)) {
                    final Widget icon = ConfirmPrompt();
                    return (null, icon);
                  } else {
                    return (null, null);
                  }
                },
              ),
            ),
          ],
        );
      },
    );
    if (confirmed != true) {
      await coord.cancelProtocol();
      return false;
    }
    return true;
  }

  if (!await displayBackupOnDevice()) {
    return false;
  }

  if (context.mounted) {
    final result = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) {
        return AlertDialog(
          titlePadding: const EdgeInsets.fromLTRB(24, 16, 16, 0),
          title: Row(
            children: [
              const Expanded(child: Text('Write Down Your Backup')),
              IconButton(
                icon: const Icon(Icons.close),
                onPressed: () {
                  Navigator.pop(context, false);
                },
              ),
            ],
          ),
          content: const Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Write down the backup shown on the device.'),
              SizedBox(height: 8),
              Text(
                'Then store it alongside your device in this secure location.',
              ),
            ],
          ),
          actions: [
            FilledButton(
              onPressed: () {
                Navigator.pop(context, true);
              },
              child: const Text('Backup & Device Secured'),
            ),
          ],
        );
      },
    );

    await coord.cancelProtocol();
    return result ?? false;
  }

  await coord.cancelProtocol();
  return false;
}

Future<bool?> verifyBackup(
  BuildContext context,
  DeviceId deviceId,
  AccessStructureRef accessStructureRef,
) async {
  final backupEntry =
      coord
          .tellDeviceToEnterPhysicalBackup(deviceId: deviceId)
          .asBroadcastStream();

  final aborted = backupEntry.firstWhere((state) => state.abort != null).then((
    state,
  ) {
    if (context.mounted) {
      showErrorSnackbarBottom(context, state.abort!);
    }
    return null;
  });

  final result = await showDeviceActionDialog<bool>(
    context: context,
    complete: aborted,
    builder: (BuildContext context) {
      return StreamBuilder(
        stream: backupEntry,
        builder: (context, snapshot) {
          final entered = snapshot.data?.entered;

          if (entered != null) {
            bool isExpected = coord.checkPhysicalBackupIsExpected(
              accessStructureRef: accessStructureRef,
              phase: entered,
              deviceId: deviceId,
            );
            bool isValid =
                isExpected ||
                coord.checkPhysicalBackup(
                  accessStructureRef: accessStructureRef,
                  phase: entered,
                );

            Future.microtask(() async {
              if (context.mounted) {
                await showVerifyBackupResult(context, isExpected, isValid);
              }
            });
            if (context.mounted) {
              Navigator.of(context).pop();
            }
          }
          return Column(
            children: [
              DialogHeader(child: Text("Enter the backup on the device.")),
              Expanded(
                child: DeviceListWithIcons(
                  iconAssigner: (context, deviceId) {
                    if (deviceIdEquals(deviceId, deviceId)) {
                      return (
                        null,
                        DevicePrompt(icon: Icon(Icons.keyboard), text: ""),
                      );
                    }
                    return (null, null);
                  },
                ),
              ),
            ],
          );
        },
      );
    },
  );

  if (result == null) {
    await coord.cancelProtocol();
  }

  return result;
}

Future<void> showVerifyBackupResult(
  BuildContext context,
  bool isExpected,
  bool isValid,
) async {
  IconData icon;
  String title;
  String content;

  if (isExpected) {
    icon = Icons.check_circle_outline;
    title = "Valid Backup";
    content =
        "This backup belongs to this wallet and is associated with this device.";
  } else if (isValid) {
    icon = Icons.warning;
    title = "Valid but Unexpected Backup";
    content =
        "This backup is valid and belongs to this wallet, but is associated with a different device to the one you entered it on.";
  } else {
    icon = Icons.error;
    title = "Unrelated Backup";
    content =
        "The backup you have entered is not valid for this wallet. This backup belongs to a different wallet.";
  }

  showDialog(
    context: context,
    barrierDismissible: false,
    builder: (BuildContext context) {
      return BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 5, sigmaY: 5),
        child: AlertDialog(
          title: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Flexible(
                child: Text(
                  title,
                  style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                ),
              ),
              Icon(icon, size: 40),
            ],
          ),
          content: Text(content),
          actions: [
            FilledButton(
              onPressed: () {
                Navigator.of(context).pop();
              },
              child: Text("OK"),
            ),
          ],
        ),
      );
    },
  );
}

class BackupInstructions extends StatelessWidget {
  final AccessStructure accessStructure;

  const BackupInstructions({super.key, required this.accessStructure});

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          "Your device has presented a backup, write it down on your Frostsnap backup card.",
        ),
        const SizedBox(height: 16),
        Text(
          "Any ${accessStructure.threshold()} of these backups will provide complete control over this key.",
        ),
        const SizedBox(height: 16),
        const Text(
          "You must store these backups securely in separate locations.",
        ),
      ],
    );
  }
}
