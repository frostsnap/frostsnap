import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/restoration/candidate_ready_view.dart';
import 'package:frostsnap/restoration/choose_method_view.dart';
import 'package:frostsnap/restoration/enter_backup_view.dart';
import 'package:frostsnap/restoration/enter_device_name_view.dart';
import 'package:frostsnap/restoration/enter_threshold_view.dart';
import 'package:frostsnap/restoration/enter_wallet_name_view.dart';
import 'package:frostsnap/restoration/error_view.dart';
import 'package:frostsnap/restoration/firmware_upgrade_view.dart';
import 'package:frostsnap/restoration/nonce_generation_page.dart';
import 'package:frostsnap/restoration/physical_backup_success_view.dart';
import 'package:frostsnap/restoration/plug_in_blank_view.dart';
import 'package:frostsnap/restoration/plug_in_prompt_view.dart';
import 'package:frostsnap/restoration/state.dart';
import 'package:frostsnap/restoration/target_device.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';

class WalletRecoveryFlow extends StatefulWidget {
  final RecoveryContext recoveryContext;
  final bool isDialog;

  const WalletRecoveryFlow({
    super.key,
    required this.recoveryContext,
    this.isDialog = true,
  });

  @override
  State<WalletRecoveryFlow> createState() => _WalletRecoveryFlowState();
}

class _WalletRecoveryFlowState extends State<WalletRecoveryFlow> {
  late RecoveryContext recoveryContext;

  RecoveryFlowState flowState = const RecoveryFlowState.start();

  final prevStates = List<RecoveryFlowState>.empty(growable: true);
  bool isAnimationForward = true;

  void pushPrevState() {
    isAnimationForward = true;
    prevStates.add(flowState);
  }

  bool tryPopPrevState(BuildContext context) {
    if (prevStates.isNotEmpty) {
      setState(() {
        isAnimationForward = false;
        flowState = prevStates.removeLast();
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
      RecoveryFlowState returnState;
      if (prevStates.isNotEmpty) {
        returnState = prevStates.removeLast();
      } else {
        returnState = flowState;
      }
      flowState = RecoveryFlowState.error(
        title: errorTitle,
        message: errorMessage,
        isWarning: !isException,
        returnState: returnState,
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
  }

  @override
  Widget build(BuildContext context) {
    TitledWidget child;

    switch (flowState) {
      case StartState():
        final kind = switch (recoveryContext) {
          ContinuingRestorationContext() => MethodChoiceKind.continueRecovery,
          AddingToWalletContext() => MethodChoiceKind.addToWallet,
          NewRestorationContext() => MethodChoiceKind.startRecovery,
        };
        child = ChooseMethodView(
          kind: kind,
          onDeviceChosen: () {
            setState(() {
              pushPrevState();
              flowState = const RecoveryFlowState.waitDevice();
            });
          },
          onPhysicalBackupChosen: () {
            setState(() {
              pushPrevState();
              flowState = switch (recoveryContext) {
                NewRestorationContext() =>
                  const RecoveryFlowState.enterRestorationDetails(),
                _ => const RecoveryFlowState.waitPhysicalBackupDevice(),
              };
            });
          },
        );
        break;

      case WaitDeviceState():
        child = PlugInPromptView(
          context: recoveryContext,
          onCandidateDetected: (detectedShare) async {
            if (mounted) {
              final encryptionKey = await SecureKeyProvider.getEncryptionKey();

              if (recoveryContext case ContinuingRestorationContext(
                :final restorationId,
              )) {
                final error = await coord
                    .checkContinueRestoringWalletFromDeviceShare(
                      restorationId: restorationId,
                      recoverShare: detectedShare,
                      encryptionKey: encryptionKey,
                    );
                if (error != null) {
                  final deviceName =
                      coord.getDeviceName(id: detectedShare.heldBy) ??
                      '<empty>';
                  popOnError(
                    errorMessage: error.toString(),
                    errorTitle: 'Cannot add key from $deviceName',
                  );
                  return;
                }
              } else if (recoveryContext case AddingToWalletContext(
                :final accessStructureRef,
              )) {
                final error = await coord.checkRecoverShare(
                  accessStructureRef: accessStructureRef,
                  recoverShare: detectedShare,
                  encryptionKey: encryptionKey,
                );
                if (error != null) {
                  final deviceName =
                      coord.getDeviceName(id: detectedShare.heldBy) ??
                      '<empty>';
                  popOnError(
                    errorMessage: error.toString(),
                    errorTitle: 'Cannot add key from $deviceName',
                  );
                  return;
                }
              } else {
                final error = await coord.checkStartRestoringKeyFromDeviceShare(
                  recoverShare: detectedShare,
                  encryptionKey: encryptionKey,
                );
                if (error != null) {
                  popOnError(
                    errorMessage: error.toString(),
                    errorTitle: 'Cannot start restoration',
                  );
                  return;
                }
              }

              final deviceList = await GlobalStreams.deviceListSubject.first;
              final device = deviceList.state.getDevice(
                id: detectedShare.heldBy,
              );

              if (device != null) {
                setState(() {
                  pushPrevState();
                  flowState = RecoveryFlowState.candidateReady(
                    candidate: detectedShare,
                    targetDevice: TargetDevice(device),
                  );
                });
              }
            }
          },
        );
        break;
      case CandidateReadyState(:final candidate, :final targetDevice):
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
              flowState = RecoveryFlowState.generatingNonces(
                targetDevice: targetDevice,
                nextState: RecoveryFlowState.completingDeviceShareEnrollment(
                  candidate: candidate,
                ),
              );
            });
          },
        );
        break;
      case WaitPhysicalBackupDeviceState():
        child = PlugInBlankView(
          onBlankDeviceConnected: (device) {
            final eligibility = device.firmwareUpgradeEligibility();
            setState(() {
              eligibility.when(
                canUpgrade: () {
                  flowState = RecoveryFlowState.firmwareUpgrade(
                    targetDevice: TargetDevice(device),
                  );
                },
                upToDate: () {
                  flowState = RecoveryFlowState.enterDeviceName(
                    targetDevice: TargetDevice(device),
                  );
                },
                cannotUpgrade: (reason) {
                  isAnimationForward = false;
                  flowState = RecoveryFlowState.error(
                    title: 'Incompatible Firmware',
                    message: reason,
                    isWarning: true,
                    returnState:
                        const RecoveryFlowState.waitPhysicalBackupDevice(),
                  );
                },
              );
            });
          },
        );
        break;

      case FirmwareUpgradeState(:final targetDevice):
        child = FirmwareUpgradeView(
          key: ValueKey("firmware-upgrade"),
          targetDevice: targetDevice,
          onComplete: () {
            setState(() {
              flowState = const RecoveryFlowState.waitPhysicalBackupDevice();
            });
          },
          onCancel: () {
            setState(() {
              flowState = const RecoveryFlowState.waitPhysicalBackupDevice();
            });
          },
          onDisconnected: () {
            popOnError(
              errorTitle: 'Device Disconnected',
              errorMessage:
                  'The device was disconnected. Please reconnect and try again.',
            );
          },
        );
        break;

      case EnterDeviceNameState(:final targetDevice):
        child = EnterDeviceNameView(
          targetDevice: targetDevice,
          onDisconnected: () {
            popOnError(
              errorTitle: 'Device Disconnected',
              errorMessage:
                  'The device was disconnected. Please reconnect and try again.',
            );
          },
          onDeviceName: (deviceName) {
            final nonceRequest = coord.createNonceRequest(
              devices: [targetDevice.id],
            );

            final enterBackupState = RecoveryFlowState.enterBackup(
              targetDevice: targetDevice,
              deviceName: deviceName,
            );

            if (nonceRequest.someNoncesRequested()) {
              setState(() {
                pushPrevState();
                flowState = RecoveryFlowState.generatingNonces(
                  targetDevice: targetDevice,
                  nextState: enterBackupState,
                );
              });
            } else {
              setState(() {
                pushPrevState();
                flowState = enterBackupState;
              });
            }
          },
        );

      case GeneratingNoncesState(:final targetDevice, :final nextState):
        final nonceRequest = coord.createNonceRequest(
          devices: [targetDevice.id],
        );
        final stream = coord
            .replenishNonces(
              nonceRequest: nonceRequest,
              devices: [targetDevice.id],
            )
            .toBehaviorSubject();

        child = NonceGenerationPage(
          stream: stream,
          deviceName: coord.getDeviceName(id: targetDevice.id),
          onDisconnected: targetDevice.onDisconnected,
          onComplete: () async {
            setState(() {
              flowState = nextState;
            });
          },
          onCancel: () {
            coord.cancelProtocol();
            tryPopPrevState(context);
          },
          onDeviceDisconnected: () {
            popOnError(
              errorTitle: 'Device Disconnected',
              errorMessage: 'The device was disconnected during preparation.',
            );
          },
          onError: (error) {
            popOnError(
              errorTitle: 'Nonce Generation Failed',
              errorMessage: error,
            );
          },
        );
        break;

      case CompletingDeviceShareEnrollmentState(:final candidate):
        _completeDeviceShareEnrollment(candidate);
        child = const _LoadingView(title: 'Completing enrollment...');
        break;

      case EnterBackupState(:final targetDevice, :final deviceName):
        final stream = coord.tellDeviceToEnterPhysicalBackup(
          deviceId: targetDevice.id,
        );
        child = EnterBackupView(
          stream: stream,
          deviceId: targetDevice.id,
          deviceName: coord.getDeviceName(id: targetDevice.id),
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
                    flowState = RecoveryFlowState.physicalBackupSuccess(
                      restorationId: restorationId,
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
                    flowState = RecoveryFlowState.physicalBackupSuccess(
                      restorationId: restorationId,
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
      case EnterRestorationDetailsState():
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
              flowState = RecoveryFlowState.enterThreshold(
                walletName: walletName,
                network: bitcoinNetwork,
              );
            });
          },
        );
        break;

      case EnterThresholdState(:final walletName, :final network):
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
              flowState = const RecoveryFlowState.waitPhysicalBackupDevice();
            });
          },
        );
        break;
      case PhysicalBackupSuccessState(:final restorationId, :final deviceName):
        child = PhysicalBackupSuccessView(
          deviceName: deviceName,
          onClose: () {
            Navigator.pop(context, restorationId);
          },
        );
        break;

      case ErrorState(
        :final title,
        :final message,
        :final isWarning,
        :final returnState,
      ):
        child = ErrorView(
          title: title,
          message: message,
          isWarning: isWarning,
          onRetry: () {
            setState(() {
              isAnimationForward = true;
              flowState = returnState;
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
      child: Padding(
        key: ValueKey(flowState),
        padding: const EdgeInsets.all(16.0),
        child: child,
      ),
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
      final windowSize = WindowSizeContext.of(context);
      final header = TopBarSliver(
        title: Text(child.titleText),
        leading: IconButton(
          icon: Icon(Icons.arrow_back_rounded),
          onPressed: () => goBackOrClose(context),
          tooltip: 'Back',
        ),
      );
      return ConstrainedBox(
        constraints: BoxConstraints(minHeight: 360),
        child: CustomScrollView(
          shrinkWrap: windowSize != WindowSizeClass.compact,
          slivers: [
            header,
            SliverToBoxAdapter(child: scopedSwitcher),
          ],
        ),
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
