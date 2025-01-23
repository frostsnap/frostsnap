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
  final shareRestoreStream =
      coord
          .checkShareOnDevice(
            deviceId: deviceId,
            accessStructureRef: accessStructureRef,
          )
          .asBroadcastStream();

  final aborted = shareRestoreStream
      .firstWhere((state) => state.abort != null)
      .then((state) {
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
        stream: shareRestoreStream,
        builder: (context, snapshot) {
          final outcome = snapshot.data?.outcome;
          return Column(
            children: [
              DialogHeader(child: Text("Enter the backup on the device.")),
              Expanded(
                child: DeviceListWithIcons(
                  iconAssigner: (context, deviceId) {
                    if (deviceIdEquals(deviceId, deviceId)) {
                      const icon = DevicePrompt(
                        icon: Icon(Icons.keyboard),
                        text: "",
                      );
                      return (null, icon);
                    } else {
                      return (null, null);
                    }
                  },
                ),
              ),
              DialogFooter(
                child: ElevatedButton(
                  onPressed: () {
                    Navigator.pop(context, outcome);
                  },
                  style: ElevatedButton.styleFrom(
                    backgroundColor: switch (outcome) {
                      true => Colors.green,
                      false => Colors.red,
                      null => null,
                    },
                  ),
                  child: Text(switch (outcome) {
                    true => "Your backup is valid. Done!",
                    false => "Your backup is invalid. Display again.",
                    null => "Cancel",
                  }),
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
