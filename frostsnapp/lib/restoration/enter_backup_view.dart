import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';

class EnterBackupView extends StatefulWidget with TitledWidget {
  final Stream<EnterPhysicalBackupState> stream;
  final Function(PhysicalBackupPhase)? onFinished;
  final Function(String)? onError;
  final VoidCallback? onCancel;
  final String? deviceName;
  final TargetDevice targetDevice;

  const EnterBackupView({
    super.key,
    required this.stream,
    required this.targetDevice,
    this.deviceName,
    this.onFinished,
    this.onError,
    this.onCancel,
  });

  @override
  State<EnterBackupView> createState() => _EnterBackupViewState();

  @override
  String get titleText => 'Enter backup on device';
}

class _EnterBackupViewState extends State<EnterBackupView> {
  late final FullscreenActionDialogController<void> _backupController;
  StreamSubscription? _subscription;
  bool _dialogShown = false;

  @override
  void initState() {
    super.initState();

    _backupController = FullscreenActionDialogController(
      title: 'Enter Physical Backup',
      body: (context) {
        final theme = Theme.of(context);
        return Card.filled(
          margin: EdgeInsets.zero,
          color: theme.colorScheme.surfaceContainerHigh,
          child: Padding(
            padding: EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'On your device${widget.deviceName != null ? " ${widget.deviceName}" : ""}',
                  style: theme.textTheme.titleMedium,
                ),
                SizedBox(height: 12),
                Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(
                        text: '1. Enter the Key Number. ',
                        style: theme.textTheme.bodyMedium,
                      ),
                      TextSpan(
                        text:
                            '\nYou can find this on the inside of your backup card, labeled "Key Number".',
                        style: theme.textTheme.bodySmall?.copyWith(
                          color: theme.colorScheme.onSurfaceVariant,
                        ),
                      ),
                    ],
                  ),
                ),
                SizedBox(height: 8),
                Text(
                  '2. Enter all 25 seed words in order.',
                  style: theme.textTheme.bodyMedium,
                ),
                SizedBox(height: 8),
                Text(
                  'The app will continue automatically once complete.',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
        );
      },
      actionButtons: [
        OutlinedButton(
          child: Text('Cancel'),
          onPressed: () async {
            await _backupController.clearAllActionsNeeded();
            widget.onCancel?.call();
          },
        ),
        DeviceActionHint(
          label: 'Enter on device',
          icon: Icons.keyboard_rounded,
        ),
      ],
    );

    _subscription = widget.stream.listen((state) async {
      if (state.entered != null) {
        await _subscription?.cancel();
        await _backupController.clearAllActionsNeeded();
        widget.onFinished?.call(state.entered!);
      }
      if (state.abort != null) {
        await _backupController.clearAllActionsNeeded();
        widget.onError?.call(state.abort!);
      }
    });

    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_dialogShown && mounted) {
        _dialogShown = true;
        _backupController.addActionNeeded(context, widget.targetDevice.id);
      }
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _backupController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Center(child: CircularProgressIndicator());
  }
}
