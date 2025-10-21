import 'package:freezed_annotation/freezed_annotation.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';

part 'state.freezed.dart';

/// Represents the context of the recovery flow - what we're trying to accomplish
@freezed
sealed class RecoveryContext with _$RecoveryContext {
  const RecoveryContext._();

  /// Starting a brand new restoration from scratch
  /// Fields get filled in as we collect them during the flow
  const factory RecoveryContext.newRestoration({
    String? walletName,
    BitcoinNetwork? network,
    int? threshold,
  }) = NewRestorationContext;

  /// Continuing an existing restoration that's in progress
  const factory RecoveryContext.continuingRestoration({
    required RestorationId restorationId,
    required String walletName,
    required BitcoinNetwork network,
    int? threshold,
  }) = ContinuingRestorationContext;

  /// Adding a share to an already complete wallet
  const factory RecoveryContext.addingToWallet({
    required AccessStructureRef accessStructureRef,
  }) = AddingToWalletContext;
}

/// Represents all possible states in the wallet recovery flow.
/// Each state contains exactly the data it needs, ensuring type safety.
@freezed
sealed class RecoveryFlowState with _$RecoveryFlowState {
  const RecoveryFlowState._();

  /// Initial state when starting any recovery flow
  const factory RecoveryFlowState.start() = StartState;

  // ============ Device Share Flow States ============

  /// Waiting for a device with an existing share to be connected
  const factory RecoveryFlowState.waitDevice() = WaitDeviceState;

  /// A candidate device share has been detected and is ready for confirmation
  const factory RecoveryFlowState.candidateReady({
    required RecoverShare candidate,
    required TargetDevice targetDevice,
  }) = CandidateReadyState;

  // ============ Physical Backup Flow States ============

  /// Waiting for a blank device to be connected for physical backup entry
  const factory RecoveryFlowState.waitPhysicalBackupDevice() =
      WaitPhysicalBackupDeviceState;

  /// Device needs firmware upgrade before it can be used
  const factory RecoveryFlowState.firmwareUpgrade({
    required TargetDevice targetDevice,
  }) = FirmwareUpgradeState;

  /// Prompting user to enter a name for the device
  const factory RecoveryFlowState.enterDeviceName({
    required TargetDevice targetDevice,
  }) = EnterDeviceNameState;

  // ============ Shared Flow States ============

  /// Generating nonces on the device
  const factory RecoveryFlowState.generatingNonces({
    required TargetDevice targetDevice,

    /// The state to transition to after nonce generation completes
    required RecoveryFlowState nextState,
  }) = GeneratingNoncesState;

  /// Completing device share enrollment (called after nonce generation for share flow)
  const factory RecoveryFlowState.completingDeviceShareEnrollment({
    required RecoverShare candidate,
  }) = CompletingDeviceShareEnrollmentState;

  // ============ Physical Backup Entry State ============

  /// Device is entering physical backup mode
  /// The context determines what happens with the backup (new restoration, continuing, or adding to wallet)
  const factory RecoveryFlowState.enterBackup({
    required TargetDevice targetDevice,
    required String deviceName,
  }) = EnterBackupState;

  /// User is entering restoration details (wallet name, network)
  /// This happens BEFORE any physical backup is entered
  const factory RecoveryFlowState.enterRestorationDetails() =
      EnterRestorationDetailsState;

  /// User is setting the threshold for restoration
  const factory RecoveryFlowState.enterThreshold({
    required String walletName,
    required BitcoinNetwork network,
  }) = EnterThresholdState;

  /// Physical backup has been successfully saved
  const factory RecoveryFlowState.physicalBackupSuccess({
    required RestorationId restorationId,
    required String deviceName,
  }) = PhysicalBackupSuccessState;

  /// Error state - shows an error message and returns to a previous state on dismiss
  const factory RecoveryFlowState.error({
    required String title,
    required String message,
    required bool isWarning,
    required RecoveryFlowState returnState,
  }) = ErrorState;

  // ============ Helper Methods ============

  /// Returns the target device if this state has one
  TargetDevice? get targetDevice => switch (this) {
    CandidateReadyState(:final targetDevice) => targetDevice,
    FirmwareUpgradeState(:final targetDevice) => targetDevice,
    EnterDeviceNameState(:final targetDevice) => targetDevice,
    GeneratingNoncesState(:final targetDevice) => targetDevice,
    EnterBackupState(:final targetDevice) => targetDevice,
    ErrorState(:final returnState) => returnState.targetDevice,
    _ => null,
  };

  /// Dispose any resources owned by this state
  void dispose() {
    targetDevice?.dispose();
  }
}
