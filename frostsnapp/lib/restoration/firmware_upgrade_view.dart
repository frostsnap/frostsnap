import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_upgrade.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/dialog_content_with_actions.dart';
import 'package:frostsnap/restoration/target_device.dart';

class FirmwareUpgradeView extends StatefulWidget with TitledWidget {
  final TargetDevice targetDevice;
  final VoidCallback onComplete;
  final VoidCallback onCancel;
  final VoidCallback onDisconnected;

  const FirmwareUpgradeView({
    super.key,
    required this.targetDevice,
    required this.onComplete,
    required this.onCancel,
    required this.onDisconnected,
  });

  @override
  State<FirmwareUpgradeView> createState() => _FirmwareUpgradeViewState();

  @override
  String get titleText => 'Firmware Upgrade Required';
}

class _FirmwareUpgradeViewState extends State<FirmwareUpgradeView> {
  late final DeviceActionUpgradeController _controller;
  bool _isUpgrading = false;

  @override
  void initState() {
    super.initState();
    _controller = DeviceActionUpgradeController();

    widget.targetDevice.onDisconnected().then((_) {
      if (mounted && !_isUpgrading) {
        widget.onDisconnected();
      }
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _startUpgrade() async {
    setState(() {
      _isUpgrading = true;
    });

    final success = await _controller.run(context);

    if (mounted) {
      if (success) {
        widget.onComplete();
      } else {
        Navigator.of(context).pop();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return DialogContentWithActions(
      key: const ValueKey('firmwareUpgradePrompt'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            Icons.system_update_alt_rounded,
            size: 64,
            color: theme.colorScheme.primary,
          ),
          const SizedBox(height: 24),
          Text(
            'Firmware Update Required',
            style: theme.textTheme.headlineSmall,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 16),
          Text(
            'This device needs a firmware update before it can be used for wallet restoration.',
            textAlign: TextAlign.center,
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: _isUpgrading ? null : () => Navigator.of(context).pop(),
          child: Text('Cancel'),
        ),
        FilledButton(
          onPressed: _isUpgrading ? null : _startUpgrade,
          child: Text(_isUpgrading ? 'Upgrading...' : 'Upgrade Now'),
        ),
      ],
    );
  }
}
