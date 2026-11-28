import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:rxdart/rxdart.dart';

class RecoveryProtocol {
  const RecoveryProtocol();

  bool noncesNeeded(DeviceId device) =>
      coord.createNonceRequest(devices: [device]).someNoncesRequested();

  Stream<NonceReplenishState> replenishNonces(DeviceId device) =>
      coord.replenishNonces(
        nonceRequest: coord.createNonceRequest(devices: [device]),
        devices: [device],
      );

  void cancelProtocol() => coord.cancelProtocol();

  Stream<EnterPhysicalBackupState> enterPhysicalBackup(DeviceId device) =>
      coord.tellDeviceToEnterPhysicalBackup(deviceId: device);

  Future<RestorationId> startRestoringWallet({
    required String name,
    required int? threshold,
    required BitcoinNetwork network,
  }) => coord.startRestoringWallet(
    name: name,
    threshold: threshold,
    network: network,
  );

  Future<RestorePhysicalBackupError?> checkPhysicalBackupForRestoration({
    required RestorationId restorationId,
    required PhysicalBackupPhase phase,
  }) async => coord.checkPhysicalBackupForRestoration(
    restorationId: restorationId,
    phase: phase,
    encryptionKey: await SecureKeyProvider.getEncryptionKey(),
  );

  Future<void> tellDeviceToSavePhysicalBackup({
    required PhysicalBackupPhase phase,
    required RestorationId restorationId,
    required String deviceName,
  }) => coord.tellDeviceToSavePhysicalBackup(
    phase: phase,
    restorationId: restorationId,
    deviceName: deviceName,
  );

  Future<void> cancelRestoration(RestorationId restorationId) =>
      coord.cancelRestoration(restorationId: restorationId);
}

/// Resolves the key of an existing wallet, or returns null once it has told
/// the user why it couldn't.
typedef ExistingWalletKey =
    Future<SymmetricKey?> Function(
      AccessStructureRef accessStructureRef,
      String action,
    );

sealed class RecoveryFlowOutcome {
  const RecoveryFlowOutcome();
}

final class RecoveryFlowFinished extends RecoveryFlowOutcome {
  const RecoveryFlowFinished(this.restorationId);
  final RestorationId? restorationId;
}

final class RecoveryFlowDismissed extends RecoveryFlowOutcome {
  const RecoveryFlowDismissed();
}

sealed class BackupEntryResult {
  const BackupEntryResult();
}

final class BackupEntered extends BackupEntryResult {
  const BackupEntered(this.phase);
  final PhysicalBackupPhase phase;
}

final class BackupAborted extends BackupEntryResult {
  const BackupAborted(this.message);
  final String message;
}

/// Protocol work starts only on entering a stage and stops on leaving it. Once
/// [outcome] is set the flow is over and ignores every further call.
class RecoveryFlowController extends ChangeNotifier {
  RecoveryFlowController({
    required RecoveryContext recoveryContext,
    required this.targetDevice,
    required RecoveryFlowStage initialStage,
    required ExistingWalletKey existingWalletKey,
    RecoveryProtocol protocol = const RecoveryProtocol(),
  }) : _recoveryContext = recoveryContext,
       _stage = initialStage,
       _existingWalletKey = existingWalletKey,
       _protocol = protocol {
    _enterStage(initialStage);
  }

  final TargetDevice targetDevice;
  final ExistingWalletKey _existingWalletKey;
  final RecoveryProtocol _protocol;

  RecoveryContext _recoveryContext;
  RecoveryContext get recoveryContext => _recoveryContext;

  RecoveryFlowStage _stage;
  RecoveryFlowStage get stage => _stage;

  /// Bumped on every stage entry so async work started by an earlier entry
  /// can tell it no longer owns the flow.
  int _entry = 0;

  final List<RecoveryFlowStage> _backStack = [];

  bool _forward = true;
  bool get isAnimationForward => _forward;

  RecoveryFlowOutcome? _outcome;
  RecoveryFlowOutcome? get outcome => _outcome;

  /// Owned by the current stage entry and cancelled on leaving it, so an
  /// entry's device watches and protocol streams never outlive it.
  final List<StreamSubscription<Object?>> _entrySubscriptions = [];

  BehaviorSubject<NonceReplenishState>? _nonceStream;
  ValueStream<NonceReplenishState>? get nonceStream => _nonceStream;
  bool _nonceTerminalHandled = false;

  /// Set when the device reports the backup entry. The view must dismiss its
  /// root-navigator prompt before calling [completeBackupEntry], or popping
  /// this flow's dialog would pop the prompt instead.
  BackupEntryResult? _pendingBackupEntry;
  BackupEntryResult? get pendingBackupEntry => _pendingBackupEntry;
  bool _savingBackup = false;

  bool _firmwareUpgrading = false;
  bool get firmwareUpgrading => _firmwareUpgrading;

  bool _disposed = false;

  @override
  void notifyListeners() {
    if (_disposed) return;
    super.notifyListeners();
  }

  void _go(RecoveryFlowStage next, {required bool forward}) {
    if (_disposed || _outcome != null) return;
    _leaveStage(_stage);
    _forward = forward;
    _stage = next;
    _enterStage(next);
    notifyListeners();
  }

  void _push(RecoveryFlowStage next) {
    if (_disposed || _outcome != null) return;
    _backStack.add(_stage);
    _go(next, forward: true);
  }

  bool _pop() {
    if (_backStack.isEmpty) return false;
    _go(_backStack.removeLast(), forward: false);
    return true;
  }

  void _end(RecoveryFlowOutcome outcome) {
    if (_disposed || _outcome != null) return;
    _leaveStage(_stage);
    _outcome = outcome;
    notifyListeners();
  }

  void _watchConnection({
    required bool connected,
    required void Function() action,
  }) {
    _entrySubscriptions.add(
      targetDevice.connected
          .where((c) => c == connected)
          .take(1)
          .listen((_) => action()),
    );
  }

  void _enterStage(RecoveryFlowStage stage) {
    _entry++;
    switch (stage) {
      case FirmwareUpgradeStage():
        _watchConnection(
          connected: false,
          action: () {
            // The upgrade reboots the device; DeviceActionUpgradeController
            // owns that disconnect.
            if (_firmwareUpgrading) return;
            _go(
              const RecoveryFlowStage.waitReconnectDevice(
                nextStage: RecoveryFlowStage.firmwareUpgrade(),
              ),
              forward: true,
            );
          },
        );
      case EnterDeviceNameStage():
        _watchConnection(
          connected: false,
          action: () => _go(
            const RecoveryFlowStage.waitReconnectDevice(
              nextStage: RecoveryFlowStage.enterDeviceName(),
            ),
            forward: true,
          ),
        );
      case GeneratingNoncesStage(:final nextStage):
        _nonceTerminalHandled = false;
        final nonceStream = BehaviorSubject<NonceReplenishState>();
        _nonceStream = nonceStream;
        _entrySubscriptions.add(
          _protocol
              .replenishNonces(targetDevice.id)
              .listen(nonceStream.add, onError: nonceStream.addError),
        );
        _watchConnection(
          connected: false,
          action: () => _go(
            RecoveryFlowStage.waitReconnectDevice(
              nextStage: RecoveryFlowStage.generatingNonces(
                nextStage: nextStage,
              ),
            ),
            forward: true,
          ),
        );
      case CompletingDeviceShareEnrollmentStage(:final candidate):
        unawaited(_completeDeviceShareEnrollment(candidate, _entry));
      case EnterBackupStage():
        _entrySubscriptions.add(
          _protocol.enterPhysicalBackup(targetDevice.id).listen(_onBackupEvent),
        );
      case WaitReconnectDeviceStage(:final nextStage):
        _watchConnection(
          connected: true,
          action: () => _go(nextStage, forward: true),
        );
      case CandidateReadyStage():
      case StartRestorationWithPhysicalBackupStage():
      case EnterRestorationDetailsStage():
      case EnterThresholdStage():
      case PhysicalBackupSuccessStage():
      case ErrorStage():
        break;
    }
  }

  void _leaveStage(RecoveryFlowStage stage) {
    for (final subscription in _entrySubscriptions) {
      subscription.cancel();
    }
    _entrySubscriptions.clear();
    switch (stage) {
      case GeneratingNoncesStage():
        // Leaving before the replenish reached a terminal (back, a
        // disconnect, the dialog closing) must not leave the device's
        // protocol session open.
        if (!_nonceTerminalHandled) _protocol.cancelProtocol();
        _nonceStream?.close();
        _nonceStream = null;
      case EnterBackupStage():
        _pendingBackupEntry = null;
        _savingBackup = false;
      case FirmwareUpgradeStage():
        _firmwareUpgrading = false;
      default:
        break;
    }
  }

  void _popOnError({
    required String title,
    required String message,
    bool isException = false,
  }) {
    final returnStage = _backStack.isNotEmpty
        ? _backStack.removeLast()
        : _stage;
    _go(
      RecoveryFlowStage.error(
        title: title,
        message: message,
        isWarning: !isException,
        returnStage: returnStage,
      ),
      forward: false,
    );
  }

  RecoveryFlowStage get _afterFirmwareUpgrade => switch (_recoveryContext) {
    NewRestorationContext() =>
      const RecoveryFlowStage.enterRestorationDetails(),
    _ => const RecoveryFlowStage.enterDeviceName(),
  };

  RestorationId? get _restorationId => switch (_recoveryContext) {
    NewRestorationContext(:final restorationId) => restorationId,
    ContinuingRestorationContext(:final restorationId) => restorationId,
    AddingToWalletContext() => null,
  };

  /// The user's back gesture. Stages with protocol work in flight ignore it.
  void back() {
    switch (_stage) {
      case CompletingDeviceShareEnrollmentStage():
        return;
      case EnterBackupStage() when _savingBackup:
        return;
      case FirmwareUpgradeStage() when _firmwareUpgrading:
        return;
      case WaitReconnectDeviceStage():
        dismiss();
        return;
      case PhysicalBackupSuccessStage():
        finish();
        return;
      default:
        break;
    }
    if (!_pop()) dismiss();
  }

  void dismiss() => _end(const RecoveryFlowDismissed());

  void finish() => _end(RecoveryFlowFinished(_restorationId));

  void confirmCandidate() {
    final stage = _stage;
    if (stage is! CandidateReadyStage) return;
    _push(
      RecoveryFlowStage.generatingNonces(
        nextStage: RecoveryFlowStage.completingDeviceShareEnrollment(
          candidate: stage.candidate,
        ),
      ),
    );
  }

  void confirmStartRestoration() {
    if (_stage is! StartRestorationWithPhysicalBackupStage) return;
    _push(
      targetDevice.needsFirmwareUpgrade()
          ? const RecoveryFlowStage.firmwareUpgrade()
          : _afterFirmwareUpgrade,
    );
  }

  void firmwareUpgradeStarted() {
    if (_stage is! FirmwareUpgradeStage) return;
    _firmwareUpgrading = true;
    notifyListeners();
  }

  void firmwareUpgradeFinished({required bool success}) {
    if (_stage is! FirmwareUpgradeStage) return;
    _firmwareUpgrading = false;
    if (success) {
      _go(_afterFirmwareUpgrade, forward: true);
    } else {
      dismiss();
    }
  }

  void submitWalletName(String walletName, BitcoinNetwork network) {
    if (_stage is! EnterRestorationDetailsStage) return;
    if (_recoveryContext case NewRestorationContext()) {
      _recoveryContext = RecoveryContext.newRestoration(
        walletName: walletName,
        network: network,
      );
    }
    _push(
      RecoveryFlowStage.enterThreshold(
        walletName: walletName,
        network: network,
      ),
    );
  }

  void submitThreshold(int? threshold) {
    if (_stage is! EnterThresholdStage) return;
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
    _push(const RecoveryFlowStage.enterDeviceName());
  }

  void submitDeviceName(String deviceName) {
    if (_stage is! EnterDeviceNameStage) return;
    final enterBackup = RecoveryFlowStage.enterBackup(deviceName: deviceName);
    _push(
      _protocol.noncesNeeded(targetDevice.id)
          ? RecoveryFlowStage.generatingNonces(nextStage: enterBackup)
          : enterBackup,
    );
  }

  void onNonceTerminal(NonceReplenishTerminal terminal) {
    final stage = _stage;
    if (stage is! GeneratingNoncesStage || _nonceTerminalHandled) return;
    _nonceTerminalHandled = true;
    switch (terminal) {
      case NonceReplenishCompleted():
        _go(stage.nextStage, forward: true);
      case NonceReplenishAborted():
        _popOnError(
          title: 'Nonce Generation Failed',
          message: 'Device disconnected during preparation',
        );
      case NonceReplenishFailed(:final error):
        _popOnError(
          title: 'Nonce Generation Failed',
          message: 'Failed to prepare device: $error',
        );
    }
  }

  void retryFromError() {
    final stage = _stage;
    if (stage is! ErrorStage) return;
    _go(stage.returnStage, forward: true);
  }

  void _onBackupEvent(EnterPhysicalBackupState state) {
    if (_pendingBackupEntry != null) return;
    final entered = state.entered;
    final abort = state.abort;
    if (entered != null) {
      _pendingBackupEntry = BackupEntered(entered);
    } else if (abort != null) {
      _pendingBackupEntry = BackupAborted(abort);
    } else {
      return;
    }
    notifyListeners();
  }

  Future<void> completeBackupEntry() async {
    final stage = _stage;
    final result = _pendingBackupEntry;
    if (stage is! EnterBackupStage || result == null || _savingBackup) return;
    switch (result) {
      case BackupAborted(:final message):
        _pendingBackupEntry = null;
        _popOnError(title: 'Backup Entry Failed', message: message);
      case BackupEntered(:final phase):
        _savingBackup = true;
        await _saveBackup(phase, stage.deviceName, _entry);
    }
  }

  Future<void> _saveBackup(
    PhysicalBackupPhase phase,
    String deviceName,
    int entry,
  ) async {
    bool current() => !_disposed && _outcome == null && entry == _entry;
    try {
      switch (_recoveryContext) {
        case AddingToWalletContext(:final accessStructureRef):
          final encryptionKey = await _existingWalletKey(
            accessStructureRef,
            'add this backup to the wallet',
          );
          if (!current()) return;
          if (encryptionKey == null) {
            dismiss();
            return;
          }
          final isValid = await coord.checkPhysicalBackup(
            accessStructureRef: accessStructureRef,
            phase: phase,
            encryptionKey: encryptionKey,
          );
          if (!current()) return;
          if (!isValid) {
            _popOnError(
              title: 'Cannot add backup',
              message: 'The backup is not compatible with this wallet',
            );
            return;
          }
          await coord.tellDeviceToConsolidatePhysicalBackup(
            accessStructureRef: accessStructureRef,
            phase: phase,
            encryptionKey: encryptionKey,
            deviceName: deviceName,
          );
          finish();

        case NewRestorationContext(
          :final walletName,
          :final network,
          :final threshold,
        ):
          final restorationId = await _protocol.startRestoringWallet(
            name: walletName!,
            threshold: threshold,
            network: network!,
          );
          // The coordinator sends the save to the device before its future
          // can fail, so once the save is invoked the device may hold a share
          // in this restoration. Only an exit before that point cancels it.
          var saveInvoked = false;
          try {
            final error = await _protocol.checkPhysicalBackupForRestoration(
              restorationId: restorationId,
              phase: phase,
            );
            if (error != null) {
              if (current()) {
                _popOnError(
                  title: 'Cannot add backup',
                  message: error.toString(),
                );
              }
              return;
            }
            if (!current()) return;
            saveInvoked = true;
            await _protocol.tellDeviceToSavePhysicalBackup(
              phase: phase,
              restorationId: restorationId,
              deviceName: deviceName,
            );
          } finally {
            if (!saveInvoked) await _protocol.cancelRestoration(restorationId);
          }
          if (!current()) return;
          _recoveryContext = (_recoveryContext as NewRestorationContext)
              .copyWith(restorationId: restorationId);
          _go(
            RecoveryFlowStage.physicalBackupSuccess(deviceName: deviceName),
            forward: true,
          );

        case ContinuingRestorationContext(:final restorationId):
          final error = await _protocol.checkPhysicalBackupForRestoration(
            restorationId: restorationId,
            phase: phase,
          );
          if (!current()) return;
          if (error != null) {
            _popOnError(title: 'Cannot add backup', message: error.toString());
            return;
          }
          await _protocol.tellDeviceToSavePhysicalBackup(
            phase: phase,
            restorationId: restorationId,
            deviceName: deviceName,
          );
          if (!current()) return;
          _go(
            RecoveryFlowStage.physicalBackupSuccess(deviceName: deviceName),
            forward: true,
          );
      }
    } catch (e, stackTrace) {
      if (!current()) return;
      _popOnError(
        title: 'Failed to save backup',
        message: '$e\n\nStack trace:\n$stackTrace',
        isException: true,
      );
    }
  }

  Future<void> _completeDeviceShareEnrollment(
    RecoverShare candidate,
    int entry,
  ) async {
    bool current() => !_disposed && _outcome == null && entry == _entry;
    try {
      switch (_recoveryContext) {
        case ContinuingRestorationContext(:final restorationId):
          // Continuing establishes the finished key rather than decrypting
          // existing data, so it uses the establish surface.
          final encryptionKey = await SecureKeyProvider.getEncryptionKey();
          await coord.continueRestoringWalletFromDeviceShare(
            restorationId: restorationId,
            recoverShare: candidate,
            encryptionKey: encryptionKey,
          );
          if (current()) _end(const RecoveryFlowFinished(null));
        case AddingToWalletContext(:final accessStructureRef):
          final encryptionKey = await _existingWalletKey(
            accessStructureRef,
            'recover this key into the wallet',
          );
          if (!current()) return;
          if (encryptionKey == null) {
            dismiss();
            return;
          }
          await coord.recoverShare(
            accessStructureRef: accessStructureRef,
            recoverShare: candidate,
            encryptionKey: encryptionKey,
          );
          if (current()) _end(const RecoveryFlowFinished(null));
        case NewRestorationContext():
          final restorationId = await coord.startRestoringWalletFromDeviceShare(
            recoverShare: candidate,
          );
          if (current()) _end(RecoveryFlowFinished(restorationId));
      }
    } catch (e, stackTrace) {
      if (!current()) return;
      _popOnError(
        title: 'Unexpected error',
        message: '$e\n\nStack trace:\n$stackTrace',
        isException: true,
      );
    }
  }

  @override
  void dispose() {
    if (_outcome == null) _leaveStage(_stage);
    _disposed = true;
    super.dispose();
  }
}
