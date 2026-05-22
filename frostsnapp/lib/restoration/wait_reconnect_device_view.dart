import 'package:flutter/material.dart';
import 'package:frostsnap/animated_gradient_card.dart';
import 'package:frostsnap/restoration/target_device.dart';

class WaitReconnectDeviceView extends StatefulWidget {
  final TargetDevice targetDevice;
  final VoidCallback onReconnected;

  const WaitReconnectDeviceView({
    super.key,
    required this.targetDevice,
    required this.onReconnected,
  });

  @override
  State<WaitReconnectDeviceView> createState() =>
      _WaitReconnectDeviceViewState();
}

class _WaitReconnectDeviceViewState extends State<WaitReconnectDeviceView> {
  @override
  void initState() {
    super.initState();
    widget.targetDevice.waitForReconnection().then((_) {
      if (mounted) widget.onReconnected();
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.usb_off, size: 64, color: theme.colorScheme.primary),
        const SizedBox(height: 24),
        Text(
          'Device Disconnected',
          style: theme.textTheme.headlineSmall,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 24),
        AnimatedGradientPrompt(
          icon: const Icon(Icons.usb_rounded),
          content: const Text('Reconnect the device to continue'),
        ),
      ],
    );
  }
}
