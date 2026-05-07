import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/copy_feedback.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_action_upgrade.dart' as device_action_upgrade;
import 'package:frostsnap/fullscreen_dialog_scaffold.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:rxdart/rxdart.dart';
import 'package:frostsnap/restoration/candidate_ready_view.dart';
import 'package:frostsnap/restoration/enter_backup_view.dart';
import 'package:frostsnap/restoration/enter_device_name_view.dart';
import 'package:frostsnap/restoration/enter_threshold_view.dart';
import 'package:frostsnap/restoration/enter_wallet_name_view.dart';
import 'package:frostsnap/restoration/error_view.dart';
import 'package:frostsnap/restoration/firmware_upgrade_view.dart';
import 'package:frostsnap/restoration/physical_backup_success_view.dart';
import 'package:frostsnap/restoration/start_restoration_info_view.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/restoration/wait_reconnect_device_view.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/stream_ext.dart';

sealed class _BackupResult {
  const _BackupResult();
}

final class _BackupEntered extends _BackupResult {
  const _BackupEntered(this.phase);
  final PhysicalBackupPhase phase;
}

final class _BackupAborted extends _BackupResult {
  const _BackupAborted(this.message);
  final String message;
}

/// Widget-unaware workflow state and protocol side effects.
class RecoveryFlowController extends ChangeNotifier {
  RecoveryFlowController({
    required RecoveryContext recoveryContext,
    required RecoveryFlowState flowState,
  }) : _recoveryContext = recoveryContext,
       _flowState = flowState {
    _enterStage(_flowState.stage);
  }

  RecoveryContext _recoveryContext;
  RecoveryContext get recoveryContext => _recoveryContext;

  final RecoveryFlowState _flowState;
  RecoveryFlowState get flowState => _flowState;

  TargetDevice get targetDevice => _flowState.targetDevice;

  bool _isAnimationForward = true;
  bool get isAnimationForward => _isAnimationForward;

  final List<RecoveryFlowStage> _prevStates = <RecoveryFlowStage>[];

  bool _completed = false;
  bool get completed => _completed;
  RestorationId? _completedRestorationId;
  RestorationId? get completedRestorationId => _completedRestorationId;

  /// Set in the EnterPhysicalBackup stream listener and consumed by
  /// the State, which must dismiss `_backupController` before calling
  /// [completeBackupEntry] — otherwise the overlay (root navigator)
  /// gets orphaned above the popped recovery dialog (child navigator).
  _BackupResult? _pendingBackupResult;
  _BackupResult? get pendingBackupResult => _pendingBackupResult;

  StreamSubscription? _backupSubscription;

  ValueStream<NonceReplenishState>? _nonceStream;
  ValueStream<NonceReplenishState>? get nonceStream => _nonceStream;

  bool _nonceTerminalHandled = false;

  /// Async protocol work can resume after the page has been dismissed.
  bool _disposed = false;

  @override
  void notifyListeners() {
    if (_disposed) return;
    super.notifyListeners();
  }

  void transitionTo(RecoveryFlowStage stage) {
    if (_disposed) return;
    if (identical(stage, _flowState.stage)) return;
    final prev = _flowState.stage;
    _leaveStage(prev);
    _flowState.stage = stage;
    _enterStage(stage);
    notifyListeners();
  }

  void pushAndTransitionTo(RecoveryFlowStage stage) {
    _isAnimationForward = true;
    _prevStates.add(_flowState.stage);
    transitionTo(stage);
  }

  bool tryPopPrevState() {
    if (_prevStates.isEmpty) return false;
    _isAnimationForward = false;
    final stage = _prevStates.removeLast();
    transitionTo(stage);
    return true;
  }

  void _popOnError({
    required String errorTitle,
    required String errorMessage,
    bool isException = false,
  }) {
    _isAnimationForward = false;
    final returnStage = _prevStates.isNotEmpty
        ? _prevStates.removeLast()
        : _flowState.stage;
    transitionTo(
      RecoveryFlowStage.error(
        title: errorTitle,
        message: errorMessage,
        isWarning: !isException,
        returnStage: returnStage,
      ),
    );
  }

  void _enterStage(RecoveryFlowStage stage) {
    switch (stage) {
      case GeneratingNoncesStage(:final nextStage):
        final nonceRequest = coord.createNonceRequest(
          devices: [targetDevice.id],
        );
        _nonceTerminalHandled = false;
        _nonceStream = coord
            .replenishNonces(
              nonceRequest: nonceRequest,
              devices: [targetDevice.id],
            )
            .toBehaviorSubject();
        _nextStageAfterNonce = nextStage;
        targetDevice.onDisconnected().then((_) {
          if (_disposed) return;
          if (_flowState.stage is GeneratingNoncesStage) {
            nonceDeviceDisconnected();
          }
        });
      case EnterBackupStage():
        _backupSubscription = coord
            .tellDeviceToEnterPhysicalBackup(deviceId: targetDevice.id)
            .listen(_onBackupStreamEvent);
      case FirmwareUpgradeStage():
        // During-upgrade disconnects are handled by `_runFirmwareUpgrade`.
        targetDevice.onDisconnected().then((_) {
          if (_disposed) return;
          if (_flowState.stage is FirmwareUpgradeStage) {
            firmwareUpgradeDisconnected();
          }
        });
      case EnterDeviceNameStage():
        targetDevice.onDisconnected().then((_) {
          if (_disposed) return;
          if (_flowState.stage is EnterDeviceNameStage) {
            enterDeviceNameDisconnected();
          }
        });
      default:
        break;
    }
  }

  void _leaveStage(RecoveryFlowStage stage) {
    switch (stage) {
      case GeneratingNoncesStage():
        // Only explicit cancel should publish `cancelProtocol()`.
        _nonceStream = null;
        _nextStageAfterNonce = null;
        _nonceTerminalHandled = false;
      case EnterBackupStage():
        _backupSubscription?.cancel();
        _backupSubscription = null;
        _pendingBackupResult = null;
      default:
        break;
    }
  }

  void confirmCandidate(RecoverShare candidate) {
    pushAndTransitionTo(
      RecoveryFlowStage.generatingNonces(
        nextStage: RecoveryFlowStage.completingDeviceShareEnrollment(
          candidate: candidate,
        ),
      ),
    );
  }

  void confirmStartRestoration() {
    pushAndTransitionTo(
      targetDevice.needsFirmwareUpgrade()
          ? const RecoveryFlowStage.firmwareUpgrade()
          : switch (_recoveryContext) {
              NewRestorationContext() =>
                const RecoveryFlowStage.enterRestorationDetails(),
              _ => const RecoveryFlowStage.enterDeviceName(),
            },
    );
  }

  void firmwareUpgradeCompleted() {
    transitionTo(switch (_recoveryContext) {
      NewRestorationContext() =>
        const RecoveryFlowStage.enterRestorationDetails(),
      _ => const RecoveryFlowStage.enterDeviceName(),
    });
  }

  void firmwareUpgradeDisconnected() {
    transitionTo(
      RecoveryFlowStage.waitReconnectDevice(
        nextStage: const RecoveryFlowStage.firmwareUpgrade(),
      ),
    );
  }

  void enterDeviceNameDisconnected() {
    transitionTo(
      RecoveryFlowStage.waitReconnectDevice(
        nextStage: const RecoveryFlowStage.enterDeviceName(),
      ),
    );
  }

  void submitDeviceName(String deviceName) {
    final nonceRequest = coord.createNonceRequest(devices: [targetDevice.id]);
    if (nonceRequest.someNoncesRequested()) {
      pushAndTransitionTo(
        RecoveryFlowStage.generatingNonces(
          nextStage: RecoveryFlowStage.enterBackup(deviceName: deviceName),
        ),
      );
    } else {
      pushAndTransitionTo(
        RecoveryFlowStage.enterBackup(deviceName: deviceName),
      );
    }
  }

  void submitWalletName(String walletName, BitcoinNetwork bitcoinNetwork) {
    if (_recoveryContext case NewRestorationContext()) {
      _recoveryContext = RecoveryContext.newRestoration(
        walletName: walletName,
        network: bitcoinNetwork,
        threshold: null,
      );
    }
    pushAndTransitionTo(
      RecoveryFlowStage.enterThreshold(
        walletName: walletName,
        network: bitcoinNetwork,
      ),
    );
  }

  void submitThreshold(int? threshold) {
    if (_recoveryContext case NewRestorationContext(
      :final walletName,
      :final network,
    )) {
      _recoveryContext = RecoveryContext.newRestoration(
        walletName: walletName,
        network: network,
        threshold: threshold,
      );
    }
    pushAndTransitionTo(const RecoveryFlowStage.enterDeviceName());
  }

  void deviceReconnected(RecoveryFlowStage nextStage) {
    transitionTo(nextStage);
  }

  void retryFromError(RecoveryFlowStage returnStage) {
    _isAnimationForward = true;
    transitionTo(returnStage);
  }

  RecoveryFlowStage? _nextStageAfterNonce;

  void onNonceTerminal(NonceReplenishTerminal terminal) {
    if (_nonceTerminalHandled) return;
    _nonceTerminalHandled = true;
    final nextStage = _nextStageAfterNonce;
    if (nextStage == null) return;
    switch (terminal) {
      case NonceReplenishCompleted():
        // Share enrollment is a no-UI async tail after nonce generation.
        if (nextStage case CompletingDeviceShareEnrollmentStage(
          :final candidate,
        )) {
          unawaited(_completeDeviceShareEnrollment(candidate));
        } else {
          transitionTo(nextStage);
        }
      case NonceReplenishAborted():
        _popOnError(
          errorTitle: 'Nonce Generation Failed',
          errorMessage: 'Device disconnected during preparation',
        );
      case NonceReplenishFailed(:final error):
        _popOnError(
          errorTitle: 'Nonce Generation Failed',
          errorMessage: 'Failed to prepare device: $error',
        );
    }
  }

  void cancelGeneratingNonces() {
    coord.cancelProtocol();
    tryPopPrevState();
  }

  void nonceDeviceDisconnected() {
    final stage = _flowState.stage;
    if (stage is! GeneratingNoncesStage) return;
    transitionTo(
      RecoveryFlowStage.waitReconnectDevice(
        nextStage: RecoveryFlowStage.generatingNonces(
          nextStage: stage.nextStage,
        ),
      ),
    );
  }

  void _onBackupStreamEvent(EnterPhysicalBackupState state) {
    if (state.entered != null) {
      _pendingBackupResult = _BackupEntered(state.entered!);
      notifyListeners();
    }
    if (state.abort != null) {
      _pendingBackupResult = _BackupAborted(state.abort!);
      notifyListeners();
    }
  }

  /// The State must dismiss the root-navigator overlay before calling this.
  Future<void> completeBackupEntry() async {
    final result = _pendingBackupResult;
    if (result == null) return;
    _pendingBackupResult = null;
    switch (result) {
      case _BackupEntered(:final phase):
        await _onBackupEntered(phase);
      case _BackupAborted(:final message):
        _popOnError(errorTitle: 'Backup Entry Failed', errorMessage: message);
    }
  }

  Future<void> _onBackupEntered(PhysicalBackupPhase phase) async {
    try {
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      switch (_recoveryContext) {
        case AddingToWalletContext(:final accessStructureRef):
          final isValid = await coord.checkPhysicalBackup(
            accessStructureRef: accessStructureRef,
            phase: phase,
            encryptionKey: encryptionKey,
          );
          if (!isValid) {
            _popOnError(
              errorTitle: 'Cannot add backup',
              errorMessage: 'The backup is not compatible with this wallet',
            );
            return;
          }
          await coord.tellDeviceToConsolidatePhysicalBackup(
            accessStructureRef: accessStructureRef,
            phase: phase,
            encryptionKey: encryptionKey,
          );
          _completed = true;
          notifyListeners();

        case NewRestorationContext(
          :final walletName,
          :final network,
          :final threshold,
        ):
          final restorationId = await coord.startRestoringWallet(
            name: walletName!,
            threshold: threshold,
            network: network!,
          );
          final error = await coord.checkPhysicalBackupForRestoration(
            restorationId: restorationId,
            phase: phase,
            encryptionKey: encryptionKey,
          );
          if (error != null) {
            _popOnError(
              errorTitle: 'Cannot add backup',
              errorMessage: error.toString(),
            );
            return;
          }
          await coord.tellDeviceToSavePhysicalBackup(
            phase: phase,
            restorationId: restorationId,
          );
          _recoveryContext = (_recoveryContext as NewRestorationContext)
              .copyWith(restorationId: restorationId);
          final deviceName =
              coord.getDeviceName(id: targetDevice.id) ?? '<unknown>';
          transitionTo(
            RecoveryFlowStage.physicalBackupSuccess(deviceName: deviceName),
          );

        case ContinuingRestorationContext(:final restorationId):
          final error = await coord.checkPhysicalBackupForRestoration(
            restorationId: restorationId,
            phase: phase,
            encryptionKey: encryptionKey,
          );
          if (error != null) {
            _popOnError(
              errorTitle: 'Cannot add backup',
              errorMessage: error.toString(),
            );
            return;
          }
          await coord.tellDeviceToSavePhysicalBackup(
            phase: phase,
            restorationId: restorationId,
          );
          final deviceName =
              coord.getDeviceName(id: targetDevice.id) ?? '<unknown>';
          transitionTo(
            RecoveryFlowStage.physicalBackupSuccess(deviceName: deviceName),
          );
      }
    } catch (e, stackTrace) {
      _popOnError(
        errorTitle: 'Failed to save backup',
        errorMessage: '$e\n\nStack trace:\n$stackTrace',
        isException: true,
      );
    }
  }

  void cancelBackupEntry() {
    tryPopPrevState();
  }

  /// True between nonce-replenish completion and the share-enrollment
  /// protocol call returning. Visually we're still on `generatingNonces`,
  /// but the device is mid-call; OS back during this window would
  /// race the async work, so the State blocks it.
  bool _shareEnrollmentInFlight = false;
  bool get shareEnrollmentInFlight => _shareEnrollmentInFlight;

  Future<void> _completeDeviceShareEnrollment(RecoverShare candidate) async {
    _shareEnrollmentInFlight = true;
    try {
      RestorationId? restorationId;
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();
      switch (_recoveryContext) {
        case ContinuingRestorationContext(:final restorationId):
          await coord.continueRestoringWalletFromDeviceShare(
            restorationId: restorationId,
            recoverShare: candidate,
            encryptionKey: encryptionKey,
          );
        case AddingToWalletContext(:final accessStructureRef):
          await coord.recoverShare(
            accessStructureRef: accessStructureRef,
            recoverShare: candidate,
            encryptionKey: encryptionKey,
          );
        case NewRestorationContext():
          restorationId = await coord.startRestoringWalletFromDeviceShare(
            recoverShare: candidate,
          );
      }
      _completedRestorationId = restorationId;
      _completed = true;
      _shareEnrollmentInFlight = false;
      notifyListeners();
    } catch (e, stackTrace) {
      _shareEnrollmentInFlight = false;
      _popOnError(
        errorTitle: 'Unexpected error',
        errorMessage: '$e\n\nStack trace:\n$stackTrace',
        isException: true,
      );
    }
  }

  @override
  void dispose() {
    // Set _disposed first: in-flight async work resuming after this
    // takes the no-op branch in `notifyListeners` / `transitionTo`.
    _disposed = true;
    // Dismiss-while-protocol-running: cancel before tearing down so
    // the device doesn't keep a stale protocol session open.
    if (_flowState.stage is GeneratingNoncesStage) {
      coord.cancelProtocol();
    }
    _backupSubscription?.cancel();
    _backupSubscription = null;
    _flowState.dispose();
    super.dispose();
  }
}

// =============================================================================
// WalletRecoveryFlow
// =============================================================================

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

  // Lifted from the body widgets so the parent's footer can read
  // them. (`onChanged` callbacks update these.)
  String _enterDeviceNameCurrent = '';
  bool _enterDeviceNameCanSubmit = false;
  bool _enterWalletNameCanSubmit = false;

  // GlobalKey + `submit()` keeps form state local to each body widget
  // — the parent's footer button just calls into it.
  final _enterThresholdKey = GlobalKey<EnterThresholdViewState>();
  final _enterWalletNameKey = GlobalKey<EnterWalletNameViewState>();

  // Context-bound (need BuildContext, so they live here, not on the
  // controller). Lifecycle is edge-detected against stage predicates
  // in [_onCtrlChanged].
  FullscreenActionDialogController<void>? _backupController;
  device_action_upgrade.DeviceActionUpgradeController?
  _firmwareUpgradeController;
  bool _isUpgrading = false;

  bool _popped = false;

  /// True while [_completeBackupEntry] is awaiting overlay dismissal.
  /// Without it a re-fired listener would kick a second completion.
  bool _backupCompletionInFlight = false;

  @override
  void initState() {
    super.initState();
    final initialStage = widget.recoverShare != null
        ? RecoveryFlowStage.candidateReady(candidate: widget.recoverShare!)
        : RecoveryFlowStage.startRestorationWithPhysicalBackup();
    _ctrl = RecoveryFlowController(
      recoveryContext: widget.recoveryContext,
      flowState: RecoveryFlowState(
        targetDevice: widget.targetDevice,
        stage: initialStage,
      ),
    );
    _ctrl.addListener(_onCtrlChanged);
    // Run the initial edge-detection so any context-bound resources
    // for the starting stage get instantiated.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _onCtrlChanged();
    });
  }

  @override
  void dispose() {
    _ctrl.removeListener(_onCtrlChanged);
    _disposeBackupController();
    _firmwareUpgradeController?.dispose();
    _firmwareUpgradeController = null;
    _ctrl.dispose();
    super.dispose();
  }

  void _onCtrlChanged() {
    if (!mounted || _popped) return;
    setState(() {});

    // Edge-detect stage predicates (NOT stage equality) so a
    // redundant notify with the same stage doesn't reinstantiate.
    final isEnterBackup = _ctrl.flowState.stage is EnterBackupStage;
    final hasBackupCtrl = _backupController != null;
    if (isEnterBackup && !hasBackupCtrl) {
      _backupController = _buildBackupController();
    } else if (!isEnterBackup && hasBackupCtrl) {
      _disposeBackupController();
    }

    final isFirmware = _ctrl.flowState.stage is FirmwareUpgradeStage;
    final hasFirmwareCtrl = _firmwareUpgradeController != null;
    if (isFirmware && !hasFirmwareCtrl) {
      _firmwareUpgradeController =
          device_action_upgrade.DeviceActionUpgradeController();
    } else if (!isFirmware && hasFirmwareCtrl) {
      _firmwareUpgradeController!.dispose();
      _firmwareUpgradeController = null;
      _isUpgrading = false;
    }

    // EnterBackup handshake: dismiss overlay before letting the
    // controller transition (see `completeBackupEntry` doc).
    if (_ctrl.pendingBackupResult != null && !_backupCompletionInFlight) {
      _backupCompletionInFlight = true;
      unawaited(_completeBackupEntry());
    }

    if (_ctrl.completed) {
      _popped = true;
      final result = _ctrl.completedRestorationId;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Navigator.of(context).pop(result);
      });
      return;
    }
  }

  Future<void> _completeBackupEntry() async {
    final ctrl = _backupController;
    if (ctrl != null) {
      await ctrl.clearAllActionsNeeded();
      _disposeBackupController();
    }
    if (!mounted) return;
    await _ctrl.completeBackupEntry();
    _backupCompletionInFlight = false;
  }

  void _disposeBackupController() {
    final ctrl = _backupController;
    if (ctrl == null) return;
    _backupController = null;
    ctrl.dispose();
  }

  FullscreenActionDialogController<void> _buildBackupController() {
    final stage = _ctrl.flowState.stage;
    final deviceName = stage is EnterBackupStage ? stage.deviceName : null;
    return FullscreenActionDialogController<void>(
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
                  'On your device${deviceName != null ? " $deviceName" : ""}',
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
                            '\nYou can find this on the inside of your backup '
                            'card, labeled "Key Number".',
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
            await _backupController?.clearAllActionsNeeded();
            _ctrl.cancelBackupEntry();
          },
        ),
        const DeviceActionHint(
          label: 'Enter on device',
          icon: Icons.keyboard_rounded,
        ),
      ],
    );
  }

  /// OS-back routing. Footer Cancel/Close are the only way out of
  /// unsafe/waiting stages — letting OS back unwind would race
  /// in-flight protocol work.
  void _goBackOrClose() {
    if (_ctrl.shareEnrollmentInFlight) return;
    switch (_ctrl.flowState.stage) {
      case GeneratingNoncesStage():
        _ctrl.cancelGeneratingNonces();
        return;
      case CompletingDeviceShareEnrollmentStage():
      case WaitReconnectDeviceStage():
      case PhysicalBackupSuccessStage():
        return;
      case FirmwareUpgradeStage():
        if (_isUpgrading) return;
      default:
        break;
    }
    if (!_ctrl.tryPopPrevState()) {
      Navigator.of(context).pop();
    }
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        _goBackOrClose();
      },
      child: _buildStep(context),
    );
  }

  MultiStepDialogScaffold _buildStep(BuildContext context) {
    final stage = _ctrl.flowState.stage;
    return switch (stage) {
      CandidateReadyStage(:final candidate) => _buildCandidateReadyStep(
        candidate,
      ),
      FirmwareUpgradeStage() => _buildFirmwareUpgradeStep(),
      EnterDeviceNameStage() => _buildEnterDeviceNameStep(),
      GeneratingNoncesStage() => _buildGeneratingNoncesStep(),
      CompletingDeviceShareEnrollmentStage() =>
        _buildCompletingDeviceShareEnrollmentStep(),
      EnterBackupStage(:final deviceName) => _buildEnterBackupStep(deviceName),
      StartRestorationWithPhysicalBackupStage() => _buildStartRestorationStep(),
      WaitReconnectDeviceStage(:final nextStage) => _buildWaitReconnectStep(
        nextStage,
      ),
      EnterRestorationDetailsStage() => _buildEnterRestorationDetailsStep(),
      EnterThresholdStage(:final walletName, :final network) =>
        _buildEnterThresholdStep(walletName, network),
      PhysicalBackupSuccessStage(:final deviceName) =>
        _buildPhysicalBackupSuccessStep(deviceName),
      ErrorStage(
        :final title,
        :final message,
        :final isWarning,
        :final returnStage,
      ) =>
        _buildErrorStep(title, message, isWarning, returnStage),
    };
  }

  Widget _backLeading() => IconButton(
    icon: const Icon(Icons.arrow_back_rounded),
    onPressed: _goBackOrClose,
    tooltip: 'Back',
  );

  MultiStepDialogScaffold _buildCandidateReadyStep(RecoverShare candidate) {
    final addingToExisting = switch (_ctrl.recoveryContext) {
      ContinuingRestorationContext() => true,
      AddingToWalletContext() => true,
      NewRestorationContext() => false,
    };
    return MultiStepDialogScaffold(
      stepKey: 'candidateReady',
      title: const Text('Restore with existing key'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: CandidateReadyView(
          candidate: candidate,
          addingToExisting: addingToExisting,
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton(
          onPressed: () => _ctrl.confirmCandidate(candidate),
          child: Text(addingToExisting ? 'Add to wallet' : 'Restore'),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildFirmwareUpgradeStep() {
    return MultiStepDialogScaffold(
      stepKey: 'firmwareUpgrade',
      title: const Text('Firmware Upgrade Required'),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: const SliverToBoxAdapter(child: FirmwareUpgradeView()),
      footer: Row(
        children: [
          TextButton(
            onPressed: _isUpgrading ? null : () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          const Spacer(),
          FilledButton(
            onPressed: _isUpgrading ? null : _runFirmwareUpgrade,
            child: Text(_isUpgrading ? 'Upgrading…' : 'Upgrade Now'),
          ),
        ],
      ),
    );
  }

  Future<void> _runFirmwareUpgrade() async {
    final ctrl = _firmwareUpgradeController;
    if (ctrl == null) return;
    setState(() => _isUpgrading = true);
    final stageBeforeRun = _ctrl.flowState.stage;
    final success = await ctrl.run(context);
    if (!mounted) return;
    // If the controller's disconnect listener already transitioned us
    // off the firmware stage during the upgrade, don't second-guess
    // it with success/failure handling here.
    if (!identical(_ctrl.flowState.stage, stageBeforeRun)) return;
    if (success) {
      _ctrl.firmwareUpgradeCompleted();
    } else {
      Navigator.of(context).pop();
    }
  }

  MultiStepDialogScaffold _buildEnterDeviceNameStep() {
    return MultiStepDialogScaffold(
      stepKey: 'enterDeviceName',
      title: const Text('Device name'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: EnterDeviceNameView(
          deviceId: _ctrl.targetDevice.id,
          onChanged: (name, canSubmit) {
            if (name == _enterDeviceNameCurrent &&
                canSubmit == _enterDeviceNameCanSubmit) {
              return;
            }
            setState(() {
              _enterDeviceNameCurrent = name;
              _enterDeviceNameCanSubmit = canSubmit;
            });
          },
          onSubmit: _submitDeviceName,
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton(
          onPressed: _enterDeviceNameCanSubmit ? _submitDeviceName : null,
          child: const Text('Continue'),
        ),
      ),
    );
  }

  void _submitDeviceName() {
    if (!_enterDeviceNameCanSubmit) return;
    _ctrl.submitDeviceName(_enterDeviceNameCurrent);
  }

  MultiStepDialogScaffold _buildGeneratingNoncesStep() {
    final stream = _ctrl.nonceStream;
    return MultiStepDialogScaffold(
      stepKey: 'generatingNonces',
      title: const Text('Preparing Device'),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      // Natural-height sliver; `SliverFillRemaining` stretches this step.
      body: SliverToBoxAdapter(
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 32),
          child: stream == null
              ? const Center(child: CircularProgressIndicator())
              : NonceReplenishIndicator(
                  stream: stream,
                  onTerminal: _onNonceTerminal,
                ),
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: TextButton(
          onPressed: _ctrl.cancelGeneratingNonces,
          child: const Text('Cancel'),
        ),
      ),
    );
  }

  void _onNonceTerminal(NonceReplenishTerminal terminal) {
    _ctrl.onNonceTerminal(terminal);
  }

  MultiStepDialogScaffold _buildCompletingDeviceShareEnrollmentStep() {
    return const MultiStepDialogScaffold(
      stepKey: 'completingDeviceShareEnrollment',
      title: Text('Completing enrollment…'),
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: Padding(
          padding: EdgeInsets.symmetric(vertical: 32),
          child: Center(child: CircularProgressIndicator()),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildEnterBackupStep(String deviceName) {
    return MultiStepDialogScaffold(
      stepKey: 'enterBackup',
      title: const Text('Enter backup on device'),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(child: EnterBackupView(deviceName: deviceName)),
    );
  }

  MultiStepDialogScaffold _buildStartRestorationStep() {
    return MultiStepDialogScaffold(
      stepKey: 'startRestorationWithPhysicalBackup',
      title: const Text('Found blank device for backup entry'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: StartRestorationInfoView(recoveryContext: _ctrl.recoveryContext),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton(
          onPressed: _ctrl.confirmStartRestoration,
          child: Text(switch (_ctrl.recoveryContext) {
            NewRestorationContext() => 'Begin restoration',
            _ => 'Next',
          }),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildWaitReconnectStep(RecoveryFlowStage nextStage) {
    return MultiStepDialogScaffold(
      stepKey: 'waitReconnect',
      title: const Text('Device Disconnected'),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: WaitReconnectDeviceView(
          targetDevice: _ctrl.targetDevice,
          onReconnected: () => _ctrl.deviceReconnected(nextStage),
        ),
      ),
      footer: Align(
        alignment: Alignment.center,
        child: OutlinedButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildEnterRestorationDetailsStep() {
    final initialName = switch (_ctrl.recoveryContext) {
      NewRestorationContext(:final walletName) => walletName,
      _ => null,
    };
    final initialNetwork = switch (_ctrl.recoveryContext) {
      NewRestorationContext(:final network) => network,
      _ => null,
    };
    return MultiStepDialogScaffold(
      stepKey: 'enterRestorationDetails',
      title: const Text('Wallet name'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: EnterWalletNameView(
          key: _enterWalletNameKey,
          initialWalletName: initialName,
          initialBitcoinNetwork: initialNetwork,
          onChanged: (canSubmit) {
            if (canSubmit != _enterWalletNameCanSubmit) {
              setState(() => _enterWalletNameCanSubmit = canSubmit);
            }
          },
          onSubmit: _ctrl.submitWalletName,
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton(
          onPressed: _enterWalletNameCanSubmit
              ? () => _enterWalletNameKey.currentState?.submit()
              : null,
          child: const Text('Continue'),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildEnterThresholdStep(
    String walletName,
    BitcoinNetwork network,
  ) {
    final initialThreshold = switch (_ctrl.recoveryContext) {
      NewRestorationContext(:final threshold) => threshold,
      _ => null,
    };
    return MultiStepDialogScaffold(
      stepKey: 'enterThreshold',
      title: const Text('Wallet Threshold (Optional)'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: EnterThresholdView(
          key: _enterThresholdKey,
          initialThreshold: initialThreshold,
          onSubmit: _ctrl.submitThreshold,
        ),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton(
          onPressed: () => _enterThresholdKey.currentState?.submit(),
          child: const Text('Continue'),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildPhysicalBackupSuccessStep(String deviceName) {
    final restorationId = switch (_ctrl.recoveryContext) {
      NewRestorationContext(:final restorationId) => restorationId,
      ContinuingRestorationContext(:final restorationId) => restorationId,
      _ => null,
    };
    return MultiStepDialogScaffold(
      stepKey: 'physicalBackupSuccess',
      title: const Text(''),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: PhysicalBackupSuccessView(deviceName: deviceName),
      ),
      footer: Align(
        alignment: Alignment.centerRight,
        child: FilledButton.icon(
          icon: const Icon(Icons.arrow_forward),
          label: const Text('Close'),
          onPressed: () => Navigator.of(context).pop(restorationId),
        ),
      ),
    );
  }

  MultiStepDialogScaffold _buildErrorStep(
    String title,
    String message,
    bool isWarning,
    RecoveryFlowStage returnStage,
  ) {
    final theme = Theme.of(context);
    return MultiStepDialogScaffold(
      stepKey: 'error',
      title: Text(isWarning ? 'Warning' : 'Error'),
      leading: _backLeading(),
      forward: _ctrl.isAnimationForward,
      reverseDuration: Duration.zero,
      body: SliverToBoxAdapter(
        child: ErrorView(title: title, message: message, isWarning: isWarning),
      ),
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
            onPressed: () => _ctrl.retryFromError(returnStage),
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
