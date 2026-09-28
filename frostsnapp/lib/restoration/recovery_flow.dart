import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_action_upgrade.dart'
    show DeviceActionUpgradeController;
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/restoration/candidate_ready_view.dart';
import 'package:frostsnap/restoration/enter_backup_view.dart';
import 'package:frostsnap/restoration/enter_device_name_view.dart';
import 'package:frostsnap/restoration/enter_threshold_view.dart';
import 'package:frostsnap/restoration/enter_wallet_name_view.dart';
import 'package:frostsnap/restoration/error_view.dart';
import 'package:frostsnap/restoration/firmware_upgrade_view.dart';
import 'package:frostsnap/restoration/physical_backup_success_view.dart';
import 'package:frostsnap/restoration/recovery_flow_controller.dart';
import 'package:frostsnap/restoration/start_restoration_info_view.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/restoration/wait_reconnect_device_view.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/wallet_key_mismatch.dart';

class WalletRecoveryFlow extends StatefulWidget {
  final RecoveryContext recoveryContext;
  final TargetDevice targetDevice;
  final RecoverShare? recoverShare;

  const WalletRecoveryFlow({
    super.key,
    required this.recoveryContext,
    required this.targetDevice,
    this.recoverShare,
  });

  @override
  State<WalletRecoveryFlow> createState() => _WalletRecoveryFlowState();
}

class _WalletRecoveryFlowState extends State<WalletRecoveryFlow> {
  late final RecoveryFlowController _ctrl;

  @override
  void initState() {
    super.initState();
    final recoverShare = widget.recoverShare;
    _ctrl = RecoveryFlowController(
      recoveryContext: widget.recoveryContext,
      targetDevice: widget.targetDevice,
      initialStage: recoverShare != null
          ? RecoveryFlowStage.candidateReady(candidate: recoverShare)
          : const RecoveryFlowStage.startRestorationWithPhysicalBackup(),
      existingWalletKey: (accessStructureRef, action) => existingWalletKey(
        context: mounted ? context : null,
        accessStructureRef: accessStructureRef,
        action: action,
      ),
    );
  }

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RecoveryFlowSteps(controller: _ctrl);
}

class RecoveryFlowSteps extends StatefulWidget {
  final RecoveryFlowController controller;

  const RecoveryFlowSteps({super.key, required this.controller});

  @override
  State<RecoveryFlowSteps> createState() => _RecoveryFlowStepsState();
}

class _RecoveryFlowStepsState extends State<RecoveryFlowSteps> {
  RecoveryFlowController get _ctrl => widget.controller;

  final _walletNameKey = GlobalKey<EnterWalletNameViewState>();
  final _thresholdKey = GlobalKey<EnterThresholdViewState>();
  final _deviceNameKey = GlobalKey<EnterDeviceNameViewState>();
  bool _canSubmitWalletName = false;
  bool _canSubmitDeviceName = false;

  FullscreenActionDialogController<void>? _backupPrompt;
  DeviceActionUpgradeController? _firmwareUpgrader;
  bool _completingBackupEntry = false;
  bool _popped = false;

  @override
  void initState() {
    super.initState();
    _ctrl.addListener(_onCtrlChanged);
    _syncStageResources();
  }

  @override
  void dispose() {
    _ctrl.removeListener(_onCtrlChanged);
    _disposeBackupPrompt();
    _firmwareUpgrader?.dispose();
    super.dispose();
  }

  void _onCtrlChanged() {
    if (!mounted || _popped) return;
    setState(() {});
    _syncStageResources();

    if (_ctrl.pendingBackupEntry != null && !_completingBackupEntry) {
      _completingBackupEntry = true;
      unawaited(_completeBackupEntry());
    }

    final outcome = _ctrl.outcome;
    if (outcome != null) {
      _popped = true;
      _disposeBackupPrompt();
      Navigator.of(context).pop(switch (outcome) {
        RecoveryFlowFinished(:final restorationId) => restorationId,
        RecoveryFlowDismissed() => null,
      });
    }
  }

  // Keyed on stage type, not stage identity, so a notify within one stage
  // keeps them.
  void _syncStageResources() {
    final stage = _ctrl.stage;
    if (stage is EnterBackupStage && _ctrl.outcome == null) {
      _backupPrompt ??= _buildBackupPrompt(stage.deviceName);
    } else {
      _disposeBackupPrompt();
    }
    if (stage is FirmwareUpgradeStage) {
      _firmwareUpgrader ??= DeviceActionUpgradeController();
    } else {
      _firmwareUpgrader?.dispose();
      _firmwareUpgrader = null;
    }
  }

  Future<void> _completeBackupEntry() async {
    await _backupPrompt?.clearAllActionsNeeded();
    _disposeBackupPrompt();
    if (mounted) await _ctrl.completeBackupEntry();
    _completingBackupEntry = false;
  }

  void _disposeBackupPrompt() {
    final prompt = _backupPrompt;
    _backupPrompt = null;
    prompt?.dispose();
  }

  FullscreenActionDialogController<void> _buildBackupPrompt(String deviceName) {
    late final FullscreenActionDialogController<void> prompt;
    prompt = FullscreenActionDialogController<void>(
      context: context,
      devices: [_ctrl.targetDevice.id],
      title: 'Enter Physical Backup',
      body: (context) {
        final theme = Theme.of(context);
        return Card.filled(
          margin: EdgeInsets.zero,
          color: theme.colorScheme.surfaceContainerHigh,
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  'On your device $deviceName',
                  style: theme.textTheme.titleMedium,
                ),
                const SizedBox(height: 12),
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
                const SizedBox(height: 8),
                Text(
                  '2. Enter all 25 seed words in order.',
                  style: theme.textTheme.bodyMedium,
                ),
                const SizedBox(height: 8),
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
          child: const Text('Cancel'),
          onPressed: () async {
            await prompt.clearAllActionsNeeded();
            _ctrl.back();
          },
        ),
        const DeviceActionHint(
          label: 'Enter on device',
          icon: Icons.keyboard_rounded,
        ),
      ],
    );
    return prompt;
  }

  Future<void> _runFirmwareUpgrade() async {
    final upgrader = _firmwareUpgrader;
    if (upgrader == null) return;
    _ctrl.firmwareUpgradeStarted();
    final success = await upgrader.run(context);
    if (!mounted) return;
    _ctrl.firmwareUpgradeFinished(success: success);
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) _ctrl.back();
      },
      child: _buildStep(),
    );
  }

  MultiStepDialogScaffold _buildStep() {
    return switch (_ctrl.stage) {
      CandidateReadyStage(:final candidate) => _candidateReadyStep(candidate),
      StartRestorationWithPhysicalBackupStage() => _startRestorationStep(),
      FirmwareUpgradeStage() => _firmwareUpgradeStep(),
      EnterRestorationDetailsStage() => _walletNameStep(),
      EnterThresholdStage() => _thresholdStep(),
      EnterDeviceNameStage() => _deviceNameStep(),
      GeneratingNoncesStage() => _generatingNoncesStep(),
      CompletingDeviceShareEnrollmentStage() => _step(
        stepKey: 'completingDeviceShareEnrollment',
        title: 'Completing enrollment...',
        body: const _Spinner(),
      ),
      EnterBackupStage(:final deviceName) => _step(
        stepKey: 'enterBackup',
        title: 'Enter backup on device',
        body: EnterBackupView(deviceName: deviceName),
      ),
      WaitReconnectDeviceStage() => _step(
        stepKey: 'waitReconnect',
        title: 'Device Disconnected',
        body: const WaitReconnectDeviceView(),
        footer: Align(
          child: OutlinedButton(
            onPressed: _ctrl.dismiss,
            child: const Text('Cancel'),
          ),
        ),
      ),
      PhysicalBackupSuccessStage(:final deviceName) => _step(
        stepKey: 'physicalBackupSuccess',
        title: '',
        body: PhysicalBackupSuccessView(deviceName: deviceName),
        footer: Align(
          alignment: Alignment.centerRight,
          child: FilledButton.icon(
            icon: const Icon(Icons.arrow_forward),
            label: const Text('Close'),
            onPressed: _ctrl.finish,
          ),
        ),
      ),
      ErrorStage(:final title, :final message, :final isWarning) => _errorStep(
        title,
        message,
        isWarning,
      ),
    };
  }

  MultiStepDialogScaffold _step({
    required String stepKey,
    required String title,
    required Widget body,
    bool canGoBack = false,
    Widget? footer,
  }) {
    return MultiStepDialogScaffold(
      stepKey: stepKey,
      title: Text(title),
      leading: canGoBack
          ? IconButton(
              icon: const Icon(Icons.arrow_back_rounded),
              onPressed: _ctrl.back,
              tooltip: 'Back',
            )
          : null,
      forward: _ctrl.isAnimationForward,
      duration: Durations.medium4,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(child: body),
      footer: footer,
    );
  }

  Widget _primaryButton(String label, VoidCallback? onPressed) => Align(
    alignment: Alignment.centerRight,
    child: FilledButton(onPressed: onPressed, child: Text(label)),
  );

  MultiStepDialogScaffold _candidateReadyStep(RecoverShare candidate) {
    final addingToExisting = _ctrl.recoveryContext is! NewRestorationContext;
    return _step(
      stepKey: 'candidateReady',
      title: 'Restore with existing key',
      canGoBack: true,
      body: CandidateReadyView(
        candidate: candidate,
        addingToExisting: addingToExisting,
      ),
      footer: _primaryButton(
        addingToExisting ? 'Add to wallet' : 'Restore',
        _ctrl.confirmCandidate,
      ),
    );
  }

  MultiStepDialogScaffold _startRestorationStep() {
    return _step(
      stepKey: 'startRestorationWithPhysicalBackup',
      title: 'Found blank device for backup entry',
      canGoBack: true,
      body: StartRestorationInfoView(recoveryContext: _ctrl.recoveryContext),
      footer: _primaryButton(switch (_ctrl.recoveryContext) {
        NewRestorationContext() => 'Begin restoration',
        _ => 'Next',
      }, _ctrl.confirmStartRestoration),
    );
  }

  MultiStepDialogScaffold _firmwareUpgradeStep() {
    final upgrading = _ctrl.firmwareUpgrading;
    return _step(
      stepKey: 'firmwareUpgrade',
      title: 'Firmware Upgrade Required',
      body: const FirmwareUpgradeView(),
      footer: Row(
        children: [
          TextButton(
            onPressed: upgrading ? null : _ctrl.dismiss,
            child: const Text('Cancel'),
          ),
          const Spacer(),
          FilledButton(
            onPressed: upgrading ? null : _runFirmwareUpgrade,
            child: Text(upgrading ? 'Upgrading...' : 'Upgrade Now'),
          ),
        ],
      ),
    );
  }

  MultiStepDialogScaffold _walletNameStep() {
    final (initialName, initialNetwork) = switch (_ctrl.recoveryContext) {
      NewRestorationContext(:final walletName, :final network) => (
        walletName,
        network,
      ),
      _ => (null, null),
    };
    return _step(
      stepKey: 'enterRestorationDetails',
      title: 'Wallet name',
      canGoBack: true,
      body: EnterWalletNameView(
        key: _walletNameKey,
        initialWalletName: initialName,
        initialBitcoinNetwork: initialNetwork,
        onChanged: (canSubmit) =>
            setState(() => _canSubmitWalletName = canSubmit),
        onSubmit: _ctrl.submitWalletName,
      ),
      footer: _primaryButton(
        'Continue',
        _canSubmitWalletName
            ? () => _walletNameKey.currentState?.submit()
            : null,
      ),
    );
  }

  MultiStepDialogScaffold _thresholdStep() {
    final initialThreshold = switch (_ctrl.recoveryContext) {
      NewRestorationContext(:final threshold) => threshold,
      _ => null,
    };
    return _step(
      stepKey: 'enterThreshold',
      title: 'Wallet Threshold (Optional)',
      canGoBack: true,
      body: EnterThresholdView(
        key: _thresholdKey,
        initialThreshold: initialThreshold,
        onSubmit: _ctrl.submitThreshold,
      ),
      footer: _primaryButton(
        'Continue',
        () => _thresholdKey.currentState?.submit(),
      ),
    );
  }

  MultiStepDialogScaffold _deviceNameStep() {
    return _step(
      stepKey: 'enterDeviceName',
      title: 'Device name',
      canGoBack: true,
      body: EnterDeviceNameView(
        key: _deviceNameKey,
        deviceId: _ctrl.targetDevice.id,
        onChanged: (canSubmit) =>
            setState(() => _canSubmitDeviceName = canSubmit),
        onSubmit: _ctrl.submitDeviceName,
      ),
      footer: _primaryButton(
        'Continue',
        _canSubmitDeviceName
            ? () => _deviceNameKey.currentState?.submit()
            : null,
      ),
    );
  }

  MultiStepDialogScaffold _generatingNoncesStep() {
    final stream = _ctrl.nonceStream;
    return _step(
      stepKey: 'generatingNonces',
      title: 'Preparing Device',
      body: Padding(
        padding: const EdgeInsets.symmetric(vertical: 32),
        child: stream == null
            ? const Center(child: CircularProgressIndicator())
            : NonceReplenishIndicator(
                stream: stream,
                onTerminal: _ctrl.onNonceTerminal,
              ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: TextButton(onPressed: _ctrl.back, child: const Text('Cancel')),
      ),
    );
  }

  MultiStepDialogScaffold _errorStep(
    String title,
    String message,
    bool isWarning,
  ) {
    final theme = Theme.of(context);
    return _step(
      stepKey: 'error',
      title: isWarning ? 'Warning' : 'Error',
      canGoBack: true,
      body: ErrorView(title: title, message: message, isWarning: isWarning),
      footer: Row(
        children: [
          if (!isWarning)
            CopyTapTarget(
              data: message,
              builder: (ctx, onCopy, checked) => OutlinedButton.icon(
                icon: CopyIcon(checked: checked),
                label: const Text('Copy Error'),
                onPressed: onCopy,
              ),
            ),
          const Spacer(),
          FilledButton.icon(
            icon: const Icon(Icons.refresh),
            label: const Text('Try Again'),
            onPressed: _ctrl.retryFromError,
            style: isWarning
                ? null
                : FilledButton.styleFrom(
                    backgroundColor: theme.colorScheme.error,
                    foregroundColor: theme.colorScheme.onError,
                  ),
          ),
        ],
      ),
    );
  }
}

class _Spinner extends StatelessWidget {
  const _Spinner();

  @override
  Widget build(BuildContext context) => const Padding(
    padding: EdgeInsets.symmetric(vertical: 32),
    child: Center(child: CircularProgressIndicator()),
  );
}
