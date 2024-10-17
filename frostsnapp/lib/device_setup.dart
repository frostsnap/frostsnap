import 'package:flutter/material.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';

import 'package:frostsnapp/ffi.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/settings.dart';

class DeviceSetup extends StatelessWidget {
  final DeviceId id;

  const DeviceSetup({super.key, required this.id});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: FsAppBar(
          title: const Text('Device Setup'),
        ),
        body: Padding(
            padding: EdgeInsets.all(10.0),
            child: Column(
              children: [
                DeviceNameField(
                    id: id,
                    onNamed: (_) {
                      if (context.mounted) {
                        Navigator.pop(context);
                      }
                    }),
              ],
            )));
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
      onPopInvokedWithResult: (didPop, result) {
        if (changed) {
          coord.sendCancel(id: widget.id);
        }
      },
      child: ConstrainedBox(
          constraints: BoxConstraints(
            maxWidth: 300, // Set the maximum width for the text box
          ),
          child: TextField(
            maxLength: 20,
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
              final result = await showDeviceActionDialog(
                  context: context,
                  complete: completeWhen,
                  builder: (context) {
                    return Column(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          DialogHeader(
                            child: Text("Confirm name '$name' on device"),
                          ),
                          Expanded(child: DeviceListWithIcons(
                              iconAssigner: (context, deviceId) {
                            if (deviceIdEquals(deviceId, widget.id)) {
                              final label = LabeledDeviceText("'$name'?");
                              final icon = ConfirmPrompt();
                              return (label, icon);
                            } else {
                              return (null, null);
                            }
                          }))
                        ]);
                  });

              if (result == null) {
                await coord.sendCancel(id: widget.id);
              }
            },
            onChanged: (value) async {
              changed = true;
              await coord.updateNamePreview(id: widget.id, name: value);
            },
          )),
    );
  }
}
