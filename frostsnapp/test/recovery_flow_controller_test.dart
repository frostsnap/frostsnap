import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/restoration/recovery_flow.dart';
import 'package:frostsnap/restoration/recovery_flow_controller.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/src/rust/lib.dart';

// The Rust library isn't loaded here, so any coordinator call the controller
// makes outside its RecoveryProtocol seam throws. Tests that reach a terminal
// without an ErrorStage therefore prove no such call was made.

DeviceId deviceId(int seed) {
  final bytes = Uint8List(33);
  bytes[0] = seed;
  return DeviceId(field0: U8Array33(bytes));
}

final accessStructureRef = AccessStructureRef(
  keyId: KeyId(field0: U8Array32(Uint8List(32))),
  accessStructureId: AccessStructureId(field0: U8Array32(Uint8List(32))),
);

class _Stub {
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class StubRecoverShare extends _Stub implements RecoverShare {}

class StubPhysicalBackupPhase extends _Stub implements PhysicalBackupPhase {}

class FakeTargetDevice extends TargetDevice {
  FakeTargetDevice() : super(deviceId(1));

  final _watchers = <StreamController<bool>>[];

  /// Watches the controller currently holds open.
  int get watchCount => _watchers.length;

  void setConnected(bool connected) {
    for (final watcher in List.of(_watchers)) {
      watcher.add(connected);
    }
  }

  @override
  ConnectedDevice? get device => null;

  @override
  bool needsFirmwareUpgrade() => false;

  @override
  Stream<bool> get connected {
    late final StreamController<bool> watcher;
    watcher = StreamController<bool>(
      onListen: () => _watchers.add(watcher),
      onCancel: () => _watchers.remove(watcher),
    );
    return watcher.stream;
  }
}

class FakeProtocol extends RecoveryProtocol {
  bool needNonces = false;
  int replenishCalls = 0;
  int cancelCalls = 0;
  int enterBackupCalls = 0;
  StreamController<NonceReplenishState>? replenish;
  StreamController<EnterPhysicalBackupState>? backupEvents;

  @override
  bool noncesNeeded(DeviceId device) => needNonces;

  @override
  Stream<NonceReplenishState> replenishNonces(DeviceId device) {
    replenishCalls++;
    replenish = StreamController<NonceReplenishState>();
    return replenish!.stream;
  }

  @override
  void cancelProtocol() => cancelCalls++;

  @override
  Stream<EnterPhysicalBackupState> enterPhysicalBackup(DeviceId device) {
    enterBackupCalls++;
    backupEvents = StreamController<EnterPhysicalBackupState>();
    return backupEvents!.stream;
  }
}

class Harness {
  Harness({
    required RecoveryContext context,
    required RecoveryFlowStage initialStage,
    SymmetricKey? walletKey,
  }) {
    controller = RecoveryFlowController(
      recoveryContext: context,
      targetDevice: device,
      initialStage: initialStage,
      protocol: protocol,
      existingWalletKey: (ref, action) async {
        keyRequests.add(action);
        return walletKey;
      },
    );
  }

  final device = FakeTargetDevice();
  final protocol = FakeProtocol();
  final keyRequests = <String>[];
  late final RecoveryFlowController controller;
}

const newRestoration = RecoveryContext.newRestoration();
final addingToWallet = RecoveryContext.addingToWallet(
  accessStructureRef: accessStructureRef,
);

void main() {
  group('stage side effects', () {
    test('generating nonces holds one replenish per entry and releases it '
        'on leaving', () {
      final h = Harness(
        context: addingToWallet,
        initialStage: RecoveryFlowStage.candidateReady(
          candidate: StubRecoverShare(),
        ),
      );
      final ctrl = h.controller;
      expect(h.protocol.replenishCalls, 0);
      expect(h.device.watchCount, 0);

      ctrl.confirmCandidate();
      expect(ctrl.stage, isA<GeneratingNoncesStage>());
      expect(h.protocol.replenishCalls, 1);
      expect(h.protocol.replenish!.hasListener, isTrue);
      expect(h.device.watchCount, 1);

      ctrl.notifyListeners();
      ctrl.notifyListeners();
      expect(h.protocol.replenishCalls, 1);
      expect(h.device.watchCount, 1);

      final firstReplenish = h.protocol.replenish!;
      ctrl.back();
      expect(ctrl.stage, isA<CandidateReadyStage>());
      expect(h.protocol.cancelCalls, 1);
      expect(firstReplenish.hasListener, isFalse);
      expect(ctrl.nonceStream, isNull);
      expect(h.device.watchCount, 0);

      ctrl.confirmCandidate();
      expect(h.protocol.replenishCalls, 2);
      expect(h.device.watchCount, 1);
      ctrl.dispose();
      expect(h.protocol.cancelCalls, 2);
      expect(h.protocol.replenish!.hasListener, isFalse);
      expect(h.device.watchCount, 0);
    });

    test('a completed replenish is not cancelled on leaving', () {
      final h = Harness(
        context: addingToWallet,
        initialStage: const RecoveryFlowStage.enterDeviceName(),
      );
      h.protocol.needNonces = true;
      final ctrl = h.controller;

      ctrl.submitDeviceName('frosty');
      ctrl.onNonceTerminal(const NonceReplenishCompleted());
      expect(ctrl.stage, isA<EnterBackupStage>());
      expect(h.protocol.cancelCalls, 0);
      expect(h.device.watchCount, 0);
      ctrl.dispose();
    });

    test('entering the backup starts the device once and stops on leave', () {
      final h = Harness(
        context: addingToWallet,
        initialStage: const RecoveryFlowStage.enterDeviceName(),
      );
      final ctrl = h.controller;

      ctrl.submitDeviceName('frosty');
      expect(ctrl.stage, isA<EnterBackupStage>());
      expect(h.protocol.enterBackupCalls, 1);
      expect(h.protocol.backupEvents!.hasListener, isTrue);

      ctrl.notifyListeners();
      expect(h.protocol.enterBackupCalls, 1);

      ctrl.back();
      expect(ctrl.stage, isA<EnterDeviceNameStage>());
      expect(h.protocol.backupEvents!.hasListener, isFalse);
      expect(h.device.watchCount, 1);
      ctrl.dispose();
      expect(h.device.watchCount, 0);
    });

    test(
      'disconnect and reconnect leave only the current entry subscribed',
      () async {
        final h = Harness(
          context: addingToWallet,
          initialStage: const RecoveryFlowStage.enterDeviceName(),
        );
        final ctrl = h.controller;
        expect(h.device.watchCount, 1);

        h.protocol.needNonces = true;
        ctrl.submitDeviceName('frosty');
        expect(ctrl.stage, isA<GeneratingNoncesStage>());
        expect(h.device.watchCount, 1);
        final firstReplenish = h.protocol.replenish!;

        h.device.setConnected(false);
        await pumpEventQueue();
        expect(ctrl.stage, isA<WaitReconnectDeviceStage>());
        expect(firstReplenish.hasListener, isFalse);
        expect(h.protocol.cancelCalls, 1);
        expect(h.device.watchCount, 1);

        h.device.setConnected(true);
        await pumpEventQueue();
        expect(ctrl.stage, isA<GeneratingNoncesStage>());
        expect(h.protocol.replenishCalls, 2);
        expect(h.protocol.replenish!.hasListener, isTrue);
        expect(h.device.watchCount, 1);

        ctrl.back();
        expect(ctrl.stage, isA<EnterDeviceNameStage>());
        expect(h.protocol.cancelCalls, 2);
        expect(h.device.watchCount, 1);
        ctrl.dispose();
        expect(h.device.watchCount, 0);
      },
    );

    testWidgets('rebuilding the steps never repeats protocol work', (
      tester,
    ) async {
      final h = Harness(
        context: addingToWallet,
        initialStage: RecoveryFlowStage.candidateReady(
          candidate: StubRecoverShare(),
        ),
      );
      h.controller.confirmCandidate();
      late StateSetter rebuild;
      await tester.pumpWidget(
        MaterialApp(
          home: Material(
            child: StatefulBuilder(
              builder: (context, setState) {
                rebuild = setState;
                return RecoveryFlowSteps(controller: h.controller);
              },
            ),
          ),
        ),
      );
      for (var i = 0; i < 3; i++) {
        rebuild(() {});
        await tester.pump();
      }
      expect(find.text('Preparing Device'), findsOneWidget);
      expect(h.protocol.replenishCalls, 1);
      expect(h.device.watchCount, 1);

      await tester.pumpWidget(const SizedBox());
      h.controller.dispose();
    });
  });

  group('back stack', () {
    test('back retraces pushed stages, then dismisses', () {
      final h = Harness(
        context: newRestoration,
        initialStage:
            const RecoveryFlowStage.startRestorationWithPhysicalBackup(),
      );
      final ctrl = h.controller;

      ctrl.confirmStartRestoration();
      ctrl.submitWalletName('Savings', BitcoinNetwork.bitcoin);
      ctrl.submitThreshold(2);
      expect(ctrl.stage, isA<EnterDeviceNameStage>());
      expect(ctrl.isAnimationForward, isTrue);
      expect(
        ctrl.recoveryContext,
        const RecoveryContext.newRestoration(
          walletName: 'Savings',
          network: BitcoinNetwork.bitcoin,
          threshold: 2,
        ),
      );

      ctrl.back();
      expect(ctrl.stage, isA<EnterThresholdStage>());
      expect(ctrl.isAnimationForward, isFalse);
      ctrl.back();
      expect(ctrl.stage, isA<EnterRestorationDetailsStage>());
      ctrl.back();
      expect(ctrl.stage, isA<StartRestorationWithPhysicalBackupStage>());
      expect(ctrl.outcome, isNull);

      ctrl.back();
      expect(ctrl.outcome, isA<RecoveryFlowDismissed>());
      ctrl.dispose();
    });

    test('an error pops the stage it came from and retry returns to it', () {
      final h = Harness(
        context: addingToWallet,
        initialStage: RecoveryFlowStage.candidateReady(
          candidate: StubRecoverShare(),
        ),
      );
      final ctrl = h.controller;

      ctrl.confirmCandidate();
      ctrl.onNonceTerminal(const NonceReplenishAborted());
      final stage = ctrl.stage;
      expect(stage, isA<ErrorStage>());
      expect((stage as ErrorStage).returnStage, isA<CandidateReadyStage>());

      ctrl.onNonceTerminal(const NonceReplenishCompleted());
      expect(ctrl.stage, isA<ErrorStage>());

      ctrl.retryFromError();
      expect(ctrl.stage, isA<CandidateReadyStage>());
      ctrl.back();
      expect(ctrl.outcome, isA<RecoveryFlowDismissed>());
      ctrl.dispose();
    });

    test('back is ignored while the share is being enrolled', () {
      final h = Harness(
        context: addingToWallet,
        initialStage: RecoveryFlowStage.candidateReady(
          candidate: StubRecoverShare(),
        ),
        walletKey: null,
      );
      final ctrl = h.controller;
      ctrl.confirmCandidate();
      ctrl.onNonceTerminal(const NonceReplenishCompleted());
      expect(ctrl.stage, isA<CompletingDeviceShareEnrollmentStage>());

      ctrl.back();
      expect(ctrl.stage, isA<CompletingDeviceShareEnrollmentStage>());
      expect(ctrl.outcome, isNull);
      ctrl.dispose();
    });
  });

  group('wallet key mismatch', () {
    test('adding a device share dismisses without enrolling', () async {
      final h = Harness(
        context: addingToWallet,
        initialStage: RecoveryFlowStage.candidateReady(
          candidate: StubRecoverShare(),
        ),
      );
      final ctrl = h.controller;

      ctrl.confirmCandidate();
      ctrl.onNonceTerminal(const NonceReplenishCompleted());
      await pumpEventQueue();

      expect(h.keyRequests, ['recover this key into the wallet']);
      expect(ctrl.outcome, isA<RecoveryFlowDismissed>());
      expect(ctrl.stage, isA<CompletingDeviceShareEnrollmentStage>());

      ctrl.back();
      ctrl.retryFromError();
      expect(ctrl.outcome, isA<RecoveryFlowDismissed>());
      ctrl.dispose();
    });

    test('adding a physical backup dismisses without saving', () async {
      final h = Harness(
        context: addingToWallet,
        initialStage: const RecoveryFlowStage.enterDeviceName(),
      );
      final ctrl = h.controller;

      ctrl.submitDeviceName('frosty');
      h.protocol.backupEvents!.add(
        EnterPhysicalBackupState(
          deviceId: deviceId(1),
          entered: StubPhysicalBackupPhase(),
          saved: false,
        ),
      );
      await pumpEventQueue();
      expect(ctrl.pendingBackupEntry, isA<BackupEntered>());
      expect(h.keyRequests, isEmpty);

      await ctrl.completeBackupEntry();

      expect(h.keyRequests, ['add this backup to the wallet']);
      expect(ctrl.outcome, isA<RecoveryFlowDismissed>());
      expect(h.protocol.backupEvents!.hasListener, isFalse);
      ctrl.dispose();
    });
  });
}
