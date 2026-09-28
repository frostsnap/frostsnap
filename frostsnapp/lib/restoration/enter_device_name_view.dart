import 'package:flutter/material.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/src/rust/api.dart';

class EnterDeviceNameView extends StatefulWidget {
  final DeviceId deviceId;
  final ValueChanged<bool>? onChanged;
  final ValueChanged<String> onSubmit;

  const EnterDeviceNameView({
    super.key,
    required this.deviceId,
    required this.onSubmit,
    this.onChanged,
  });

  @override
  State<EnterDeviceNameView> createState() => EnterDeviceNameViewState();
}

class EnterDeviceNameViewState extends State<EnterDeviceNameView> {
  bool _canSubmit = false;
  String _currentName = '';

  void submit() {
    final name = _currentName.trim();
    if (_canSubmit && name.isNotEmpty) widget.onSubmit(name);
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          "Give the device a name. If in doubt you can use the name written "
          "on the backup or make up a new one.",
          style: theme.textTheme.bodyMedium,
        ),
        const SizedBox(height: 16),
        DeviceNameField(
          id: widget.deviceId,
          onCanSubmitChanged: (canSubmit) {
            _canSubmit = canSubmit;
            widget.onChanged?.call(canSubmit);
          },
          onNameChanged: (name) => _currentName = name,
          onNamed: widget.onSubmit,
        ),
      ],
    );
  }
}
