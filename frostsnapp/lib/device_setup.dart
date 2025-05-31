import 'package:flutter/material.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/id_ext.dart';
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
      appBar: FsAppBar(title: const Text('Device Setup')),
      body: SafeArea(
        child: Center(
          child: Padding(
            padding: const EdgeInsets.all(24.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const SizedBox(height: 20),
                DeviceNameField(
                  id: id,
                  mode: DeviceNameMode.preview,
                  onNamed: (_) {
                    if (context.mounted) {
                      Navigator.pop(context);
                    }
                  },
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

enum DeviceNameMode {
  /// The name field renames the device and prompts the user for confirmation.
  rename,

  /// The name field stages the device name and persists it after keygen finalizes.
  preview,
}

class DeviceNameField extends StatefulWidget {
  final DeviceId id;
  final DeviceNameMode mode;
  final Function(String)? onNamed;

  const DeviceNameField({
    super.key,
    required this.id,
    required this.mode,
    this.onNamed,
  });

  @override
  State<StatefulWidget> createState() => _DeviceNameField();
}

class _DeviceNameField extends State<DeviceNameField> {
  final TextEditingController _controller = TextEditingController();

  @override
  void initState() {
    super.initState();
    final name = coord.getDeviceName(id: widget.id);
    if (name != null) {
      _controller.text = name;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _previewOnSubmitted(String name) {
    final name = _controller.text;
    if (name.isNotEmpty) widget.onNamed?.call(name);
  }

  void _renameOnSubmitted(BuildContext context, String name) async {
    final completeWhen = GlobalStreams.deviceListChangeStream
        .firstWhere(
          (change) =>
              change.kind == DeviceListChangeKind.Named &&
              deviceIdEquals(widget.id, change.device.id),
        )
        .then((change) => change.device.name!);
    coord.finishNaming(id: widget.id, name: name);
    if (context.mounted) {
      final confirmedName = await showDeviceActionDialog(
        context: context,
        complete: completeWhen,
        builder: (context) {
          return Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              DialogHeader(child: Text("Confirm name '$name' on device")),
              Expanded(
                child: DeviceListWithIcons(
                  iconAssigner: (context, deviceId) {
                    if (deviceIdEquals(deviceId, widget.id)) {
                      final label = LabeledDeviceText("'$name'?");
                      final icon = ConfirmPrompt();
                      return (label, icon);
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

      if (confirmedName != null) {
        widget.onNamed?.call(confirmedName);
      } else {
        coord.sendCancel(id: widget.id);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      onPopInvokedWithResult: (didPop, _) async {
        if (didPop) {
          await coord.sendCancel(id: widget.id);
        }
      },
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: 300),
        child: TextField(
          controller: _controller,
          maxLength: 20,
          decoration: InputDecoration(
            icon: Icon(Icons.drive_file_rename_outline),
            hintText: 'What do you want to name this device?',
            labelText: 'Name',
          ),
          onSubmitted: switch (widget.mode) {
            DeviceNameMode.rename =>
              (name) => _renameOnSubmitted(context, name),
            DeviceNameMode.preview => (name) => _previewOnSubmitted(name),
          },
          onChanged: (value) {
            coord.updateNamePreview(id: widget.id, name: value);
          },
        ),
      ),
    );
  }
}
