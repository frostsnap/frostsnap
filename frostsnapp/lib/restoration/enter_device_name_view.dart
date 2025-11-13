import 'package:flutter/material.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/dialog_content_with_actions.dart';
import 'package:frostsnap/restoration/target_device.dart';

class EnterDeviceNameView extends StatefulWidget with TitledWidget {
  final Function(String)? onDeviceName;
  final VoidCallback? onDisconnected;
  final TargetDevice targetDevice;

  const EnterDeviceNameView({
    super.key,
    required this.targetDevice,
    this.onDeviceName,
    this.onDisconnected,
  });

  @override
  State<EnterDeviceNameView> createState() => _EnterDeviceNameViewState();

  @override
  String get titleText => 'Device name';
}

class _EnterDeviceNameViewState extends State<EnterDeviceNameView> {
  bool _canSubmit = false;
  String _currentName = '';

  @override
  void initState() {
    super.initState();
    widget.targetDevice.onDisconnected().then((_) {
      if (mounted) {
        widget.onDisconnected?.call();
      }
    });
  }

  void _handleSubmit() {
    if (_canSubmit && _currentName.isNotEmpty) {
      widget.onDeviceName?.call(_currentName);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return DialogContentWithActions(
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            "Give the device a name. If in doubt you can use the name written on the backup or make up a new one.",
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 16),
          DeviceNameField(
            id: widget.targetDevice.id,
            mode: DeviceNameMode.preview,
            onCanSubmitChanged: (canSubmit) {
              setState(() {
                _canSubmit = canSubmit;
              });
            },
            onNameChanged: (name) {
              setState(() {
                _currentName = name;
              });
            },
            onNamed: (name) => widget.onDeviceName?.call(name),
          ),
        ],
      ),
      actions: [
        FilledButton(
          onPressed: _canSubmit ? _handleSubmit : null,
          child: const Text('Continue'),
        ),
      ],
    );
  }
}
