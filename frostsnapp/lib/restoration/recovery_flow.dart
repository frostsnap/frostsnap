import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/restoration/candidate_ready_view.dart';
import 'package:frostsnap/restoration/enter_backup_view.dart';
import 'package:frostsnap/restoration/enter_device_name_view.dart';
import 'package:frostsnap/restoration/enter_threshold_view.dart';
import 'package:frostsnap/restoration/enter_wallet_name_view.dart';
import 'package:frostsnap/restoration/error_view.dart';
import 'package:frostsnap/restoration/firmware_upgrade_view.dart';
import 'package:frostsnap/restoration/nonce_generation_page.dart';
import 'package:frostsnap/restoration/physical_backup_success_view.dart';
import 'package:frostsnap/restoration/start_restoration_info_view.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/wait_reconnect_device_view.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';

mixin TitledWidget on Widget {
  String get titleText;
}

class WalletRecoveryFlow extends StatefulWidget {
  final RecoveryContext recoveryContext;
  final TargetDevice targetDevice;
  final RecoverShare? recoverShare;
  final bool isDialog;

  const WalletRecoveryFlow({
    super.key,
    required this.recoveryContext,
    required this.targetDevice,
    this.recoverShare,
    this.isDialog = true,
  });

  @override
  State<WalletRecoveryFlow> createState() => _WalletRecoveryFlowState();
}

class _WalletRecoveryFlowState extends State<WalletRecoveryFlow> {
  late RecoveryContext recoveryContext;
  late RecoveryFlowState flowState;

  final prevStates = List<RecoveryFlowStage>.empty(growable: true);
  bool isAnimationForward = true;

  void pushPrevState() {
    isAnimationForward = true;
    prevStates.add(flowState.stage);
  }

  bool tryPopPrevState(BuildContext context) {
    if (prevStates.isNotEmpty) {
      setState(() {
        isAnimationForward = false;
        flowState.stage = prevStates.removeLast();
      });
      return true;
    }
    return false;
  }

  void popOnError({
    required String errorTitle,
    required String errorMessage,
    bool isException = false,
  }) {
    setState(() {
      isAnimationForward = false;
      RecoveryFlowStage returnStage;
      if (prevStates.isNotEmpty) {
        returnStage = prevStates.removeLast();
      } else {
        returnStage = flowState.stage;
      }
      flowState.stage = RecoveryFlowStage.error(
        title: errorTitle,
        message: errorMessage,
        isWarning: !isException,
        returnStage: returnStage,
      );
    });
  }

  @override
  void dispose() {
    flowState.dispose();
    super.dispose();
  }

  Future<void> _completeDeviceShareEnrollment(RecoverShare candidate) async {
    try {
      RestorationId? restorationId;
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();

      switch (recoveryContext) {
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

      if (mounted) {
        Navigator.pop(context, restorationId);
      }
    } catch (e, stackTrace) {
      if (mounted) {
        popOnError(
          errorTitle: 'Unexpected error',
          errorMessage: '$e\n\nStack trace:\n$stackTrace',
          isException: true,
        );
      }
    }
  }

  @override
  void initState() {
    super.initState();
    recoveryContext = widget.recoveryContext;

    // Determine initial stage based on whether we have a share or blank device
    final initialStage = widget.recoverShare != null
        ? RecoveryFlowStage.candidateReady(candidate: widget.recoverShare!)
        : RecoveryFlowStage.startRestorationWithPhysicalBackup();

    flowState = RecoveryFlowState(
      targetDevice: widget.targetDevice,
      stage: initialStage,
    );
  }

  @override
  Widget build(BuildContext context) {
    TitledWidget child;

    switch (flowState.stage) {
      case CandidateReadyStage(:final candidate):
        child = CandidateReadyView(
          candidate: candidate,
          continuing: switch (recoveryContext) {
            ContinuingRestorationContext(:final restorationId) => restorationId,
            _ => null,
          },
          existing: switch (recoveryContext) {
            AddingToWalletContext(:final accessStructureRef) =>
              accessStructureRef,
            _ => null,
          },
          onConfirm: () {
            setState(() {
              pushPrevState();
              flowState.stage = RecoveryFlowStage.generatingNonces(
                nextStage: RecoveryFlowStage.completingDeviceShareEnrollment(
                  candidate: candidate,
                ),
              );
            });
          },
        );
        break;

      case FirmwareUpgradeStage():
        child = FirmwareUpgradeView(
          key: ValueKey("firmware-upgrade"),
          targetDevice: flowState.targetDevice,
          onComplete: () {
            setState(() {
              flowState.stage = switch (recoveryContext) {
                NewRestorationContext() =>
                  const RecoveryFlowStage.enterRestorationDetails(),
                _ => const RecoveryFlowStage.enterDeviceName(),
              };
            });
          },
          onCancel: () {
            Navigator.pop(context);
          },
          onDisconnected: () {
            setState(() {
              flowState.stage = RecoveryFlowStage.waitReconnectDevice(
                nextStage: const RecoveryFlowStage.firmwareUpgrade(),
              );
            });
          },
        );
        break;

      case EnterDeviceNameStage():
        child = EnterDeviceNameView(
          targetDevice: flowState.targetDevice,
          onDisconnected: () {
            setState(() {
              flowState.stage = RecoveryFlowStage.waitReconnectDevice(
                nextStage: const RecoveryFlowStage.enterDeviceName(),
              );
            });
          },
          onDeviceName: (deviceName) {
            final nonceRequest = coord.createNonceRequest(
              devices: [flowState.targetDevice.id],
            );

            if (nonceRequest.someNoncesRequested()) {
              setState(() {
                pushPrevState();
                flowState.stage = RecoveryFlowStage.generatingNonces(
                  nextStage: RecoveryFlowStage.enterBackup(
                    deviceName: deviceName,
                  ),
                );
              });
            } else {
              setState(() {
                pushPrevState();
                flowState.stage = RecoveryFlowStage.enterBackup(
                  deviceName: deviceName,
                );
              });
            }
          },
        );

      case GeneratingNoncesStage(:final nextStage):
        final nonceRequest = coord.createNonceRequest(
          devices: [flowState.targetDevice.id],
        );
        final stream = coord
            .replenishNonces(
              nonceRequest: nonceRequest,
              devices: [flowState.targetDevice.id],
            )
            .toBehaviorSubject();

        child = NonceGenerationPage(
          stream: stream,
          deviceName: coord.getDeviceName(id: flowState.targetDevice.id),
          onDisconnected: flowState.targetDevice.onDisconnected(),
          onComplete: () async {
            setState(() {
              flowState.stage = nextStage;
            });
          },
          onCancel: () {
            coord.cancelProtocol();
            tryPopPrevState(context);
          },
          onDeviceDisconnected: () {
            setState(() {
              flowState.stage = RecoveryFlowStage.waitReconnectDevice(
                nextStage: RecoveryFlowStage.generatingNonces(
                  nextStage: nextStage,
                ),
              );
            });
          },
          onError: (error) {
            popOnError(
              errorTitle: 'Nonce Generation Failed',
              errorMessage: error,
            );
          },
        );
        break;

      case CompletingDeviceShareEnrollmentStage(:final candidate):
        _completeDeviceShareEnrollment(candidate);
        child = const _LoadingView(title: 'Completing enrollment...');
        break;

      case EnterBackupStage(:final deviceName):
        final stream = coord.tellDeviceToEnterPhysicalBackup(
          deviceId: flowState.targetDevice.id,
        );
        child = EnterBackupView(
          stream: stream,
          targetDevice: flowState.targetDevice,
          deviceName: coord.getDeviceName(id: flowState.targetDevice.id),
          onCancel: () {
            tryPopPrevState(context);
          },
          onFinished: (backupPhase) async {
            try {
              switch (recoveryContext) {
                case AddingToWalletContext(:final accessStructureRef):
                  final encryptionKey =
                      await SecureKeyProvider.getEncryptionKey();
                  final isValid = await coord.checkPhysicalBackup(
                    accessStructureRef: accessStructureRef,
                    phase: backupPhase,
                    encryptionKey: encryptionKey,
                  );

                  if (!isValid) {
                    popOnError(
                      errorTitle: 'Cannot add backup',
                      errorMessage:
                          'The backup is not compatible with this wallet',
                    );
                    return;
                  }

                  await coord.tellDeviceToConsolidatePhysicalBackup(
                    accessStructureRef: accessStructureRef,
                    phase: backupPhase,
                    encryptionKey: encryptionKey,
                  );
                  if (mounted) {
                    Navigator.pop(context);
                  }

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

                  final encryptionKey =
                      await SecureKeyProvider.getEncryptionKey();
                  final error = await coord.checkPhysicalBackupForRestoration(
                    restorationId: restorationId,
                    phase: backupPhase,
                    encryptionKey: encryptionKey,
                  );

                  if (error != null) {
                    popOnError(
                      errorTitle: 'Cannot add backup',
                      errorMessage: error.toString(),
                    );
                    return;
                  }

                  await coord.tellDeviceToSavePhysicalBackup(
                    phase: backupPhase,
                    restorationId: restorationId,
                  );
                  setState(() {
                    recoveryContext = (recoveryContext as NewRestorationContext)
                        .copyWith(restorationId: restorationId);
                    flowState.stage = RecoveryFlowStage.physicalBackupSuccess(
                      deviceName: deviceName,
                    );
                  });

                case ContinuingRestorationContext(:final restorationId):
                  final encryptionKey =
                      await SecureKeyProvider.getEncryptionKey();
                  final error = await coord.checkPhysicalBackupForRestoration(
                    restorationId: restorationId,
                    phase: backupPhase,
                    encryptionKey: encryptionKey,
                  );

                  if (error != null) {
                    popOnError(
                      errorTitle: 'Cannot add backup',
                      errorMessage: error.toString(),
                    );
                    return;
                  }

                  await coord.tellDeviceToSavePhysicalBackup(
                    phase: backupPhase,
                    restorationId: restorationId,
                  );
                  setState(() {
                    flowState.stage = RecoveryFlowStage.physicalBackupSuccess(
                      deviceName: deviceName,
                    );
                  });
              }
            } catch (e, stackTrace) {
              popOnError(
                errorTitle: 'Failed to save backup',
                errorMessage: '$e\n\nStack trace:\n$stackTrace',
                isException: true,
              );
            }
          },
          onError: (e) {
            popOnError(errorTitle: 'Backup Entry Failed', errorMessage: e);
          },
        );
        break;
      case StartRestorationWithPhysicalBackupStage():
        child = StartRestorationInfoView(
          recoveryContext: recoveryContext,
          onContinue: () {
            setState(() {
              pushPrevState();
              if (flowState.targetDevice.needsFirmwareUpgrade()) {
                flowState.stage = RecoveryFlowStage.firmwareUpgrade();
              } else {
                switch (recoveryContext) {
                  case NewRestorationContext():
                    flowState.stage =
                        RecoveryFlowStage.enterRestorationDetails();
                    break;
                  default:
                    flowState.stage = RecoveryFlowStage.enterDeviceName();
                }
              }
            });
          },
        );
        break;

      case WaitReconnectDeviceStage(:final nextStage):
        child = WaitReconnectDeviceView(
          targetDevice: flowState.targetDevice,
          onReconnected: () {
            setState(() {
              flowState.stage = nextStage;
            });
          },
          onCancel: () {
            Navigator.pop(context);
          },
        );
        break;

      case EnterRestorationDetailsStage():
        child = EnterWalletNameView(
          initialWalletName: null,
          initialBitcoinNetwork: null,
          onWalletNameEntered: (walletName, bitcoinNetwork) {
            setState(() {
              pushPrevState();
              if (recoveryContext case NewRestorationContext()) {
                recoveryContext = RecoveryContext.newRestoration(
                  walletName: walletName,
                  network: bitcoinNetwork,
                  threshold: null,
                );
              }
              flowState.stage = RecoveryFlowStage.enterThreshold(
                walletName: walletName,
                network: bitcoinNetwork,
              );
            });
          },
        );
        break;

      case EnterThresholdStage(:final walletName, :final network):
        child = EnterThresholdView(
          walletName: walletName,
          network: network,
          initialThreshold: null,
          onThresholdEntered: (threshold) {
            setState(() {
              pushPrevState();
              if (recoveryContext case NewRestorationContext(
                :final walletName,
                :final network,
              )) {
                recoveryContext = RecoveryContext.newRestoration(
                  walletName: walletName,
                  network: network,
                  threshold: threshold,
                );
              }
              flowState.stage = const RecoveryFlowStage.enterDeviceName();
            });
          },
        );
        break;
      case PhysicalBackupSuccessStage(:final deviceName):
        final restorationId = switch (recoveryContext) {
          NewRestorationContext(:final restorationId) => restorationId,
          ContinuingRestorationContext(:final restorationId) => restorationId,
          _ => null,
        };
        child = PhysicalBackupSuccessView(
          deviceName: deviceName,
          onClose: () {
            Navigator.pop(context, restorationId);
          },
        );
        break;

      case ErrorStage(
        :final title,
        :final message,
        :final isWarning,
        :final returnStage,
      ):
        child = ErrorView(
          title: title,
          message: message,
          isWarning: isWarning,
          onRetry: () {
            setState(() {
              isAnimationForward = true;
              flowState.stage = returnStage;
            });
          },
        );
        break;
    }

    final switcher = AnimatedSwitcher(
      duration: Durations.medium4,
      reverseDuration: Duration.zero,
      transitionBuilder: (child, animation) {
        final curvedAnimation = CurvedAnimation(
          parent: animation,
          curve: Curves.easeInOutCubicEmphasized,
        );
        return SlideTransition(
          position: Tween<Offset>(
            begin: isAnimationForward
                ? const Offset(1, 0)
                : const Offset(-1, 0),
            end: Offset.zero,
          ).animate(curvedAnimation),
          child: FadeTransition(opacity: animation, child: child),
        );
      },
      child: child,
    );

    final scopedSwitcher = PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, result) {
        if (didPop) return;
        goBackOrClose(context);
      },
      child: switcher,
    );

    if (widget.isDialog) {
      return Dialog(
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
        child: ConstrainedBox(
          constraints: const BoxConstraints(
            minWidth: 480,
            maxWidth: 480,
            minHeight: 360,
          ),
          child: switcher,
        ),
      );
    } else {
      final header = TopBar(
        title: Text(child.titleText),
        leadingButton: IconButton(
          icon: Icon(Icons.arrow_back_rounded),
          onPressed: () => goBackOrClose(context),
          tooltip: 'Back',
        ),
      );
      return Column(
        mainAxisSize: MainAxisSize.min,
        children: [header, scopedSwitcher],
      );
    }
  }

  void goBackOrClose(BuildContext context) {
    if (!tryPopPrevState(context)) Navigator.pop(context);
  }
}

class _LoadingView extends StatelessWidget with TitledWidget {
  final String title;

  const _LoadingView({required this.title});

  @override
  String get titleText => title;

  @override
  Widget build(BuildContext context) {
    return const Center(child: CircularProgressIndicator());
  }
}
