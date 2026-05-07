import 'package:flutter/material.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/src/rust/api.dart';

/// Disconnect routing lives on `RecoveryFlowController` (it's a
/// workflow transition, not view-local state).
class EnterDeviceNameView extends StatefulWidget {
  final DeviceId deviceId;
  final void Function(String name, bool canSubmit)? onChanged;
  final VoidCallback? onSubmit;

  const EnterDeviceNameView({
    super.key,
    required this.deviceId,
    this.onChanged,
    this.onSubmit,
  });

  @override
  State<EnterDeviceNameView> createState() => _EnterDeviceNameViewState();
}

class _EnterDeviceNameViewState extends State<EnterDeviceNameView> {
  bool _canSubmit = false;
  String _currentName = '';

  void _emit() {
    widget.onChanged?.call(_currentName, _canSubmit && _currentName.isNotEmpty);
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
            setState(() => _canSubmit = canSubmit);
            _emit();
          },
          onNameChanged: (name) {
            setState(() => _currentName = name);
            _emit();
          },
          onNamed: (_) => widget.onSubmit?.call(),
        ),
      ],
    );
  }
}
