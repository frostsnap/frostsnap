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
    RestorationId? restorationId,
    String? walletName,
    BitcoinNetwork? network,
    int? threshold,
  }) = NewRestorationContext;

  /// Continuing an existing restoration that's in progress
  const factory RecoveryContext.continuingRestoration({
    required RestorationId restorationId,
  }) = ContinuingRestorationContext;

  /// Adding a share to an already complete wallet
  const factory RecoveryContext.addingToWallet({
    required AccessStructureRef accessStructureRef,
  }) = AddingToWalletContext;
}

/// Composite state representing the recovery flow
/// Always has a targetDevice and a stage
class RecoveryFlowState {
  final TargetDevice targetDevice;
  RecoveryFlowStage stage;

  RecoveryFlowState({required this.targetDevice, required this.stage});

  /// Dispose resources owned by this state
  void dispose() {
    targetDevice.dispose();
  }
}

/// Represents the stage/step in the wallet recovery flow
/// Each stage contains exactly the data it needs, ensuring type safety
@freezed
sealed class RecoveryFlowStage with _$RecoveryFlowStage {
  const RecoveryFlowStage._();

  // ============ Device Share Flow Stages ============

  /// A candidate device share has been detected and is ready for confirmation
  const factory RecoveryFlowStage.candidateReady({
    required RecoverShare candidate,
  }) = CandidateReadyStage;

  // ============ Physical Backup Flow Stages ============

  /// Device needs firmware upgrade before it can be used
  const factory RecoveryFlowStage.firmwareUpgrade() = FirmwareUpgradeStage;

  /// Prompting user to enter a name for the device
  const factory RecoveryFlowStage.enterDeviceName() = EnterDeviceNameStage;

  // ============ Shared Flow Stages ============

  /// Generating nonces on the device
  const factory RecoveryFlowStage.generatingNonces({
    /// The stage to transition to after nonce generation completes
    required RecoveryFlowStage nextStage,
  }) = GeneratingNoncesStage;

  /// Completing device share enrollment (called after nonce generation for share flow)
  const factory RecoveryFlowStage.completingDeviceShareEnrollment({
    required RecoverShare candidate,
  }) = CompletingDeviceShareEnrollmentStage;

  // ============ Physical Backup Flow Stages ============

  /// Starting a new restoration with a physical backup
  /// Shows explanation that wallet details are needed first before loading backup
  /// Only shown for new restorations, not for continuing or adding to wallet
  const factory RecoveryFlowStage.startRestorationWithPhysicalBackup() =
      StartRestorationWithPhysicalBackupStage;

  /// User is entering restoration details (wallet name, network)
  /// For new restorations with physical backup, this happens before loading backup
  const factory RecoveryFlowStage.enterRestorationDetails() =
      EnterRestorationDetailsStage;

  /// Device is entering physical backup mode
  /// The context determines what happens with the backup (new restoration, continuing, or adding to wallet)
  const factory RecoveryFlowStage.enterBackup({required String deviceName}) =
      EnterBackupStage;

  /// User is setting the threshold for restoration
  /// This happens after wallet name/network entry for new restorations
  const factory RecoveryFlowStage.enterThreshold({
    required String walletName,
    required BitcoinNetwork network,
  }) = EnterThresholdStage;

  /// Physical backup has been successfully loaded
  /// For new restorations, this transitions to wallet metadata entry
  const factory RecoveryFlowStage.physicalBackupSuccess({
    required String deviceName,
  }) = PhysicalBackupSuccessStage;

  /// Waiting for device to reconnect after disconnection
  const factory RecoveryFlowStage.waitReconnectDevice({
    required RecoveryFlowStage nextStage,
  }) = WaitReconnectDeviceStage;

  /// Error stage - shows an error message and returns to a previous stage on dismiss
  const factory RecoveryFlowStage.error({
    required String title,
    required String message,
    required bool isWarning,
    required RecoveryFlowStage returnStage,
  }) = ErrorStage;
}
