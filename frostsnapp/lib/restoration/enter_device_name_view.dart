import 'package:flutter/material.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';
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
  @override
  void initState() {
    super.initState();
    widget.targetDevice.onDisconnected.then((_) {
      if (mounted) {
        widget.onDisconnected?.call();
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          "If in doubt you can use the name written on the backup or make up a new one.",
          style: theme.textTheme.bodyMedium,
        ),
        const SizedBox(height: 16),
        DeviceNameField(
          id: widget.targetDevice.id,
          mode: DeviceNameMode.preview,
          buttonText: 'Continue',
          onNamed: (name) {
            widget.onDeviceName?.call(name);
          },
        ),
      ],
    );
  }
}
