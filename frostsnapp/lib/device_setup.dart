import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_name.dart';

import 'package:frostsnap/global.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/name.dart';

enum DeviceNameMode {
  /// The name field renames the device and prompts the user for confirmation.
  rename,

  /// The name field stages the device name and persists it after keygen finalizes.
  preview,
}

class DeviceNameField extends StatefulWidget {
  final DeviceId id;
  final DeviceNameMode mode;
  final String? initialValue;
  final Function(String)? onNamed;
  final Function(bool)? onCanSubmitChanged;
  final Function(String)? onNameChanged;

  const DeviceNameField({
    super.key,
    required this.id,
    required this.mode,
    this.initialValue,
    this.onNamed,
    this.onCanSubmitChanged,
    this.onNameChanged,
  });

  @override
  State<StatefulWidget> createState() => DeviceNameFieldState();
}

class DeviceNameFieldState extends State<DeviceNameField> {
  final TextEditingController _controller = TextEditingController();
  final _renameController = DeviceActionNameDialogController();

  @override
  void initState() {
    super.initState();
    final name = widget.initialValue ?? coord.getDeviceName(id: widget.id);
    if (name != null) {
      _controller.text = name;
    }
    coord.updateNamePreview(id: widget.id, name: _controller.text);

    WidgetsBinding.instance.addPostFrameCallback((_) {
      widget.onCanSubmitChanged?.call(canSubmit);
      widget.onNameChanged?.call(_controller.text);
    });

    _controller.addListener(() {
      widget.onCanSubmitChanged?.call(canSubmit);
      widget.onNameChanged?.call(_controller.text);
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    _renameController.dispose();
    super.dispose();
  }

  bool get canSubmit => _controller.text.trim().isNotEmpty;

  void _handleSubmitted(BuildContext context) {
    final name = _controller.text.trim();
    if (name.isEmpty) return;

    switch (widget.mode) {
      case DeviceNameMode.rename:
        _renameController.show(
          context: context,
          id: widget.id,
          name: name,
          onNamed: widget.onNamed,
        );
      case DeviceNameMode.preview:
        widget.onNamed?.call(name);
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
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: 16,
        children: [
          TextField(
            autofocus: true,
            controller: _controller,
            maxLength: DeviceName.maxLength(),
            decoration: InputDecoration(
              border: OutlineInputBorder(),
              labelText: 'Device name',
            ),
            inputFormatters: [nameInputFormatter],
            onSubmitted: (_) => _handleSubmitted(context),
            onChanged: (value) async =>
                await coord.updateNamePreview(id: widget.id, name: value),
          ),
        ],
      ),
    );
  }
}
