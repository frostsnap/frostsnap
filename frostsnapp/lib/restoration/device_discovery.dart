import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_gradient_card.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/theme.dart';

class RecoveryFlowWithDiscovery extends StatefulWidget {
  final RecoveryContext recoveryContext;

  const RecoveryFlowWithDiscovery({super.key, required this.recoveryContext});

  @override
  State<RecoveryFlowWithDiscovery> createState() =>
      _RecoveryFlowWithDiscoveryState();
}

class _RecoveryFlowWithDiscoveryState extends State<RecoveryFlowWithDiscovery> {
  TargetDevice? _targetDevice;
  RecoverShare? _recoverShare;

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: _targetDevice == null,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) return;
        if (_targetDevice != null) {
          setState(() {
            _targetDevice?.dispose();
            _targetDevice = null;
            _recoverShare = null;
          });
        }
      },
      child: AnimatedSwitcher(
        duration: Durations.medium4,
        child: _targetDevice != null
            ? WalletRecoveryFlow(
                key: const ValueKey('recovery_flow'),
                recoveryContext: widget.recoveryContext,
                targetDevice: _targetDevice!,
                recoverShare: _recoverShare,
                isDialog: false,
              )
            : DeviceDiscoveryWidget(
                key: const ValueKey('device_discovery'),
                recoveryContext: widget.recoveryContext,
                onDeviceReady: (targetDevice, recoverShare) {
                  setState(() {
                    _targetDevice = targetDevice;
                    _recoverShare = recoverShare;
                  });
                },
              ),
      ),
    );
  }
}

class DeviceDiscoveryWidget extends StatefulWidget {
  final RecoveryContext recoveryContext;
  final Function(TargetDevice targetDevice, RecoverShare? recoverShare)
  onDeviceReady;

  const DeviceDiscoveryWidget({
    super.key,
    required this.recoveryContext,
    required this.onDeviceReady,
  });

  @override
  State<DeviceDiscoveryWidget> createState() => _DeviceDiscoveryWidgetState();
}

class _DeviceDiscoveryWidgetState extends State<DeviceDiscoveryWidget> {
  StreamSubscription<WaitForSingleDeviceState>? _stateSubscription;
  WaitForSingleDeviceState? _currentState;
  String? _errorMessage;

  @override
  void initState() {
    super.initState();
    startSubscription();
  }

  void startSubscription() {
    _stateSubscription?.cancel();
    _stateSubscription = coord.waitForSingleDevice().listen((state) {
      if (mounted) {
        setState(() {
          _currentState = state;
        });

        switch (state) {
          case WaitForSingleDeviceState_BlankDevice(:final deviceId):
            _handleBlankDevice(deviceId);
            break;
          case WaitForSingleDeviceState_DeviceWithShare(
            :final deviceId,
            :final share,
          ):
            _handleDeviceWithShare(deviceId, share);
            break;
          case WaitForSingleDeviceState_NoDevice():
          case WaitForSingleDeviceState_TooManyDevices():
          case WaitForSingleDeviceState_WaitingForDevice():
            break;
        }
      }
    });
  }

  @override
  void dispose() {
    _stateSubscription?.cancel();
    super.dispose();
  }

  void _handleBlankDevice(deviceId) {
    final targetDevice = TargetDevice(deviceId);
    widget.onDeviceReady(targetDevice, null);
  }

  Future<void> _handleDeviceWithShare(deviceId, RecoverShare share) async {
    final error = await _validateShare(share);

    if (error != null) {
      setState(() {
        _errorMessage = error;
      });
      return;
    }

    final targetDevice = TargetDevice(deviceId);
    widget.onDeviceReady(targetDevice, share);
  }

  Future<String?> _validateShare(RecoverShare share) async {
    switch (widget.recoveryContext) {
      case NewRestorationContext():
        final encryptionKey = await SecureKeyProvider.getEncryptionKey();
        final error = await coord.checkStartRestoringKeyFromDeviceShare(
          recoverShare: share,
          encryptionKey: encryptionKey,
        );
        return error?.toString();

      case ContinuingRestorationContext(:final restorationId):
        final encryptionKey = await SecureKeyProvider.getEncryptionKey();
        final error = await coord.checkContinueRestoringWalletFromDeviceShare(
          restorationId: restorationId,
          recoverShare: share,
          encryptionKey: encryptionKey,
        );
        return error?.toString();

      case AddingToWalletContext(:final accessStructureRef):
        final encryptionKey = await SecureKeyProvider.getEncryptionKey();
        final error = await coord.checkRecoverShare(
          accessStructureRef: accessStructureRef,
          recoverShare: share,
          encryptionKey: encryptionKey,
        );
        return error?.toString();
    }
  }

  String _getPromptText() {
    return switch (widget.recoveryContext) {
      ContinuingRestorationContext(:final restorationId) => () {
        final name = coord
            .getRestorationState(restorationId: restorationId)!
            .keyName;
        return 'Plug in a Frostsnap to continue restoring "$name".';
      }(),
      AddingToWalletContext(:final accessStructureRef) => () {
        final name = coord
            .getFrostKey(keyId: accessStructureRef.keyId)!
            .keyName();
        return 'Plug in a Frostsnap to add it to "$name".';
      }(),
      NewRestorationContext() =>
        'Plug in a Frostsnap device to begin wallet restoration.',
    };
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final windowSize = WindowSizeContext.of(context);

    final header = TopBarSliver(
      title: Text(
        _errorMessage != null ? 'Incompatible Device' : 'Restore from device',
      ),
      leading: IconButton(
        icon: Icon(Icons.arrow_back_rounded),
        onPressed: () => Navigator.of(context).pop(),
        tooltip: 'Cancel',
      ),
    );

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Flexible(
          child: CustomScrollView(
            shrinkWrap: windowSize != WindowSizeClass.compact,
            slivers: [
              header,
              SliverPadding(
                padding: const EdgeInsets.all(16.0),
                sliver: SliverToBoxAdapter(
                  child: Center(child: _buildContent(theme)),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildContent(ThemeData theme) {
    if (_errorMessage != null) {
      return _buildErrorCard(theme);
    }

    return switch (_currentState) {
      WaitForSingleDeviceState_NoDevice() => _buildWaitingCard(theme),
      WaitForSingleDeviceState_WaitingForDevice() => _buildDetectedCard(theme),
      WaitForSingleDeviceState_TooManyDevices() => _buildWaitingCard(theme),
      _ => _buildWaitingCard(theme),
    };
  }

  Widget _buildErrorCard(ThemeData theme) {
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 400),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.error_outline, size: 64, color: theme.colorScheme.error),
          const SizedBox(height: 16),
          Text('Incompatible Device', style: theme.textTheme.headlineSmall),
          const SizedBox(height: 12),
          Text(_errorMessage!, textAlign: TextAlign.center),
          const SizedBox(height: 24),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            spacing: 12,
            children: [
              TextButton(
                onPressed: () {
                  setState(() {
                    startSubscription();
                    _errorMessage = null;
                  });
                },
                child: const Text('Try Another Device'),
              ),
              FilledButton(
                onPressed: () {
                  Navigator.of(context).pop();
                },
                child: const Text('Cancel'),
              ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildWaitingCard(ThemeData theme) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Icon(Icons.usb_rounded, size: 64, color: theme.colorScheme.primary),
        const SizedBox(height: 16),
        IntrinsicWidth(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              ListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                leading: ImageIcon(
                  AssetImage('assets/icons/device2.png'),
                  size: 24,
                ),
                title: Text('To restore from a device with a key: plug it in'),
              ),
              ListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                leading: Icon(Icons.description_outlined, size: 24),
                title: Text(
                  'To enter a seed word backup: plug in a blank device',
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        Builder(
          builder: (context) {
            final isTooManyDevices =
                _currentState is WaitForSingleDeviceState_TooManyDevices;
            final theme = Theme.of(context);
            return AnimatedGradientPrompt(
              icon: Icon(
                isTooManyDevices
                    ? Icons.warning_amber_rounded
                    : Icons.info_rounded,
                color: isTooManyDevices ? theme.colorScheme.error : null,
              ),
              content: Text(
                isTooManyDevices
                    ? 'Multiple devices detected. Please disconnect all but one device.'
                    : _getPromptText(),
              ),
            );
          },
        ),
        const SizedBox(height: 24),
        TextButton(
          onPressed: () {
            Navigator.of(context).pop();
          },
          child: const Text('Cancel'),
        ),
      ],
    );
  }

  Widget _buildDetectedCard(ThemeData theme) {
    return ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 400),
      child: _buildCardContent(
        theme: theme,
        icon: Icons.usb_rounded,
        title: 'Device Detected',
        content: 'Reading device information...',
        actions: [const CircularProgressIndicator()],
        actionsAlignment: MainAxisAlignment.center,
      ),
    );
  }

  Widget _buildCardContent({
    required ThemeData theme,
    required IconData icon,
    required String title,
    required String content,
    required List<Widget> actions,
    MainAxisAlignment actionsAlignment = MainAxisAlignment.center,
  }) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, size: 64, color: theme.colorScheme.primary),
        const SizedBox(height: 16),
        Text(title, style: theme.textTheme.headlineSmall),
        const SizedBox(height: 12),
        Text(content, textAlign: TextAlign.center),
        const SizedBox(height: 24),
        Row(
          mainAxisAlignment: actionsAlignment,
          spacing: 12,
          children: actions,
        ),
      ],
    );
  }
}
