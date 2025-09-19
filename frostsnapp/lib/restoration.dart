import 'dart:async';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device_action_fullscreen_dialog.dart';
import 'package:frostsnap/device_action_upgrade.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/maybe_fullscreen_dialog.dart';
import 'package:frostsnap/secure_key_provider.dart';
import 'package:frostsnap/nonce_replenish.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/snackbar.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_add.dart';

class WalletRecoveryPage extends StatelessWidget {
  final RestoringKey restoringKey;
  final Function(AccessStructureRef) onWalletRecovered;

  const WalletRecoveryPage({
    super.key,
    required this.restoringKey,
    required this.onWalletRecovered,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;

    final progressActionCard = MaterialDialogCard(
      iconData: switch (restoringKey.problem) {
        null => Icons.check_circle_outline_rounded,
        RestorationProblem_NotEnoughShares() => Icons.info_rounded,
        RestorationProblem_InvalidShares() => Icons.running_with_errors_rounded,
      },
      title: switch (restoringKey.problem) {
        null => Text('Ready to restore'),
        RestorationProblem_NotEnoughShares() => Text('Not enough shares'),
        RestorationProblem_InvalidShares() => Text('Some shares are invalid'),
      },
      content: switch (restoringKey.problem) {
        null => Text(
          'You have enough keys to restore the wallet. You can add more keys later under settings if needed.',
        ),
        RestorationProblem_NotEnoughShares(:final needMore) => Text(
          needMore == 1
              ? '1 more key to restore wallet.'
              : '$needMore more keys needed to restore wallet.',
        ),
        RestorationProblem_InvalidShares() => Text(
          'Remove incompatible shares before continuing.',
        ),
      },
      backgroundColor: switch (restoringKey.problem) {
        null => theme.colorScheme.primaryContainer,
        RestorationProblem_NotEnoughShares() =>
          theme.colorScheme.secondaryContainer,
        RestorationProblem_InvalidShares() => theme.colorScheme.errorContainer,
      },
      textColor: switch (restoringKey.problem) {
        null => theme.colorScheme.onPrimaryContainer,
        RestorationProblem_NotEnoughShares() =>
          theme.colorScheme.onSecondaryContainer,
        RestorationProblem_InvalidShares() =>
          theme.colorScheme.onErrorContainer,
      },
      variantTextColor: switch (restoringKey.problem) {
        null => theme.colorScheme.onPrimaryContainer,
        RestorationProblem_NotEnoughShares() =>
          theme.colorScheme.onSecondaryContainer,
        RestorationProblem_InvalidShares() =>
          theme.colorScheme.onErrorContainer,
      },
      iconColor: switch (restoringKey.problem) {
        null => theme.colorScheme.onPrimaryContainer,
        RestorationProblem_NotEnoughShares() =>
          theme.colorScheme.onSecondaryContainer,
        RestorationProblem_InvalidShares() =>
          theme.colorScheme.onErrorContainer,
      },
      actions: [
        TextButton.icon(
          icon: const Icon(Icons.close_rounded),
          label: const Text('Cancel'),
          onPressed: () {
            coord.cancelRestoration(restorationId: restoringKey.restorationId);
          },
          style: TextButton.styleFrom(
            foregroundColor: switch (restoringKey.problem) {
              null => theme.colorScheme.onPrimaryContainer,
              RestorationProblem_NotEnoughShares() =>
                theme.colorScheme.onSecondaryContainer,
              RestorationProblem_InvalidShares() =>
                theme.colorScheme.onErrorContainer,
            },
          ),
        ),
        FilledButton.icon(
          icon: const Icon(Icons.check_rounded),
          label: const Text('Restore'),
          onPressed: restoringKey.problem == null
              ? () async {
                  try {
                    final encryptionKey =
                        await SecureKeyProvider.getEncryptionKey();
                    final accessStructureRef = await coord.finishRestoring(
                      restorationId: restoringKey.restorationId,
                      encryptionKey: encryptionKey,
                    );

                    // Nonce generation now happens when each device is enrolled,
                    // not at the end of wallet restoration
                    onWalletRecovered(accessStructureRef);
                  } catch (e) {
                    if (context.mounted) {
                      showErrorSnackbar(
                        context,
                        "Failed to recover wallet: $e",
                      );
                    }
                  }
                }
              : null,
        ),
      ],
    );

    final appBar = SliverAppBar.medium(
      pinned: true,
      title: Text.rich(
        TextSpan(
          children: [
            TextSpan(text: restoringKey.name),
            WidgetSpan(
              child: Card.filled(
                color: theme.colorScheme.primaryContainer.withAlpha(80),
                margin: EdgeInsets.only(left: 12, bottom: 2),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 8,
                    vertical: 4,
                  ),
                  child: Text(
                    'Wallet in Restoration',
                    style: theme.textTheme.labelSmall?.copyWith(
                      color: theme.colorScheme.onPrimaryContainer,
                      fontWeight: FontWeight.w500,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
        overflow: TextOverflow.fade,
        softWrap: false,
      ),
    );

    var devicesColumn = Column(
      spacing: 8,
      children: restoringKey.sharesObtained.map((share) {
        final deleteButton = IconButton(
          icon: const Icon(Icons.delete),
          tooltip: 'Remove key',
          onPressed: () async {
            await coord.deleteRestorationShare(
              restorationId: restoringKey.restorationId,
              deviceId: share.deviceId,
            );
            homeCtx.walletListController.selectRecoveringWallet(
              restoringKey.restorationId,
            );
          },
        );
        final deviceName = coord.getDeviceName(id: share.deviceId) ?? '<empty>';
        return Card.filled(
          color: theme.colorScheme.surfaceContainerHigh,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(24)),
          ),
          margin: EdgeInsets.zero,
          child: ListTile(
            contentPadding: EdgeInsets.symmetric(horizontal: 16),
            leading: Icon(Icons.key),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              spacing: 8,
              children: [
                ...switch (share.validity) {
                  RestorationShareValidity.valid => [
                    Tooltip(
                      message: "Valid key",
                      child: Icon(
                        Icons.check_circle,
                        color: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                  ],
                  RestorationShareValidity.unknown => [
                    Tooltip(
                      message: "Validity to be determined",
                      child: Icon(
                        Icons.pending_rounded,
                        color: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                  ],
                  RestorationShareValidity.invalid => [
                    Tooltip(
                      message: "This key is incompatible with the other keys",
                      child: Icon(
                        Icons.warning,
                        color: Theme.of(context).colorScheme.error,
                      ),
                    ),
                  ],
                },
                deleteButton,
              ],
            ),
            title: Row(
              spacing: 8,
              children: [
                Flexible(
                  child: Tooltip(
                    message: "The key number",
                    child: Text(
                      "#${share.index}",
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                  ),
                ),
                Flexible(child: Text(deviceName, style: monospaceTextStyle)),
              ],
            ),
          ),
        );
      }).toList(),
    );

    final usableHeight =
        MediaQuery.of(context).size.height -
        MediaQuery.of(context).padding.top -
        MediaQuery.of(context).padding.bottom;

    final sizeClass = WindowSizeContext.of(context);
    final alignTop =
        sizeClass == WindowSizeClass.compact ||
        sizeClass == WindowSizeClass.medium ||
        sizeClass == WindowSizeClass.expanded;
    return CustomScrollView(
      slivers: [
        appBar,
        SliverToBoxAdapter(
          child: ConstrainedBox(
            constraints: BoxConstraints(
              maxWidth: 560,
              minHeight: alignTop ? 0.0 : usableHeight * 2 / 3,
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16.0),
              child: Align(
                alignment: AlignmentDirectional.center,
                child: ConstrainedBox(
                  constraints: BoxConstraints(
                    maxWidth: alignTop ? double.infinity : 600,
                  ),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Padding(
                        padding: const EdgeInsets.all(4.0),
                        child: Text(
                          "You need ${restoringKey.threshold} or more keys to restore this wallet.",
                        ),
                      ),
                      SizedBox(height: 8),
                      Padding(
                        padding: const EdgeInsets.all(4.0),
                        child: Text('Keys added so far:'),
                      ),
                      const SizedBox(height: 12),
                      Center(
                        child: ConstrainedBox(
                          constraints: BoxConstraints(maxWidth: 600),
                          child: Column(
                            mainAxisSize: MainAxisSize.min,
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              devicesColumn,
                              const SizedBox(height: 8),
                              Align(
                                alignment: AlignmentDirectional.centerEnd,
                                child: TextButton.icon(
                                  icon: const Icon(Icons.add),
                                  label: const Text('Add another key'),
                                  onPressed: () {
                                    continueWalletRecoveryFlowDialog(
                                      context,
                                      restorationId: restoringKey.restorationId,
                                    );
                                  },
                                ),
                              ),
                              SizedBox(height: 16),
                              progressActionCard,
                            ],
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class WalletRecoveryFlow extends StatefulWidget {
  // We're continuing a restoration session
  final RestorationId? continuing;
  // We're recovering a share for a key that already exists
  final AccessStructureRef? existing;
  final bool isDialog;
  final RecoveryFlowStep? initialStep;

  const WalletRecoveryFlow({
    super.key,
    this.continuing,
    this.existing,
    this.isDialog = true,
  }) : initialStep = null;

  const WalletRecoveryFlow.startWithDevice({
    super.key,
    this.continuing,
    this.existing,
    this.isDialog = true,
  }) : initialStep = RecoveryFlowStep.waitDevice;

  const WalletRecoveryFlow.startWithPhysicalBackup({
    super.key,
    this.continuing,
    this.existing,
    this.isDialog = true,
  }) : initialStep = RecoveryFlowStep.enterRestorationDetails;

  @override
  State<WalletRecoveryFlow> createState() => _WalletRecoveryFlowState();
}

class _RecoveryFlowPrevState {
  RecoveryFlowStep currentStep = RecoveryFlowStep.start;
  RecoverShare? candidate;
  ShareCompatibility? compatibility;
  ConnectedDevice? blankDevice;
  RestorationId? restorationId;
  String? error;

  _RecoveryFlowPrevState({
    required this.currentStep,
    required this.candidate,
    required this.compatibility,
    required this.blankDevice,
    required this.restorationId,
    required this.error,
  });
}

class _WalletRecoveryFlowState extends State<WalletRecoveryFlow> {
  late final MethodChoiceKind kind;

  RecoveryFlowStep currentStep = RecoveryFlowStep.start;
  RecoverShare? candidate;
  ShareCompatibility? compatibility;
  ConnectedDevice? blankDevice;
  RestorationId? restorationId;
  String? walletName;
  BitcoinNetwork? bitcoinNetwork;
  int? threshold;
  String? error;
  StreamSubscription<DeviceListUpdate>? _deviceListSubscription;

  // For back gesture.
  final prevStates = List<_RecoveryFlowPrevState>.empty(growable: true);
  bool isAnimationForward = true;
  void pushPrevState() {
    isAnimationForward = true;
    prevStates.add(
      _RecoveryFlowPrevState(
        currentStep: currentStep,
        candidate: candidate,
        compatibility: compatibility,
        blankDevice: blankDevice,
        restorationId: restorationId,
        error: error,
      ),
    );
  }

  bool tryPopPrevState(BuildContext context) {
    if (prevStates.isNotEmpty) {
      setState(() {
        isAnimationForward = false;
        final prevState = prevStates.removeLast();
        currentStep = prevState.currentStep;
        candidate = prevState.candidate;
        compatibility = prevState.compatibility;
        blankDevice = prevState.blankDevice;
        restorationId = prevState.restorationId;
        error = prevState.error;

        // Cancel any active operations when going back
        coord.cancelProtocol();
      });
      return true;
    }
    return false;
  }

  @override
  void dispose() {
    _deviceListSubscription?.cancel();
    super.dispose();
  }

  void _setBlankDeviceAndMonitor(ConnectedDevice device) {
    // Cancel any existing subscription
    _deviceListSubscription?.cancel();

    blankDevice = device;

    // Start monitoring for disconnection
    _deviceListSubscription = GlobalStreams.deviceListSubject.listen((update) {
      // Check if our blank device is still connected
      final stillConnected = update.state.devices.any(
        (d) => deviceIdEquals(d.id, device.id),
      );

      if (!stillConnected && blankDevice != null && mounted) {
        // Device disconnected - reset to waiting state
        setState(() {
          blankDevice = null;
          _deviceListSubscription?.cancel();
          _deviceListSubscription = null;
          currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
          error = null; // Clear error since disconnection is expected
        });
      }
    });
  }

  void _clearBlankDevice() {
    blankDevice = null;
    _deviceListSubscription?.cancel();
    _deviceListSubscription = null;
  }

  @override
  void initState() {
    super.initState();

    if (widget.initialStep != null) {
      currentStep = widget.initialStep!;
    }

    if (widget.continuing != null) {
      kind = MethodChoiceKind.continueRecovery;
      restorationId = widget.continuing!;
      final state = coord.getRestorationState(restorationId: restorationId!)!;
      threshold = state.accessStructure.threshold;
      walletName = state.keyName;
      bitcoinNetwork =
          state.keyPurpose.bitcoinNetwork() ?? BitcoinNetwork.bitcoin;
    } else if (widget.existing != null) {
      kind = MethodChoiceKind.addToWallet;
    } else {
      kind = MethodChoiceKind.startRecovery;
    }
  }

  @override
  Widget build(BuildContext context) {
    _TitledWidget child;

    switch (currentStep) {
      case RecoveryFlowStep.waitDevice:
        child = _PlugInPromptView(
          continuing: widget.continuing,
          existing: widget.existing,
          onCandidateDetected: (detectedShare, compatibility) {
            if (mounted) {
              setState(() {
                pushPrevState();
                candidate = detectedShare;
                this.compatibility = compatibility;
                currentStep = RecoveryFlowStep.candidateReady;
              });
            }
          },
        );
        break;
      case RecoveryFlowStep.candidateReady:
        child = _CandidateReadyView(
          candidate: candidate!,
          compatibility: compatibility!,
          continuing: widget.continuing,
          existing: widget.existing,
          onDeviceDisconnected: () {
            // Device disconnected during nonce generation - go back to waiting
            setState(() {
              candidate = null;
              compatibility = null;
              currentStep = RecoveryFlowStep.waitDevice;
            });
          },
        );
        break;
      case RecoveryFlowStep.waitPhysicalBackupDevice:
        child = _PlugInBlankView(
          onBlankDeviceConnected: (device) {
            setState(() {
              _setBlankDeviceAndMonitor(device);
              // Check if firmware upgrade is needed
              if (device.needsFirmwareUpgrade()) {
                currentStep = RecoveryFlowStep.firmwareUpgrade;
              } else {
                currentStep = RecoveryFlowStep.enterDeviceName;
              }
            });
          },
        );
        break;

      case RecoveryFlowStep.firmwareUpgrade:
        child = _FirmwareUpgradeView(
          device: blankDevice!,
          onComplete: () {
            setState(() {
              currentStep = RecoveryFlowStep.enterDeviceName;
            });
          },
          onCancel: () {
            setState(() {
              _clearBlankDevice();
              currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
            });
          },
        );
        break;

      case RecoveryFlowStep.enterDeviceName:
        // Check if device is still connected
        if (blankDevice == null) {
          child = _ErrorView(
            title: 'Device Disconnected',
            message:
                'The device was disconnected. Please reconnect and try again.',
            onRetry: () {
              setState(() {
                currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
                error = null;
              });
            },
          );
          break;
        }

        child = _EnterDeviceNameView(
          deviceId: blankDevice!.id,
          onDeviceName: (name) {
            // Check nonces synchronously, just like we do for existing device recovery
            final device = blankDevice;
            if (device == null) {
              setState(() {
                error = 'Device disconnected';
                currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
              });
              return;
            }

            final nonceRequest = coord.createNonceRequest(devices: [device.id]);

            if (nonceRequest.someNoncesRequested()) {
              // Show nonce generation UI immediately
              setState(() {
                pushPrevState();
                currentStep = RecoveryFlowStep.generatingNonces;
              });
            } else {
              // Go straight to backup entry
              setState(() {
                pushPrevState();
                currentStep = RecoveryFlowStep.enterBackup;
              });
            }
          },
        );

      case RecoveryFlowStep.generatingNonces:
        final device = blankDevice;
        if (device == null) {
          // Device disconnected, go back
          setState(() {
            currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
          });
          child = _LoadingView(title: 'Device disconnected', subtitle: null);
        } else {
          // Create nonce stream here and pass to dialog
          final nonceRequest = coord.createNonceRequest(devices: [device.id]);
          final stream = coord
              .replenishNonces(
                nonceRequest: nonceRequest,
                devices: [device.id],
              )
              .toBehaviorSubject();

          child = _EnrollmentNonceDialog(
            stream: stream,
            deviceName: coord.getDeviceName(id: device.id),
            onComplete: () {
              setState(() {
                currentStep = RecoveryFlowStep.enterBackup;
              });
            },
            onCancel: () {
              coord.cancelProtocol();
              setState(() {
                _clearBlankDevice();
                currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
              });
            },
            onError: (error) {
              setState(() {
                // Don't set error for device disconnection - just go back
                if (!error.contains('disconnected')) {
                  this.error = error;
                }
                _clearBlankDevice();
                currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
              });
            },
          );
        }
        break;

      case RecoveryFlowStep.enterBackup:
        final stream = coord.tellDeviceToEnterPhysicalBackup(
          deviceId: blankDevice!.id,
        );
        child = _EnterBackupView(
          stream: stream,
          onFinished: (backupPhase) async {
            try {
              if (kind == MethodChoiceKind.addToWallet) {
                final encryptionKey =
                    await SecureKeyProvider.getEncryptionKey();
                await coord.tellDeviceToConsolidatePhysicalBackup(
                  accessStructureRef: widget.existing!,
                  phase: backupPhase,
                  encryptionKey: encryptionKey,
                );
              } else {
                restorationId ??= await coord.startRestoringWallet(
                  name: walletName!,
                  threshold: threshold!,
                  network: bitcoinNetwork!,
                );

                compatibility = coord.checkPhysicalBackupCompatible(
                  restorationId: restorationId!,
                  phase: backupPhase,
                );

                if (compatibility == ShareCompatibility.compatible()) {
                  await coord.tellDeviceToSavePhysicalBackup(
                    phase: backupPhase,
                    restorationId: restorationId!,
                  );
                  setState(() {
                    pushPrevState();
                    currentStep = RecoveryFlowStep.physicalBackupSuccess;
                  });
                } else {
                  // Incompatible backup - show fail screen with retry option
                  setState(() {
                    pushPrevState();
                    currentStep = RecoveryFlowStep.physicalBackupFail;
                  });
                }
              }
            } catch (e) {
              // Error during backup save - go back to waiting for device
              setState(() {
                _clearBlankDevice();
                currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
                error = e.toString();
              });
            }
          },
          onError: (e) {
            // Device disconnected or other error - go back to waiting for device
            setState(() {
              _clearBlankDevice();
              currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
              error =
                  null; // Clear error since disconnection is expected behavior
            });
          },
        );
        break;
      case RecoveryFlowStep.enterRestorationDetails:
        child = _EnterWalletNameView(
          initialWalletName: walletName,
          initialBitcoinNetwork: bitcoinNetwork,
          onWalletNameEntered: (walletName, bitcoinNetwork) {
            setState(() {
              pushPrevState();
              this.walletName = walletName;
              this.bitcoinNetwork = bitcoinNetwork;
              currentStep = RecoveryFlowStep.enterThreshold;
            });
          },
        );
        break;

      case RecoveryFlowStep.enterThreshold:
        child = _EnterThresholdView(
          walletName: walletName!,
          network: bitcoinNetwork!,
          initialThreshold: threshold,
          onThresholdEntered: (threshold) {
            setState(() {
              pushPrevState();
              this.threshold = threshold;
              currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
            });
          },
        );
        break;
      case RecoveryFlowStep.physicalBackupSuccess:
        child = _PhysicalBackupSuccessView(
          deviceName: coord.getDeviceName(id: blankDevice!.id)!,
          onClose: () {
            Navigator.pop(context);
          },
        );
        break;
      case RecoveryFlowStep.physicalBackupFail:
        child = _PhysicalBackupFailView(
          errorMessage: error,
          compatibility: compatibility,
          onRetry: () {
            setState(() {
              // Don't push prev state, just go back to enterBackup
              currentStep = RecoveryFlowStep.enterBackup;
              error = null;
            });
          },
          onClose: () {
            Navigator.pop(context);
          },
        );
        break;
      default:
        child = _ChooseMethodView(
          kind: kind,
          onDeviceChosen: () {
            setState(() {
              pushPrevState();
              currentStep = RecoveryFlowStep.waitDevice;
            });
          },
          onPhysicalBackupChosen: () {
            setState(() {
              switch (kind) {
                case MethodChoiceKind.startRecovery:
                  pushPrevState();
                  currentStep = RecoveryFlowStep.enterRestorationDetails;
                  break;
                default:
                  pushPrevState();
                  currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
              }
            });
          },
        );
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
        key: ValueKey(currentStep),
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
      // TODO: This branch can be removed at some point.
      return Dialog(
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
        child: ConstrainedBox(
          constraints: const BoxConstraints(
            minWidth: 480, // Choose a suitable fixed width
            maxWidth: 480,
            minHeight:
                360, // Ensure this is large enough for your tallest content
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

enum MethodChoiceKind { startRecovery, continueRecovery, addToWallet }

// Recovery flow step states
enum RecoveryFlowStep {
  start,
  waitDevice,
  candidateReady,
  waitPhysicalBackupDevice,
  firmwareUpgrade,
  enterDeviceName,
  generatingNonces,
  enterBackup,
  enterRestorationDetails,
  enterThreshold,
  physicalBackupSuccess,
  physicalBackupFail,
}

mixin _TitledWidget on Widget {
  String get titleText;
}

class MaterialDialogCard extends StatelessWidget {
  final IconData? iconData;
  final Widget title;
  final Widget content;
  final List<Widget> actions;
  final MainAxisAlignment actionsAlignment;
  final Color? backgroundColor;
  final Color? textColor;
  final Color? variantTextColor;
  final Color? iconColor;

  const MaterialDialogCard({
    super.key,
    this.iconData,
    required this.title,
    required this.content,
    required this.actions,
    this.actionsAlignment = MainAxisAlignment.end,
    this.backgroundColor,
    this.textColor,
    this.variantTextColor,
    this.iconColor,
  });

  static const borderRadius = BorderRadius.all(Radius.circular(24));

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      color: backgroundColor ?? theme.colorScheme.surfaceContainerHigh,
      shape: RoundedRectangleBorder(borderRadius: borderRadius),
      child: Padding(
        padding: EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          spacing: 16,
          children: [
            if (iconData != null)
              Icon(
                iconData,
                color: iconColor ?? theme.colorScheme.secondary,
                size: 24,
              ),
            DefaultTextStyle(
              style: theme.textTheme.headlineSmall!.copyWith(
                color: textColor ?? theme.colorScheme.onSurface,
              ),
              textAlign: TextAlign.center,
              child: title,
            ),
            DefaultTextStyle(
              style: theme.textTheme.bodyLarge!.copyWith(
                color: variantTextColor ?? theme.colorScheme.onSurfaceVariant,
              ),
              textAlign: TextAlign.start,
              child: content,
            ),
            Padding(
              padding: EdgeInsets.only(top: 8),
              child: Row(
                mainAxisAlignment: actionsAlignment,
                spacing: 8,
                children: actions.map((w) => Flexible(child: w)).toList(),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _ChooseMethodView extends StatelessWidget with _TitledWidget {
  final VoidCallback? onDeviceChosen;
  final VoidCallback? onPhysicalBackupChosen;
  final MethodChoiceKind kind;

  const _ChooseMethodView({
    required this.kind,
    this.onDeviceChosen,
    this.onPhysicalBackupChosen,
  });

  @override
  String get titleText => switch (kind) {
    MethodChoiceKind.startRecovery => 'Add the first key',
    MethodChoiceKind.continueRecovery => 'Add another key',
    MethodChoiceKind.addToWallet => 'Add another key',
  };

  @override
  Widget build(BuildContext context) {
    final String subtitle;

    switch (kind) {
      case MethodChoiceKind.startRecovery:
        // subtitle = 'What kind of key will you start restoring the wallet from?';
        subtitle =
            'Select how you’d like to provide the first key for this wallet.';
        break;
      case MethodChoiceKind.continueRecovery:
        // subtitle = 'Where is the next key coming from?';
        subtitle =
            'Select how you’d like to provide the next key for this wallet.';
        break;

      case MethodChoiceKind.addToWallet:
        subtitle =
            'Select how you’d like to provide the key for this wallet.\n\n⚠ For now, Frostsnap only supports adding keys that were originally part of the wallet when it was created';
        break;
    }

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        WalletAddColumn.buildTitle(context, text: subtitle),

        WalletAddColumn.buildCard(
          context,
          action: onDeviceChosen,
          icon: ImageIcon(
            AssetImage('assets/icons/device2.png'),
            size: WalletAddColumn.iconSize,
          ),
          title: 'Use existing device',
          subtitle: 'Connect a Frostsnap device that already has a key.',
          groupPosition: VerticalButtonGroupPosition.top,
        ),
        WalletAddColumn.buildCard(
          context,
          action: onPhysicalBackupChosen,
          icon: Icon(
            Icons.description_outlined,
            size: WalletAddColumn.iconSize,
          ),
          title: 'Load from backup',
          subtitle: 'Use a blank Frostsnap device with your physical backup.',
          groupPosition: VerticalButtonGroupPosition.bottom,
        ),
      ],
    );
  }
}

class _PlugInBlankView extends StatefulWidget with _TitledWidget {
  final Function(ConnectedDevice)? onBlankDeviceConnected;

  const _PlugInBlankView({this.onBlankDeviceConnected});

  @override
  State<_PlugInBlankView> createState() => _PlugInBlankViewState();

  @override
  String get titleText => 'Insert blank device';
}

class _PlugInBlankViewState extends State<_PlugInBlankView> {
  StreamSubscription? _subscription;
  ConnectedDevice? _connectedDevice;

  late final FullscreenActionDialogController<void> _eraseController;

  @override
  void initState() {
    super.initState();
    _subscription = GlobalStreams.deviceListSubject.listen((update) async {
      ConnectedDevice? connectedDevice;
      for (final candidate in update.state.devices) {
        connectedDevice = candidate;
        if (connectedDevice.name == null) {
          break;
        }
      }
      setState(() {
        _connectedDevice = connectedDevice;
      });
      if (connectedDevice != null && connectedDevice.name == null) {
        widget.onBlankDeviceConnected?.call(connectedDevice);
      }

      // For erase fullscreen action controller.
      if (connectedDevice != null) {
        final device = update.state.devices.firstWhereOrNull(
          (device) => deviceIdEquals(device.id, connectedDevice!.id),
        );
        if (device?.name == null) {
          await _eraseController.clearAllActionsNeeded();
        }
      } else {
        await _eraseController.clearAllActionsNeeded();
      }
    });
    _eraseController = FullscreenActionDialogController(
      title: 'Erase Device',
      body: (context) {
        final theme = Theme.of(context);
        return Card.filled(
          margin: EdgeInsets.zero,
          color: theme.colorScheme.errorContainer,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              ListTile(
                leading: Icon(Icons.warning_rounded),
                title: Text('This will wipe the key from the device.'),
                subtitle: Text(
                  'The device will be rendered blank.\nThis action can not be reverted, and the only way to restore this key is through loading of a backup.',
                ),
                isThreeLine: true,
                textColor: theme.colorScheme.onErrorContainer,
                iconColor: theme.colorScheme.onErrorContainer,
                contentPadding: EdgeInsets.symmetric(horizontal: 16),
              ),
            ],
          ),
        );
      },
      actionButtons: [
        OutlinedButton(child: Text('Cancel'), onPressed: _onCancel),
        DeviceActionHint(),
      ],
      onDismissed: _onCancel,
    );
  }

  void _onCancel() async {
    final id = _connectedDevice?.id;
    if (id != null) await coord.sendCancel(id: id);
    await _eraseController.clearAllActionsNeeded();
  }

  void showEraseDialog(BuildContext context, DeviceId id) async {
    _eraseController.addActionNeeded(context, id);
    await coord.wipeDeviceData(deviceId: id);
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _eraseController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final List<Widget> children;
    final theme = Theme.of(context);
    if (_connectedDevice != null && _connectedDevice!.name != null) {
      var name = _connectedDevice!.name!;
      children = [
        MaterialDialogCard(
          iconData: Icons.warning_rounded,
          title: Text('Device not blank'),
          content: Text(
            'This device already has data on it. To load a physical backup, it must be erased. Erasing will permanently delete all keys on “${name}”.',
          ),
          actions: [
            FilledButton.icon(
              style: FilledButton.styleFrom(
                backgroundColor: theme.colorScheme.error,
                foregroundColor: theme.colorScheme.onError,
              ),
              icon: Icon(Icons.delete),
              label: Text("Erase “$name”"),
              onPressed: () {
                showEraseDialog(context, _connectedDevice!.id);
              },
            ),
          ],
        ),
      ];
    } else {
      children = [
        MaterialDialogCard(
          iconData: Icons.usb_rounded,
          title: Text('Waiting for device'),
          content: Text(
            'Plug in a blank Frostsnap device. This device will be used to load your key from backup.',
          ),
          actions: [CircularProgressIndicator()],
          actionsAlignment: MainAxisAlignment.center,
        ),
      ];
    }
    return Column(
      key: const ValueKey('plugInBlankPrompt'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: children,
    );
  }
}

class _EnterWalletNameView extends StatefulWidget with _TitledWidget {
  final String? initialWalletName;
  final BitcoinNetwork? initialBitcoinNetwork;
  final Function(String walletName, BitcoinNetwork network) onWalletNameEntered;

  const _EnterWalletNameView({
    required this.onWalletNameEntered,
    this.initialWalletName,
    this.initialBitcoinNetwork,
  });

  @override
  State<_EnterWalletNameView> createState() => _EnterWalletNameViewState();

  @override
  String get titleText => 'Wallet name';
}

class _EnterWalletNameViewState extends State<_EnterWalletNameView> {
  final _formKey = GlobalKey<FormState>();
  final _walletNameController = TextEditingController();
  BitcoinNetwork bitcoinNetwork = BitcoinNetwork.bitcoin;
  bool _isButtonEnabled = false;

  @override
  void initState() {
    super.initState();
    _walletNameController.addListener(_updateButtonState);
    final initialWalletName = widget.initialWalletName;
    if (initialWalletName != null) {
      _walletNameController.text = initialWalletName;
    }
    final initialBitcoinNetwork = widget.initialBitcoinNetwork;
    if (initialBitcoinNetwork != null) {
      bitcoinNetwork = initialBitcoinNetwork;
    }
  }

  void _updateButtonState() {
    setState(() {
      _isButtonEnabled = _walletNameController.text.isNotEmpty;
    });
  }

  void _submitForm() {
    if (_isButtonEnabled && _formKey.currentState!.validate()) {
      widget.onWalletNameEntered(_walletNameController.text, bitcoinNetwork);
    }
  }

  @override
  void dispose() {
    _walletNameController.removeListener(_updateButtonState);
    _walletNameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final developerMode = SettingsContext.of(
      context,
    )!.settings.isInDeveloperMode();

    return Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            "Enter the wallet name from your physical backup.\nIf it’s missing or unreadable, choose another name — this won’t affect your wallet’s security.",
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 24),
          TextFormField(
            controller: _walletNameController,
            decoration: const InputDecoration(
              labelText: 'Wallet Name',
              border: OutlineInputBorder(),
              hintText: 'The name of the wallet being restored',
            ),
            onChanged: (_) => _updateButtonState(),
            onFieldSubmitted: (_) => _submitForm(),
            validator: (value) {
              if (value == null || value.isEmpty) {
                return 'Please enter a wallet name';
              }
              return null;
            },
          ),
          if (developerMode) ...[
            SizedBox(height: 16),
            BitcoinNetworkChooser(
              value: bitcoinNetwork,
              onChanged: (network) {
                setState(() => bitcoinNetwork = network);
              },
            ),
          ],
          const SizedBox(height: 24),
          Align(
            alignment: AlignmentDirectional.centerEnd,
            child: FilledButton(
              child: const Text('Continue'),
              onPressed: _isButtonEnabled ? _submitForm : null,
            ),
          ),
        ],
      ),
    );
  }
}

class _EnterThresholdView extends StatefulWidget with _TitledWidget {
  final String walletName;
  final BitcoinNetwork network;
  final Function(int threshold) onThresholdEntered;
  final int? initialThreshold;

  const _EnterThresholdView({
    required this.walletName,
    required this.onThresholdEntered,
    required this.network,
    this.initialThreshold,
  });

  @override
  State<_EnterThresholdView> createState() => _EnterThresholdViewState();

  @override
  String get titleText => 'Wallet Threshold';
}

class _EnterThresholdViewState extends State<_EnterThresholdView> {
  final _formKey = GlobalKey<FormState>();
  int _threshold = 2; // Default value

  @override
  void initState() {
    super.initState();
    final initialThreshold = widget.initialThreshold;
    if (initialThreshold != null) {
      _threshold = initialThreshold;
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            "Enter the threshold from your backup.\nThis is the number of keys needed to use your wallet.",
            style: theme.textTheme.bodyMedium,
          ),
          const SizedBox(height: 24),
          DropdownButtonFormField<int>(
            initialValue: _threshold,
            decoration: const InputDecoration(
              labelText: 'Threshold',
              border: OutlineInputBorder(),
              hintText: 'Number of keys needed',
            ),
            items: List.generate(5, (index) => index + 1)
                .map(
                  (number) => DropdownMenuItem<int>(
                    value: number,
                    child: Text('$number key${number > 1 ? 's' : ''}'),
                  ),
                )
                .toList(),
            onChanged: (value) {
              if (value != null) {
                setState(() {
                  _threshold = value;
                });
              }
            },
            validator: (value) {
              if (value == null) {
                return 'Please select a threshold';
              }
              return null;
            },
          ),
          const SizedBox(height: 24),
          Align(
            alignment: AlignmentDirectional.centerEnd,
            child: FilledButton(
              child: const Text('Begin restoring'),
              onPressed: () async {
                if (_formKey.currentState!.validate()) {
                  widget.onThresholdEntered(_threshold);
                }
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _EnterDeviceNameView extends StatelessWidget with _TitledWidget {
  final Function(String)? onDeviceName;
  final DeviceId deviceId;
  const _EnterDeviceNameView({required this.deviceId, this.onDeviceName});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          "If in doubt you can use the name written on the backup or make up a new one. You can always rename it later.",
          style: theme.textTheme.bodyMedium,
        ),
        const SizedBox(height: 16),
        DeviceNameField(
          id: deviceId,
          mode: DeviceNameMode.preview,
          buttonText: 'Continue',
          onNamed: (name) {
            onDeviceName?.call(name);
          },
        ),
      ],
    );
  }

  @override
  String get titleText => 'Device name';
}

class _EnterBackupView extends StatefulWidget with _TitledWidget {
  final Stream<EnterPhysicalBackupState> stream;
  final Function(PhysicalBackupPhase)? onFinished;
  final Function(String)? onError;

  const _EnterBackupView({required this.stream, this.onFinished, this.onError});

  @override
  State<_EnterBackupView> createState() => _EnterBackupViewState();

  @override
  String get titleText => 'Enter backup on device';
}

class _EnterBackupViewState extends State<_EnterBackupView> {
  StreamSubscription? _subscription;
  bool saved = false;

  @override
  void initState() {
    super.initState();
    _subscription = widget.stream.listen((state) async {
      if (state.entered != null) {
        await _subscription?.cancel();
        widget.onFinished?.call(state.entered!);
      }
      if (state.abort != null) {
        widget.onError?.call(state.abort!);
      }
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      key: const ValueKey('EnterBackup'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        MaterialDialogCard(
          iconData: Icons.keyboard_rounded,
          title: Text('Waiting for backup'),
          content: Text(
            'Use your Frostsnap device to enter the physical backup. The app will continue once it’s complete.',
          ),
          actions: [CircularProgressIndicator()],
          actionsAlignment: MainAxisAlignment.center,
        ),
      ],
    );
  }
}

class _PlugInPromptView extends StatefulWidget with _TitledWidget {
  final RestorationId? continuing;
  final AccessStructureRef? existing;
  final void Function(RecoverShare candidate, ShareCompatibility compatibility)
  onCandidateDetected;

  const _PlugInPromptView({
    this.continuing,
    this.existing,
    required this.onCandidateDetected,
  });

  @override
  String get titleText => 'Restore with existing device';

  @override
  State<_PlugInPromptView> createState() => _PlugInPromptViewState();
}

class _PlugInPromptViewState extends State<_PlugInPromptView> {
  late StreamSubscription _subscription;
  bool blankDeviceInserted = false;
  RecoverShare? alreadyGot;

  @override
  void initState() {
    super.initState();

    _subscription = coord.waitForRecoveryShare().listen((
      waitForRecoverShareState,
    ) async {
      ShareCompatibility? compatibility;
      RecoverShare? last;
      alreadyGot = null;
      blankDeviceInserted = false;

      if (waitForRecoverShareState.shares.isNotEmpty) {
        for (final detectedShare in waitForRecoverShareState.shares) {
          last = detectedShare;
          if (widget.continuing != null) {
            compatibility = coord.restorationCheckShareCompatible(
              restorationId: widget.continuing!,
              recoverShare: detectedShare,
            );
          } else if (widget.existing != null) {
            final encryptionKey = await SecureKeyProvider.getEncryptionKey();
            compatibility = coord.checkRecoverShareCompatible(
              accessStructureRef: widget.existing!,
              recoverShare: detectedShare,
              encryptionKey: encryptionKey,
            );
          } else {
            compatibility = ShareCompatibility.compatible();
          }

          if (compatibility == ShareCompatibility.compatible()) {
            break;
          }
        }

        if (compatibility == ShareCompatibility.alreadyGotIt()) {
          setState(() {
            alreadyGot = last!;
          });
        } else {
          setState(() {
            widget.onCandidateDetected(last!, compatibility!);
          });
        }
      } else {
        setState(() {
          blankDeviceInserted = waitForRecoverShareState.blank.isNotEmpty;
        });
      }
    });
  }

  @override
  void dispose() {
    _subscription.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    // Build the widget to display based on the error state.
    Widget displayWidget;
    if (alreadyGot != null) {
      displayWidget = MaterialDialogCard(
        key: const ValueKey('warning-already-got'),
        backgroundColor: theme.colorScheme.surfaceContainerLow,
        iconData: Icons.warning_amber,
        title: Text('Key already part of wallet'),
        content: Text(
          "The connected device “${coord.getDeviceName(id: alreadyGot!.heldBy)}” is already part of the wallet “${alreadyGot!.heldShare.keyName}”.",

          textAlign: TextAlign.center,
        ),
        actions: [],
      );
    } else if (blankDeviceInserted) {
      displayWidget = MaterialDialogCard(
        key: const ValueKey('warning-blank'),
        backgroundColor: theme.colorScheme.surfaceContainerLow,
        iconData: Icons.warning_amber_rounded,
        title: Text('Empty Device'),
        content: Text(
          'The device you plugged in has no key on it.',
          textAlign: TextAlign.center,
        ),
        actions: [],
      );
    } else {
      // No error: show the spinner centered within the space.
      displayWidget = Semantics(
        label: 'Waiting for device to connect',
        child: CircularProgressIndicator(),
      );
    }

    final String prompt;

    if (widget.continuing != null) {
      final name = coord
          .getRestorationState(restorationId: widget.continuing!)!
          .keyName;
      prompt = 'Plug in a Frostsnap to continue restoring "$name".';
    } else if (widget.existing != null) {
      final name = coord.getFrostKey(keyId: widget.existing!.keyId)!.keyName();
      prompt = 'Plug in a Frostsnap to add it to "$name".';
    } else {
      prompt = 'Plug in your Frostsnap device\nto begin wallet restoration.';
    }

    return Column(
      key: const ValueKey('plugInPrompt'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        MaterialDialogCard(
          iconData: Icons.usb_rounded,
          title: Text('Waiting for device'),
          content: Text(prompt, textAlign: TextAlign.center),
          actions: [
            AnimatedSize(
              duration: Durations.short4,
              curve: Curves.easeInOutCubicEmphasized,
              child: displayWidget,
            ),
          ],
          actionsAlignment: MainAxisAlignment.center,
        ),
      ],
    );
  }
}

class _CandidateReadyView extends StatefulWidget with _TitledWidget {
  final RecoverShare candidate;
  final ShareCompatibility compatibility;
  final RestorationId? continuing;
  final AccessStructureRef? existing;
  final VoidCallback? onDeviceDisconnected;

  const _CandidateReadyView({
    required this.candidate,
    required this.compatibility,
    this.continuing,
    this.existing,
    this.onDeviceDisconnected,
  });

  @override
  String get titleText => 'Restore with existing key';

  @override
  State<_CandidateReadyView> createState() => _CandidateReadyViewState();
}

class _CandidateReadyViewState extends State<_CandidateReadyView> {
  bool _isGeneratingNonces = false;
  bool _isEnrolling = false; // Prevent duplicate enrollments
  Stream<NonceReplenishState>? _nonceStream;
  StreamSubscription? _nonceStreamSubscription;

  @override
  void dispose() {
    _nonceStreamSubscription?.cancel();
    super.dispose();
  }

  Future<void> _completeEnrollment(BuildContext context) async {
    // Prevent duplicate enrollments
    if (_isEnrolling) return;

    setState(() {
      _isEnrolling = true;
    });

    try {
      RestorationId? restorationId;
      if (widget.continuing != null) {
        await coord.continueRestoringWalletFromDeviceShare(
          restorationId: widget.continuing!,
          recoverShare: widget.candidate,
        );
      } else if (widget.existing != null) {
        final encryptionKey = await SecureKeyProvider.getEncryptionKey();
        await coord.recoverShare(
          accessStructureRef: widget.existing!,
          recoverShare: widget.candidate,
          encryptionKey: encryptionKey,
        );
      } else {
        restorationId = await coord.startRestoringWalletFromDeviceShare(
          recoverShare: widget.candidate,
        );
      }

      if (context.mounted) {
        Navigator.pop(context, restorationId);
      }
    } catch (e) {
      if (context.mounted) {
        setState(() {
          _isEnrolling = false; // Reset on error
        });
        showErrorSnackbar(context, "Failed to recover key: $e");
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final deviceName =
        coord.getDeviceName(id: widget.candidate.heldBy) ?? '<empty>';

    // If we're generating nonces, show the nonce UI inline
    if (_isGeneratingNonces && _nonceStream != null) {
      return Column(
        key: const ValueKey('nonceGeneration'),
        mainAxisSize: MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          MaterialDialogCard(
            title: SizedBox.shrink(),
            content: Container(
              constraints: BoxConstraints(minHeight: 120),
              child: StreamBuilder<NonceReplenishState>(
                stream: _nonceStream!,
                builder: (context, snapshot) {
                  final state = snapshot.data;

                  // Handle abort (device disconnection)
                  if (state != null && state.abort) {
                    // Device disconnected - go back to waiting for device
                    WidgetsBinding.instance.addPostFrameCallback((_) {
                      if (mounted) {
                        coord.cancelProtocol();
                        _nonceStreamSubscription?.cancel();
                        // Call parent callback to go back to waitDevice state
                        widget.onDeviceDisconnected?.call();
                      }
                    });
                    return SizedBox.shrink(); // Return early to avoid showing completion UI
                  }

                  // Handle completion
                  if (state != null) {
                    final isComplete =
                        state.receivedFrom.length == state.devices.length;
                    if (isComplete) {
                      // Add a delay to show completion before proceeding
                      Future.delayed(Durations.long1, () async {
                        if (mounted) {
                          await _nonceStreamSubscription?.cancel();
                          // Don't change UI state - just complete enrollment which will navigate
                          _completeEnrollment(context);
                        }
                      });
                    }
                  }

                  return MinimalNonceReplenishWidget(
                    stream: _nonceStream!,
                    autoAdvance: false,
                  );
                },
              ),
            ),
            actions:
                [], // No cancel button - user can disconnect device to cancel
          ),
        ],
      );
    }

    // Otherwise show the normal "Key ready" UI
    IconData icon;
    String title;
    String message;
    String buttonText;
    bool buttonFilled;
    VoidCallback? buttonAction;

    switch (widget.compatibility) {
      case ShareCompatibility_Compatible() ||
          // we ignore the problem of different wallet names on the shares for now.
          // This happens when you eneter a physical backup and enter a different
          // name for the wallet than devices you later try to add to the wallet.
          // We just carry on with the cosmetic SNAFU.
          ShareCompatibility_NameMismatch():
        icon = Icons.check_circle;
        title = 'Key ready';

        if (widget.continuing != null || widget.existing != null) {
          message =
              'Key \'$deviceName\' is ready to be added to wallet \'${widget.candidate.heldShare.keyName}\'.';
          buttonText = _isEnrolling ? 'Adding...' : 'Add to wallet';
        } else {
          message =
              'Key \'$deviceName\' is part of a wallet called \'${widget.candidate.heldShare.keyName}\'.';
          buttonText = _isEnrolling ? 'Starting...' : 'Start restoring';
        }

        buttonFilled = true;
        buttonAction = _isEnrolling
            ? null
            : () {
                // Prevent duplicate clicks
                if (_isEnrolling) return;

                // Run async operations in a fire-and-forget manner
                () async {
                  try {
                    // Check if device needs nonces before enrolling
                    final nonceRequest = await coord.createNonceRequest(
                      devices: [widget.candidate.heldBy],
                    );

                    if (nonceRequest.someNoncesRequested()) {
                      // Show nonce generation inline instead of in a dialog
                      if (mounted) {
                        setState(() {
                          _isGeneratingNonces = true;
                          _nonceStream = coord
                              .replenishNonces(
                                nonceRequest: nonceRequest,
                                devices: [widget.candidate.heldBy],
                              )
                              .toBehaviorSubject();
                          // The abort flag in NonceReplenishState will handle device disconnection
                        });
                      }
                      return; // Don't proceed until nonces are done
                    }

                    // If no nonces needed, proceed directly
                    if (mounted) {
                      _completeEnrollment(context);
                    }
                  } catch (e) {
                    if (mounted) {
                      showErrorSnackbar(context, e.toString());
                    }
                  }
                }();
              };

        break;

      case ShareCompatibility_AlreadyGotIt():
        icon = Icons.info_rounded;
        title = 'Key already restored';
        message = "You've already restored '$deviceName'.";
        buttonText = 'Close';
        buttonFilled = false;
        buttonAction = () => Navigator.pop(context);
        break;

      case ShareCompatibility_Incompatible():
        icon = Icons.error_rounded;
        title = 'Key cannot be used';
        message =
            'This key "$deviceName" is part of a different wallet called "${widget.candidate.heldShare.keyName}".';
        buttonText = 'Close';
        buttonFilled = false;
        buttonAction = () => Navigator.pop(context);
        break;

      case ShareCompatibility_ConflictsWith(:final deviceId, :final index):
        icon = Icons.error_rounded;
        title = 'Backup does not match';
        message =
            "You have already restored backup #$index on '${coord.getDeviceName(id: deviceId)!}' and it doesn't match the one you just entered. Consider removing that key from the restoration first.";
        buttonText = 'Close';
        buttonFilled = false;
        buttonAction = () => Navigator.pop(context);
        break;
    }

    return MaterialDialogCard(
      key: const ValueKey('candidateReady'),
      iconData: icon,
      title: Text(title),
      content: Text(message, textAlign: TextAlign.center),
      actions: [
        buttonFilled
            ? FilledButton(child: Text(buttonText), onPressed: buttonAction)
            : TextButton(child: Text(buttonText), onPressed: buttonAction),
      ],
    );
  }
}

class _PhysicalBackupSuccessView extends StatelessWidget with _TitledWidget {
  final VoidCallback onClose;
  final String deviceName;

  const _PhysicalBackupSuccessView({
    required this.onClose,
    required this.deviceName,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      key: const ValueKey('physicalBackupSuccess'),
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.check_circle, size: 48, color: Colors.green),
        const SizedBox(height: 16),
        Text(
          'Physical backup restored successfully on to $deviceName!',
          style: theme.textTheme.headlineMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 24),
        ElevatedButton.icon(
          icon: const Icon(Icons.arrow_forward),
          label: const Text('Close'),
          onPressed: onClose,
        ),
      ],
    );
  }

  @override
  String get titleText => '';
}

class _PhysicalBackupFailView extends StatelessWidget with _TitledWidget {
  final String? errorMessage;
  final ShareCompatibility? compatibility;
  final VoidCallback onRetry;
  final VoidCallback onClose;

  const _PhysicalBackupFailView({
    required this.errorMessage,
    required this.compatibility,
    required this.onRetry,
    required this.onClose,
  });

  @override
  Widget build(BuildContext context) {
    final String compatMessage = switch (compatibility) {
      ShareCompatibility_ConflictsWith(:final deviceId, :final index) =>
        "You have already restored backup #$index on '${coord.getDeviceName(id: deviceId)!}' and it doesn't match the one you just entered. Try a different backup.",
      ShareCompatibility_Incompatible() =>
        "This backup is for a different wallet. Try a different backup.",
      ShareCompatibility_AlreadyGotIt() =>
        "You've already added this backup to the restoration. Try a different backup.",
      _ => "The backup is not compatible with this wallet.",
    };

    final String message = errorMessage ?? compatMessage;
    return MaterialDialogCard(
      key: const ValueKey('physicalBackupFail'),
      iconData: Icons.error_rounded,
      title: Text('Incompatible backup'),
      content: Text(message, textAlign: TextAlign.center),
      actions: [
        TextButton(child: const Text('Close'), onPressed: onClose),
        FilledButton(
          child: const Text('Try Different Backup'),
          onPressed: onRetry,
        ),
      ],
    );
  }

  @override
  String get titleText => 'Backup Error';
}

class _FirmwareUpgradeView extends StatefulWidget with _TitledWidget {
  final ConnectedDevice device;
  final VoidCallback onComplete;
  final VoidCallback onCancel;

  const _FirmwareUpgradeView({
    required this.device,
    required this.onComplete,
    required this.onCancel,
  });

  @override
  State<_FirmwareUpgradeView> createState() => _FirmwareUpgradeViewState();

  @override
  String get titleText => 'Firmware Upgrade Required';
}

class _FirmwareUpgradeViewState extends State<_FirmwareUpgradeView> {
  late final DeviceActionUpgradeController _controller;
  bool _isUpgrading = false;

  @override
  void initState() {
    super.initState();
    _controller = DeviceActionUpgradeController();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _startUpgrade() async {
    setState(() {
      _isUpgrading = true;
    });

    final success = await _controller.run(context);

    if (mounted) {
      if (success) {
        widget.onComplete();
      } else {
        // If upgrade fails or is cancelled, exit the entire flow
        Navigator.of(context).pop();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    // Simple prompt - the actual upgrade UI is handled by the controller's fullscreen dialog
    return MaterialDialogCard(
      key: const ValueKey('firmwareUpgradePrompt'),
      iconData: Icons.system_update_alt_rounded,
      title: Text('Firmware Update Required'),
      content: Text(
        'This device needs a firmware update before it can be used for wallet restoration.',
      ),
      actions: [
        TextButton(
          onPressed: _isUpgrading ? null : () => Navigator.of(context).pop(),
          child: Text('Cancel'),
        ),
        FilledButton(
          onPressed: _isUpgrading ? null : _startUpgrade,
          child: Text(_isUpgrading ? 'Upgrading...' : 'Upgrade Now'),
        ),
      ],
    );
  }
}

// Loading view with optional subtitle
class _LoadingView extends StatelessWidget with _TitledWidget {
  final String title;
  final String? subtitle;

  const _LoadingView({required this.title, this.subtitle});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedSwitcher(
      duration: Durations.medium2,
      child: Center(
        key: ValueKey('$title$subtitle'),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 24),
            AnimatedDefaultTextStyle(
              style: theme.textTheme.titleMedium!,
              duration: Durations.short4,
              child: Text(title, textAlign: TextAlign.center),
            ),
            if (subtitle != null) ...[
              SizedBox(height: 8),
              AnimatedDefaultTextStyle(
                style: theme.textTheme.bodyMedium!.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
                duration: Durations.short4,
                child: Text(subtitle!, textAlign: TextAlign.center),
              ),
            ],
          ],
        ),
      ),
    );
  }

  @override
  String get titleText => '';
}

// Error view with retry option
class _ErrorView extends StatelessWidget with _TitledWidget {
  final String title;
  final String message;
  final VoidCallback? onRetry;

  const _ErrorView({required this.title, required this.message, this.onRetry});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return MaterialDialogCard(
      iconData: Icons.error_outline_rounded,
      title: Text(title),
      content: Text(message, textAlign: TextAlign.center),
      backgroundColor: theme.colorScheme.errorContainer,
      textColor: theme.colorScheme.onErrorContainer,
      iconColor: theme.colorScheme.onErrorContainer,
      actions: onRetry != null
          ? [
              FilledButton.icon(
                icon: Icon(Icons.refresh),
                label: Text('Try Again'),
                onPressed: onRetry,
                style: FilledButton.styleFrom(
                  backgroundColor: theme.colorScheme.error,
                  foregroundColor: theme.colorScheme.onError,
                ),
              ),
            ]
          : [],
    );
  }

  @override
  String get titleText => 'Error';
}

// Unified nonce dialog for both enrollment and physical backup flows
class _EnrollmentNonceDialog extends StatefulWidget with _TitledWidget {
  final Stream<NonceReplenishState> stream;
  final String? deviceName;
  final VoidCallback onComplete;
  final VoidCallback onCancel;
  final Function(String) onError;

  const _EnrollmentNonceDialog({
    required this.stream,
    this.deviceName,
    required this.onComplete,
    required this.onCancel,
    required this.onError,
  });

  @override
  State<_EnrollmentNonceDialog> createState() => _EnrollmentNonceDialogState();

  @override
  String get titleText => 'Preparing Device';
}

class _EnrollmentNonceDialogState extends State<_EnrollmentNonceDialog> {
  bool _hasCompleted = false;
  bool _hasErrored = false;
  StreamSubscription? _streamSubscription;

  @override
  void dispose() {
    _streamSubscription?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return StreamBuilder<NonceReplenishState>(
      stream: widget.stream,
      builder: (context, snapshot) {
        // Handle stream errors
        if (snapshot.hasError && !_hasErrored) {
          _hasErrored = true;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) {
              try {
                widget.onError('Failed to prepare device: ${snapshot.error}');
              } catch (e) {
                debugPrint('Error in onError callback: $e');
              }
            }
          });
        }

        final state = snapshot.data;

        // Handle completion
        if (state != null && !_hasCompleted && !_hasErrored) {
          final isComplete = state.receivedFrom.length == state.devices.length;
          if (isComplete) {
            _hasCompleted = true;
            // Add a delay to show the completion state before transitioning
            Future.delayed(Durations.long1, () {
              if (mounted) {
                try {
                  widget.onComplete();
                } catch (e) {
                  debugPrint('Error in onComplete callback: $e');
                }
              }
            });
          }
        }

        // Handle abort
        if (state?.abort == true && !_hasCompleted && !_hasErrored) {
          _hasErrored = true;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) {
              try {
                widget.onError('Device disconnected during preparation');
              } catch (e) {
                debugPrint('Error in onError callback: $e');
              }
            }
          });
        }

        // Build compact dialog card without redundant icon/title
        return Column(
          key: const ValueKey('nonceGeneration'),
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            MaterialDialogCard(
              title: SizedBox.shrink(),
              content: Container(
                constraints: BoxConstraints(minHeight: 120),
                child: MinimalNonceReplenishWidget(
                  stream: widget.stream,
                  autoAdvance: false,
                ),
              ),
              actions:
                  [], // No cancel button - user can disconnect device to cancel
              actionsAlignment: MainAxisAlignment.center,
            ),
          ],
        );
      },
    );
  }
}

void continueWalletRecoveryFlowDialog(
  BuildContext context, {
  required RestorationId restorationId,
}) async {
  final homeCtx = HomeContext.of(context);
  await MaybeFullscreenDialog.show(
    context: context,
    barrierDismissible: true,
    child: WalletRecoveryFlow(continuing: restorationId, isDialog: false),
  );
  await coord.cancelProtocol();
  if (homeCtx == null) {
    print('NO HOME CONTEXT!');
    return;
  }
  homeCtx.walletListController.selectRecoveringWallet(restorationId);
}
