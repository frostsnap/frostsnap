import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/recovery/remote_recovery_page.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/src/rust/lib.dart';

// Pins the remote-recovery nonce invariant: a share cannot complete
// the remote-lobby flow without the generatingNonces stage having run
// (device-share path) or having confirmed nothing was needed
// (physical-backup path). Devices without nonce streams on record
// cannot sign (`NotEnoughNoncesForDevice`), so a share reaching the
// lobby implies its device was replenished at load time. The
// controller's protocol seams are injected — no live coordinator.
//
// Also unit-tests `nonceTopUpDevices`, the persist-time top-up's
// device-set filter: it must exclude access-structure devices that
// are not physically present (other participants' hardware), or the
// replenish protocol would wait forever for devices that never
// connect to this app.

DeviceId devId(int b) {
  final bytes = Uint8List(33);
  bytes[0] = b;
  return DeviceId(field0: U8Array33(bytes));
}

class _FakeTargetDevice extends TargetDevice {
  _FakeTargetDevice() : super(devId(1));

  @override
  ConnectedDevice? get device => null;

  @override
  bool needsFirmwareUpgrade() => false;

  @override
  Future<void> onDisconnected() => Completer<void>().future;

  @override
  Future<void> waitForReconnection() => Completer<void>().future;
}

/// Only carried through the flow, never invoked on — the lobby posts
/// it after completion.
class _StubRecoverShare implements RecoverShare {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

RecoveryFlowController makeController({
  required RecoveryFlowStage stage,
  required bool Function(List<DeviceId>) noncesNeeded,
  VoidCallback? cancelProtocol,
}) => RecoveryFlowController(
  recoveryContext: const RecoveryContext.remoteLobby(),
  flowState: RecoveryFlowState(targetDevice: _FakeTargetDevice(), stage: stage),
  noncesNeeded: noncesNeeded,
  replenishNonces: (_) => const Stream<NonceReplenishState>.empty(),
  cancelProtocol: cancelProtocol ?? () {},
  enterPhysicalBackup: (_) => const Stream<EnterPhysicalBackupState>.empty(),
);

void main() {
  test('device-share path: generatingNonces precedes completion', () async {
    final share = _StubRecoverShare();
    final ctrl = makeController(
      stage: RecoveryFlowStage.candidateReady(candidate: share),
      noncesNeeded: (_) => true,
    );

    ctrl.confirmCandidate(share);
    final stage = ctrl.flowState.stage;
    expect(stage, isA<GeneratingNoncesStage>());
    expect(
      (stage as GeneratingNoncesStage).nextStage,
      isA<CompletingDeviceShareEnrollmentStage>(),
    );
    expect(ctrl.completed, isFalse);
    expect(ctrl.completionResult, isNull);

    ctrl.onNonceTerminal(const NonceReplenishCompleted());
    await pumpEventQueue();
    expect(ctrl.completed, isTrue);
    final result = ctrl.completionResult;
    expect(result, isA<RemoteShareResultDeviceShare>());
    expect((result as RemoteShareResultDeviceShare).share, same(share));

    ctrl.dispose();
  });

  test('device-share path: nonce abort never completes the flow', () async {
    final share = _StubRecoverShare();
    final ctrl = makeController(
      stage: RecoveryFlowStage.candidateReady(candidate: share),
      noncesNeeded: (_) => true,
    );

    ctrl.confirmCandidate(share);
    ctrl.onNonceTerminal(const NonceReplenishAborted());
    await pumpEventQueue();
    expect(ctrl.flowState.stage, isA<ErrorStage>());
    expect(ctrl.completed, isFalse);
    expect(ctrl.completionResult, isNull);

    ctrl.dispose();
  });

  test('physical-backup path: nonces needed interposes generatingNonces '
      'before backup entry', () {
    final ctrl = makeController(
      stage: const RecoveryFlowStage.enterDeviceName(),
      noncesNeeded: (_) => true,
    );

    ctrl.submitDeviceName('frosty');
    final stage = ctrl.flowState.stage;
    expect(stage, isA<GeneratingNoncesStage>());
    final next = (stage as GeneratingNoncesStage).nextStage;
    expect(next, isA<EnterBackupStage>());
    expect((next as EnterBackupStage).deviceName, 'frosty');

    ctrl.dispose();
  });

  test('physical-backup path: nothing needed goes straight to backup '
      'entry', () {
    final ctrl = makeController(
      stage: const RecoveryFlowStage.enterDeviceName(),
      noncesNeeded: (_) => false,
    );

    ctrl.submitDeviceName('frosty');
    final stage = ctrl.flowState.stage;
    expect(stage, isA<EnterBackupStage>());
    expect((stage as EnterBackupStage).deviceName, 'frosty');

    ctrl.dispose();
  });

  test('nonceTopUpDevices keeps only locally-present access-structure '
      'devices', () {
    final here = devId(1);
    final alsoHere = devId(2);
    final remoteParticipants = devId(3);
    final unrelated = devId(9);

    final devices = nonceTopUpDevices(
      accessStructureDevices: [here, remoteParticipants, alsoHere],
      locallyConnected: {alsoHere, here, unrelated},
    );

    expect(devices, [here, alsoHere]);
    expect(devices, isNot(contains(remoteParticipants)));
  });
}
