import 'package:flutter/material.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list_widget.dart';
import 'dart:developer' as developer;

import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';

class DeviceSetup extends StatelessWidget {
  const DeviceSetup(
      {super.key,
      required this.deviceId,
      this.onSubmitted,
      this.onChanged,
      this.onCancel});

  final DeviceId deviceId;
  final ValueChanged<String>? onSubmitted;
  final ValueChanged<String>? onChanged;
  final Function()? onCancel;

  @override
  Widget build(BuildContext context) {
    bool submitted = false;
    return PopScope(
        onPopInvoked: (didPop) {
          if (!submitted) {
            onCancel?.call();
          }
        },
        child: Scaffold(
          appBar: AppBar(
            title: const Text('Device Setup'),
          ),
          body: Column(
            children: [
              TextField(
                decoration: const InputDecoration(
                  icon: Icon(Icons.person),
                  hintText: 'What do you want name this device?',
                  labelText: 'Name',
                ),
                onSubmitted: (name) {
                  submitted = true;
                  onSubmitted?.call(name);
                },
                onChanged: onChanged,
              ),
            ],
          ),
        ));
  }
}

Future<void> handleDeviceRenaming(BuildContext context, Device device) async {
  coord.updateNamePreview(id: device.id, name: "");
  Navigator.push(context, MaterialPageRoute(builder: (deviceSetupContex) {
    final completeWhen = deviceListChangeStream
        .firstWhere((change) =>
            change.kind == DeviceListChangeKind.Named &&
            deviceIdEquals(device.id, change.device.id))
        .whenComplete(() {
      if (deviceSetupContex.mounted) {
        Navigator.pop(deviceSetupContex);
      }
    });
    return DeviceSetup(
      deviceId: device.id,
      onCancel: () {
        coord.sendCancel(id: device.id);
      },
      onSubmitted: (value) async {
        coord.finishNaming(id: device.id, name: value);
        await showDeviceActionDialog(
            context: deviceSetupContex,
            content: Column(children: [
              Text("Confirm name '$value' on device"),
              Divider(),
              MaybeExpandedVertical(child: DeviceListContainer(
                  child: DeviceListWithIcons(iconAssigner: (context, deviceId) {
                if (deviceIdEquals(deviceId, device.id)) {
                  final label = LabeledDeviceText("'$value'?");
                  const icon =
                      const Row(mainAxisSize: MainAxisSize.min, children: [
                    Icon(Icons.visibility, color: Colors.orange),
                    SizedBox(width: 4),
                    Text("Confirm"),
                  ]);
                  return (label, icon);
                } else {
                  return (null, null);
                }
              })))
            ]),
            complete: completeWhen,
            onCancel: () async {
              await coord.sendCancel(id: device.id);
            });
      },
      onChanged: (value) async {
        await coord.updateNamePreview(id: device.id, name: value);
      },
    );
  }));
}

Future<void> _renameDeviceDialog(BuildContext context, Device device,
    String newName, Future<void> completeWhen) async {
  await showDeviceActionDialog(
    context: context,
    content: Column(
      children: [
        Text("Confirm name '$newName' on device"),
        Divider(),
        MaybeExpandedVertical(
          child: DeviceListContainer(
            child: DeviceListWithIcons(
              iconAssigner: (context, deviceId) {
                if (deviceIdEquals(deviceId, device.id)) {
                  final label = LabeledDeviceText("'$newName'?");
                  final icon = const Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(Icons.visibility, color: Colors.orange),
                      SizedBox(width: 4),
                      Text("Confirm"),
                    ],
                  );
                  return (label, icon);
                } else {
                  return (null, null);
                }
              },
            ),
          ),
        ),
      ],
    ),
    complete: completeWhen,
    onCancel: () async {
      await coord.sendCancel(id: device.id);
    },
  );
}
