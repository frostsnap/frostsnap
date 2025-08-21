import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_name.dart';

import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';

enum DeviceNameMode {
  /// The name field renames the device and prompts the user for confirmation.
  rename,

  /// The name field stages the device name and persists it after keygen finalizes.
  preview,
}

class DeviceNameField extends StatefulWidget {
  final DeviceId id;
  final DeviceNameMode mode;
  final String? buttonText;
  final Function(String)? onNamed;

  const DeviceNameField({
    super.key,
    required this.id,
    required this.mode,
    this.buttonText,
    this.onNamed,
  });

  @override
  State<StatefulWidget> createState() => _DeviceNameField();
}

class _DeviceNameField extends State<DeviceNameField> {
  final TextEditingController _controller = TextEditingController();
  final _renameController = DeviceActionNameDialogController();

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
    _renameController.dispose();
    super.dispose();
  }

  void _previewOnSubmitted(String name) {
    final name = _controller.text;
    if (name.isNotEmpty) widget.onNamed?.call(name);
  }

  void _renameOnSubmitted(BuildContext context, String name) async {
    await _renameController.show(
      context: context,
      id: widget.id,
      name: name,
      onNamed: widget.onNamed,
    );
  }

  void _onSubmitted(BuildContext context, String name) {
    switch (widget.mode) {
      case DeviceNameMode.rename:
        _renameOnSubmitted(context, name);
      case DeviceNameMode.preview:
        _previewOnSubmitted(name);
    }
  }

  @override
  Widget build(BuildContext context) {
    final buttonText = widget.buttonText;

    return PopScope(
      onPopInvokedWithResult: (didPop, _) async {
        if (didPop) {
          await coord.sendCancel(id: widget.id);
        }
      },
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          TextField(
            controller: _controller,
            maxLength: 20,
            decoration: InputDecoration(
              border: OutlineInputBorder(),
              labelText: 'Device name',
            ),
            onSubmitted: (name) => _onSubmitted(context, name),
            onChanged: (value) async =>
                await coord.updateNamePreview(id: widget.id, name: value),
          ),
          if (buttonText != null)
            Align(
              alignment: AlignmentDirectional.centerEnd,
              child: FilledButton(
                onPressed: () => _onSubmitted(context, _controller.text),
                child: Text(buttonText),
              ),
            ),
        ],
      ),
    );
  }
}
