import 'package:flutter/material.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';

import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';

class DeviceSetup extends StatelessWidget {
  final DeviceId id;

  const DeviceSetup({super.key, required this.id});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(
          title: const Text('Device Setup'),
        ),
        body: Column(
          children: [
            DeviceNameField(
                id: id,
                onNamed: (_) {
                  if (context.mounted) {
                    Navigator.pop(context);
                  }
                }),
          ],
        ));
  }
}

class DeviceNameField extends StatefulWidget {
  final DeviceId id;
  final String? existingName;
  final Function(String)? onNamed;

  const DeviceNameField(
      {super.key, required this.id, this.existingName, this.onNamed});

  @override
  State<StatefulWidget> createState() => _DeviceNameField();
}

class _DeviceNameField extends State<DeviceNameField> {
  bool changed = false;

  @override
  Widget build(BuildContext context) {
    return PopScope(
      onPopInvoked: (didPop) {
        if (changed) {
          coord.sendCancel(id: widget.id);
        }
      },
      child: TextField(
        decoration: InputDecoration(
          icon: Icon(Icons.drive_file_rename_outline),
          hintText: widget.existingName == null
              ? 'What do you want name this device?'
              : "What should the new name be",
          labelText: widget.existingName == null
              ? 'Name'
              : "Rename “${widget.existingName}”",
        ),
        onSubmitted: (name) async {
          final completeWhen = deviceListChangeStream
              .firstWhere((change) =>
                  change.kind == DeviceListChangeKind.Named &&
                  deviceIdEquals(widget.id, change.device.id))
              .then((change) {
            widget.onNamed?.call(change.device.name!);
            changed = false;
            return;
          });
          coord.finishNaming(id: widget.id, name: name);
          await showDeviceActionDialog(
              context: context,
              complete: completeWhen,
              onCancel: () async {
                await coord.sendCancel(id: widget.id);
              },
              builder: (context) {
                return Column(children: [
                  Text("Confirm name '$name' on device"),
                  Divider(),
                  MaybeExpandedVertical(child: DeviceListContainer(child:
                      DeviceListWithIcons(iconAssigner: (context, deviceId) {
                    if (deviceIdEquals(deviceId, widget.id)) {
                      final label = LabeledDeviceText("'$name'?");
                      const icon =
                          Row(mainAxisSize: MainAxisSize.min, children: [
                        Icon(Icons.visibility, color: Colors.orange),
                        SizedBox(width: 4),
                        Text("Confirm"),
                      ]);
                      return (label, icon);
                    } else {
                      return (null, null);
                    }
                  })))
                ]);
              });
        },
        onChanged: (value) async {
          changed = true;
          await coord.updateNamePreview(id: widget.id, name: value);
        },
      ),
    );
  }
}
