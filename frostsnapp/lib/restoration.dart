import 'dart:async';
import 'package:collection/collection.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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
import 'package:frostsnap/src/rust/api/name.dart';
import 'package:frostsnap/src/rust/api/nonce_replenish.dart';
import 'package:frostsnap/src/rust/api/recovery.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/theme.dart';
import 'package:frostsnap/wallet_add.dart';

class WalletRecoveryPage extends StatelessWidget {
  final RestorationState restorationState;
  final Function(AccessStructureRef) onWalletRecovered;

  const WalletRecoveryPage({
    super.key,
    required this.restorationState,
    required this.onWalletRecovered,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final homeCtx = HomeContext.of(context)!;

    // Get status to determine the restoration state from raw data
    final status = restorationState.status();
    final shareCount = status.shareCount();

    final isRecovered = status.sharedKey != null;
    final hasIncompatibleShares = shareCount.incompatible > 0;

    final sharesNeeded = shareCount.needed != null && !isRecovered
        ? shareCount.needed! - shareCount.got
        : null;
    final isReady = isRecovered && !hasIncompatibleShares;

    // Determine card properties based on state
    final IconData cardIcon;
    final String cardTitle;
    final String cardMessage;
    final Color cardBackgroundColor;
    final Color cardTextColor;

    if (isReady) {
      cardIcon = Icons.check_circle_outline_rounded;
      cardTitle = 'Ready to restore';
      cardMessage =
          "You have enough keys to restore the wallet. You can still continue adding keys now or add them later in the wallet's settings";
      cardBackgroundColor = theme.colorScheme.primaryContainer;
      cardTextColor = theme.colorScheme.onPrimaryContainer;
    } else if (hasIncompatibleShares && isRecovered) {
      cardIcon = Icons.error_outline_rounded;
      cardTitle = 'Remove incompatible shares';
      cardMessage =
          'You have enough compatible keys to restore, but some incompatible shares are present. Remove the incompatible shares before restoring.';
      cardBackgroundColor = theme.colorScheme.errorContainer;
      cardTextColor = theme.colorScheme.onErrorContainer;
    } else if (hasIncompatibleShares) {
      cardIcon = Icons.error_outline_rounded;
      cardTitle = 'Some shares are invalid';
      cardMessage =
          'At least one share is incompatible with the other shares. Try adding more shares.';
      cardBackgroundColor = theme.colorScheme.errorContainer;
      cardTextColor = theme.colorScheme.onErrorContainer;
    } else if (shareCount.needed == null) {
      cardIcon = Icons.info_rounded;
      cardTitle = 'Gathering keys';
      cardMessage = 'Add more keys to restore the wallet.';
      cardBackgroundColor = theme.colorScheme.secondaryContainer;
      cardTextColor = theme.colorScheme.onSecondaryContainer;
    } else if (sharesNeeded != null && sharesNeeded > 0) {
      cardIcon = Icons.info_rounded;
      cardTitle = 'Not enough shares';
      cardMessage = sharesNeeded == 1
          ? '1 more key to restore wallet.'
          : '$sharesNeeded more keys needed to restore wallet.';
      cardBackgroundColor = theme.colorScheme.secondaryContainer;
      cardTextColor = theme.colorScheme.onSecondaryContainer;
    } else {
      cardIcon = Icons.info_rounded;
      cardTitle = 'Not enough shares';
      cardMessage = 'Add more keys to restore wallet.';
      cardBackgroundColor = theme.colorScheme.secondaryContainer;
      cardTextColor = theme.colorScheme.onSecondaryContainer;
    }

    final progressActionCard = MaterialDialogCard(
      iconData: cardIcon,
      title: Text(cardTitle),
      content: Text(cardMessage),
      backgroundColor: cardBackgroundColor,
      textColor: cardTextColor,
      variantTextColor: cardTextColor,
      iconColor: cardTextColor,
      actions: [
        TextButton.icon(
          icon: const Icon(Icons.close_rounded),
          label: const Text('Cancel'),
          onPressed: () async {
            // Show confirmation if there are any shares
            if (status.shares.isNotEmpty) {
              final confirm = await showDialog<bool>(
                context: context,
                builder: (context) => AlertDialog(
                  title: Text('Cancel restoration?'),
                  content: Text(
                    'You have ${status.shares.length} key${status.shares.length > 1 ? 's' : ''} added. '
                    'Are you sure you want to cancel this restoration?',
                  ),
                  actions: [
                    TextButton(
                      onPressed: () => Navigator.of(context).pop(false),
                      child: Text('Keep restoring'),
                    ),
                    TextButton(
                      onPressed: () => Navigator.of(context).pop(true),
                      child: Text('Cancel restoration'),
                    ),
                  ],
                ),
              );
              if (confirm == true) {
                coord.cancelRestoration(
                  restorationId: restorationState.restorationId,
                );
              }
            } else {
              coord.cancelRestoration(
                restorationId: restorationState.restorationId,
              );
            }
          },
          style: TextButton.styleFrom(foregroundColor: cardTextColor),
        ),
        FilledButton.icon(
          icon: const Icon(Icons.check_rounded),
          label: const Text('Restore'),
          onPressed: isReady
              ? () async {
                  try {
                    final encryptionKey =
                        await SecureKeyProvider.getEncryptionKey();
                    final accessStructureRef = await coord.finishRestoring(
                      restorationId: restorationState.restorationId,
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
            TextSpan(text: restorationState.keyName),
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
      children: status.shares.map((share) {
        final deleteButton = IconButton(
          icon: const Icon(Icons.remove_circle_outline),
          tooltip: 'Remove key',
          onPressed: () async {
            // Show confirmation if this is a compatible share in a recovered wallet
            if (isRecovered &&
                share.compatibility == ShareCompatibility.compatible) {
              final confirm = await showDialog<bool>(
                context: context,
                builder: (context) => AlertDialog(
                  title: Text('Remove compatible key?'),
                  content: Text(
                    'This key is compatible with your wallet. '
                    'Are you sure you want to remove it?',
                  ),
                  actions: [
                    TextButton(
                      onPressed: () => Navigator.of(context).pop(false),
                      child: Text('Keep'),
                    ),
                    TextButton(
                      onPressed: () => Navigator.of(context).pop(true),
                      child: Text('Remove'),
                    ),
                  ],
                ),
              );
              if (confirm != true) return;
            }

            await coord.deleteRestorationShare(
              restorationId: restorationState.restorationId,
              deviceId: share.deviceId,
            );
            homeCtx.walletListController.selectRecoveringWallet(
              restorationState.restorationId,
            );
          },
        );
        final deviceName = coord.getDeviceName(id: share.deviceId) ?? '<empty>';
        return Card.filled(
          color: theme.colorScheme.surfaceContainerHigh,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(24)),
            side: share.compatibility == ShareCompatibility.compatible
                ? BorderSide(color: theme.colorScheme.primary, width: 2)
                : BorderSide.none,
          ),
          margin: EdgeInsets.zero,
          child: ListTile(
            contentPadding: EdgeInsets.symmetric(horizontal: 16),
            leading: Icon(Icons.key),
            trailing: deleteButton,
            subtitle: () {
              final icon;
              final text;
              final color;

              switch (share.compatibility) {
                case ShareCompatibility.compatible:
                  icon = Icons.check_circle;
                  text = 'Compatible';
                  color = Theme.of(context).colorScheme.primary;
                  break;
                case ShareCompatibility.incompatible:
                  icon = Icons.cancel;
                  text = 'Incompatible';
                  color = Theme.of(context).colorScheme.error;
                  break;
                case ShareCompatibility.uncertain:
                  icon = Icons.pending;
                  text = 'Compatibility uncertain';
                  color = Theme.of(context).colorScheme.onSurfaceVariant;
                  break;
              }

              return Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(icon, size: 14, color: color),
                  SizedBox(width: 4),
                  Text(text, style: TextStyle(fontSize: 12, color: color)),
                ],
              );
            }(),
            title: Row(
              spacing: 8,
              children: [
                Flexible(
                  child: Tooltip(
                    message: "The key number",
                    child: Text(
                      "#${share.index}",
                      style: TextStyle(
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                        fontWeight: FontWeight.w500,
                        fontSize: 18,
                      ),
                    ),
                  ),
                ),
                Flexible(
                  child: Text(
                    deviceName,
                    style: monospaceTextStyle.copyWith(fontSize: 18),
                  ),
                ),
              ],
            ),
          ),
        );
      }).toList(),
    );

    final sizeClass = WindowSizeContext.of(context);
    final alignTop =
        sizeClass == WindowSizeClass.compact ||
        sizeClass == WindowSizeClass.medium ||
        sizeClass == WindowSizeClass.expanded;
    return CustomScrollView(
      slivers: [
        appBar,
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16.0),
            child: Align(
              alignment: AlignmentDirectional.center,
              child: ConstrainedBox(
                constraints: BoxConstraints(
                  maxWidth: alignTop ? double.infinity : 600,
                ),
                child: ConstrainedBox(
                  constraints: BoxConstraints(maxWidth: 600),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Padding(
                        padding: const EdgeInsets.all(4.0),
                        child: Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text: '${shareCount.got}',
                                style: TextStyle(
                                  fontWeight: FontWeight.bold,
                                  decoration: TextDecoration.underline,
                                ),
                              ),
                              TextSpan(text: ' out of '),
                              TextSpan(
                                text: '${shareCount.needed ?? "??"}',
                                style: TextStyle(
                                  fontWeight: FontWeight.bold,
                                  decoration: TextDecoration.underline,
                                ),
                              ),
                              TextSpan(text: ' keys needed in restoration:'),
                            ],
                          ),
                        ),
                      ),
                      const SizedBox(height: 8),
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
                              restorationId: restorationState.restorationId,
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

/// Encapsulates a device and a future that completes when it disconnects
class TargetDevice {
  final ConnectedDevice device;
  final Future<void> onDisconnected;

  TargetDevice({required this.device, required this.onDisconnected});

  DeviceId get id => device.id;
  String? get name => device.name;
  bool needsFirmwareUpgrade() => device.needsFirmwareUpgrade();
}

class _RecoveryFlowPrevState {
  RecoveryFlowStep currentStep = RecoveryFlowStep.start;
  RecoverShare? candidate;
  TargetDevice? targetDevice;
  RestorationId? restorationId;

  _RecoveryFlowPrevState({
    required this.currentStep,
    required this.candidate,
    required this.targetDevice,
    required this.restorationId,
  });
}

class _WalletRecoveryFlowState extends State<WalletRecoveryFlow> {
  late final MethodChoiceKind kind;

  RecoveryFlowStep currentStep = RecoveryFlowStep.start;
  RecoverShare? candidate;
  TargetDevice? targetDevice;
  Completer<void>? _disconnectionCompleter;
  RestorationId? restorationId;
  String? walletName;
  String? deviceName;
  BitcoinNetwork? bitcoinNetwork;
  int? threshold;
  String? error;
  String? errorTitle;
  bool isException = false;
  StreamSubscription<DeviceListUpdate>? _deviceListSubscription;
  bool isPhysical = false;

  // For back gesture.
  final prevStates = List<_RecoveryFlowPrevState>.empty(growable: true);
  bool isAnimationForward = true;
  void pushPrevState() {
    isAnimationForward = true;
    prevStates.add(
      _RecoveryFlowPrevState(
        currentStep: currentStep,
        candidate: candidate,
        targetDevice: targetDevice,
        restorationId: restorationId,
      ),
    );
  }

  bool tryPopPrevState(BuildContext context) {
    if (prevStates.isNotEmpty) {
      setState(() {
        isAnimationForward = false;
        final prevState = prevStates.removeLast();
        currentStep = prevState.currentStep;
        // Clear any errors when going back
        error = null;
        errorTitle = null;
      });
      return true;
    }
    return false;
  }

  // 🚨 Pop state on error, keeping device connection intact
  void popOnError({
    String? errorMessage,
    String? errorTitle,
    bool isException = false,
  }) {
    setState(() {
      if (prevStates.isNotEmpty) {
        isAnimationForward = false;
        final prevState = prevStates.removeLast();
        currentStep = prevState.currentStep;
      }
      // Set error after popping so it shows on the previous screen
      this.error = errorMessage;
      this.errorTitle = errorTitle;
      this.isException = isException;
    });
  }

  @override
  void dispose() {
    _deviceListSubscription?.cancel();
    super.dispose();
  }

  void _setTargetDevice(ConnectedDevice device) {
    // Cancel any existing subscription
    _deviceListSubscription?.cancel();
    // Complete any existing completer if not already completed
    if (_disconnectionCompleter != null &&
        !_disconnectionCompleter!.isCompleted) {
      _disconnectionCompleter!.complete();
    }

    // Create new disconnection completer
    _disconnectionCompleter = Completer<void>();

    targetDevice = TargetDevice(
      device: device,
      onDisconnected: _disconnectionCompleter!.future,
    );

    // Start monitoring for disconnection
    _deviceListSubscription = GlobalStreams.deviceListSubject.listen((update) {
      // Check if our target device is still connected
      final stillConnected = update.state.devices.any(
        (d) => deviceIdEquals(d.id, device.id),
      );

      if (!stillConnected && targetDevice != null && mounted) {
        // Complete the disconnection future - let children handle cleanup
        if (!_disconnectionCompleter!.isCompleted) {
          _disconnectionCompleter!.complete();
        }
        _deviceListSubscription?.cancel();
        _deviceListSubscription = null;
      }
    });
  }

  void _clearTargetDevice() {
    targetDevice = null;
    candidate = null;
    // Complete if not already completed
    if (_disconnectionCompleter != null &&
        !_disconnectionCompleter!.isCompleted) {
      _disconnectionCompleter!.complete();
    }
    _disconnectionCompleter = null;
    _deviceListSubscription?.cancel();
    _deviceListSubscription = null;
  }

  Future<void> _completeDeviceShareEnrollment() async {
    try {
      RestorationId? restorationId;
      final encryptionKey = await SecureKeyProvider.getEncryptionKey();

      // Compatibility was already checked in onCandidateDetected,
      // so we can proceed directly with the restoration
      if (widget.continuing != null) {
        await coord.continueRestoringWalletFromDeviceShare(
          restorationId: widget.continuing!,
          recoverShare: candidate!,
          encryptionKey: encryptionKey,
        );
      } else if (widget.existing != null) {
        await coord.recoverShare(
          accessStructureRef: widget.existing!,
          recoverShare: candidate!,
          encryptionKey: encryptionKey,
        );
      } else {
        restorationId = await coord.startRestoringWalletFromDeviceShare(
          recoverShare: candidate!,
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

    if (widget.initialStep != null) {
      currentStep = widget.initialStep!;
      // Set isPhysical based on which flow we're starting with
      if (widget.initialStep == RecoveryFlowStep.enterRestorationDetails) {
        isPhysical = true;
      }
    }

    if (widget.continuing != null) {
      kind = MethodChoiceKind.continueRecovery;
      restorationId = widget.continuing!;
      final state = coord.getRestorationState(restorationId: restorationId!)!;
      threshold = state.accessStructure.effectiveThreshold();
      walletName = state.keyName.toString();
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
    // Normal flow
    _TitledWidget child;
    switch (currentStep) {
      case RecoveryFlowStep.waitDevice:
        child = _PlugInPromptView(
          continuing: widget.continuing,
          existing: widget.existing,
          onCandidateDetected: (detectedShare) async {
            if (mounted) {
              final encryptionKey = await SecureKeyProvider.getEncryptionKey();

              // Check compatibility based on the flow type
              if (widget.continuing != null) {
                // Case 2: Continuing a restoration
                final error = await coord
                    .checkContinueRestoringWalletFromDeviceShare(
                      restorationId: widget.continuing!,
                      recoverShare: detectedShare,
                      encryptionKey: encryptionKey,
                    );
                if (error != null) {
                  final deviceName =
                      coord.getDeviceName(id: detectedShare.heldBy) ??
                      '<empty>';
                  setState(() {
                    this.error = error.toString();
                    this.errorTitle = 'Cannot add key from $deviceName';
                  });
                  return;
                }
              } else if (widget.existing != null) {
                // Case 3: Adding to existing wallet
                final error = await coord.checkRecoverShare(
                  accessStructureRef: widget.existing!,
                  recoverShare: detectedShare,
                  encryptionKey: encryptionKey,
                );
                if (error != null) {
                  final deviceName =
                      coord.getDeviceName(id: detectedShare.heldBy) ??
                      '<empty>';
                  setState(() {
                    this.error = error.toString();
                    this.errorTitle = 'Cannot add key from $deviceName';
                  });
                  return;
                }
              } else {
                // Case 1: Starting new restoration
                final error = await coord.checkStartRestoringKeyFromDeviceShare(
                  recoverShare: detectedShare,
                  encryptionKey: encryptionKey,
                );
                if (error != null) {
                  setState(() {
                    this.error = error.toString();
                    this.errorTitle = 'Cannot start restoration';
                  });
                  return;
                }
              }

              // Get the device from the device list and set up targetDevice
              final deviceList = await GlobalStreams.deviceListSubject.first;
              final device = deviceList.state.getDevice(
                id: detectedShare.heldBy,
              );

              if (device != null) {
                _setTargetDevice(device);
              }

              setState(() {
                pushPrevState();
                candidate = detectedShare;
                currentStep = RecoveryFlowStep.candidateReady;
              });
            }
          },
        );
        break;
      case RecoveryFlowStep.candidateReady:
        child = _CandidateReadyView(
          candidate: candidate!,
          continuing: widget.continuing,
          existing: widget.existing,
          onConfirm: () {
            setState(() {
              pushPrevState();
              currentStep = RecoveryFlowStep.generatingNonces;
            });
          },
        );
        break;
      case RecoveryFlowStep.waitPhysicalBackupDevice:
        child = _PlugInBlankView(
          error: error,
          onBlankDeviceConnected: (device) {
            final eligibility = device.firmwareUpgradeEligibility();
            setState(() {
              _setTargetDevice(device);
              eligibility.when(
                canUpgrade: () {
                  error = null;
                  currentStep = RecoveryFlowStep.firmwareUpgrade;
                },
                upToDate: () {
                  error = null;
                  currentStep = RecoveryFlowStep.enterDeviceName;
                },
                cannotUpgrade: (reason) {
                  error = 'Incompatible firmware: $reason';
                },
              );
            });
          },
        );
        break;

      case RecoveryFlowStep.firmwareUpgrade:
        child = _FirmwareUpgradeView(
          key: ValueKey("firmware-upgrade"),
          targetDevice: targetDevice!,
          onComplete: () {
            setState(() {
              // After firmware upgrade, device reboots - need to wait for reconnection
              _clearTargetDevice();
              currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
            });
          },
          onCancel: () {
            setState(() {
              _clearTargetDevice();
              currentStep = RecoveryFlowStep.waitPhysicalBackupDevice;
            });
          },
          onDisconnected: () {
            _clearTargetDevice();
            popOnError(
              errorTitle: 'Device Disconnected',
              errorMessage:
                  'The device was disconnected. Please reconnect and try again.',
            );
          },
        );
        break;

      case RecoveryFlowStep.enterDeviceName:
        child = _EnterDeviceNameView(
          targetDevice: targetDevice!,
          name: deviceName,
          onDisconnected: () {
            deviceName = null;
            _clearTargetDevice(); // Device actually disconnected
            popOnError(
              errorTitle: 'Device Disconnected',
              errorMessage:
                  'The device was disconnected. Please reconnect and try again.',
            );
          },
          onDeviceName: (name) {
            // Store the device name in state
            setState(() {
              deviceName = name;
            });

            final nonceRequest = coord.createNonceRequest(
              devices: [targetDevice!.id],
            );

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
        // Create nonce stream - always use targetDevice which is set for both flows
        final nonceRequest = coord.createNonceRequest(
          devices: [targetDevice!.id],
        );
        final stream = coord
            .replenishNonces(
              nonceRequest: nonceRequest,
              devices: [targetDevice!.id],
            )
            .toBehaviorSubject();

        child = NonceGenerationPage(
          stream: stream,
          deviceName: coord.getDeviceName(id: targetDevice!.id),
          onDisconnected: targetDevice!.onDisconnected,
          onComplete: () async {
            if (isPhysical) {
              // Physical backup: continue to enter backup
              setState(() {
                currentStep = RecoveryFlowStep.enterBackup;
              });
            } else {
              // Device share: complete enrollment and exit
              await _completeDeviceShareEnrollment();
            }
          },
          onCancel: () {
            coord.cancelProtocol();
            popOnError(); // Just go back without error message
          },
          onDeviceDisconnected: () {
            _clearTargetDevice();
            popOnError(
              errorTitle: 'Device Disconnected',
              errorMessage: 'The device was disconnected during preparation.',
            );
          },
          onError: (error) {
            popOnError(errorMessage: error);
          },
        );
        break;

      case RecoveryFlowStep.enterBackup:
        final stream = coord.tellDeviceToEnterPhysicalBackup(
          deviceId: targetDevice!.id,
        );
        child = _EnterBackupView(
          stream: stream,
          deviceId: targetDevice!.id,
          deviceName: coord.getDeviceName(id: targetDevice!.id),
          onCancel: () {
            popOnError(); // User cancelled backup entry
          },
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
                // Successfully added to existing wallet, close dialog
                if (mounted) {
                  Navigator.pop(context);
                }
              } else {
                restorationId ??= await coord.startRestoringWallet(
                  name: walletName!,
                  threshold: threshold,
                  network: bitcoinNetwork!,
                );

                // Check if this physical backup can be added to the restoration
                final encryptionKey =
                    await SecureKeyProvider.getEncryptionKey();
                final error = await coord.checkPhysicalBackupForRestoration(
                  restorationId: restorationId!,
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

                // Save the backup
                await coord.tellDeviceToSavePhysicalBackup(
                  phase: backupPhase,
                  restorationId: restorationId!,
                );
                setState(() {
                  currentStep = RecoveryFlowStep.physicalBackupSuccess;
                });
              }
            } catch (e, stackTrace) {
              // Error during backup save - pop back with error
              popOnError(
                errorTitle: 'Failed to save backup',
                errorMessage: '$e\n\nStack trace:\n$stackTrace',
                isException: true,
              );
            }
          },
          onError: (e) {
            // Device disconnected or other error - pop back with error
            popOnError(errorMessage: e);
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
          deviceName: coord.getDeviceName(id: targetDevice!.id)!,
          onClose: () {
            // Return the restorationId so it can be selected in the wallet list
            Navigator.pop(context, restorationId);
          },
        );
        break;
      // physicalBackupFail case removed - we always allow saving
      default:
        child = _ChooseMethodView(
          kind: kind,
          onDeviceChosen: () {
            setState(() {
              isPhysical = false;
              pushPrevState();
              currentStep = RecoveryFlowStep.waitDevice;
            });
          },
          onPhysicalBackupChosen: () {
            setState(() {
              isPhysical = true;
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

    // Override with error view if error is present
    if (error != null && errorTitle != null) {
      child = _ErrorView(
        title: errorTitle!,
        message: error!,
        isWarning:
            !isException, // Exceptions are severe, validation errors are warnings
        onRetry: () {
          setState(() {
            error = null;
            errorTitle = null;
            isException = false;
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
  // Shared steps (both flows)
  start,
  generatingNonces,

  // Device share flow unique
  waitDevice,
  candidateReady,

  // Physical backup flow unique
  waitPhysicalBackupDevice,
  firmwareUpgrade,
  enterDeviceName,
  enterBackup,
  enterRestorationDetails,
  enterThreshold,
  physicalBackupSuccess,
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
  final String? error;

  const _PlugInBlankView({this.onBlankDeviceConnected, this.error});

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
    );
  }

  void _onCancel() async {
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
    if (widget.error != null) {
      children = [
        MaterialDialogCard(
          iconData: Icons.warning_rounded,
          backgroundColor: theme.colorScheme.errorContainer,
          textColor: theme.colorScheme.onErrorContainer,
          iconColor: theme.colorScheme.onErrorContainer,
          variantTextColor: theme.colorScheme.onErrorContainer,
          title: Text('Incompatible Firmware'),
          content: Text(widget.error!),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: Text('Close'),
              style: TextButton.styleFrom(
                foregroundColor: theme.colorScheme.onErrorContainer,
              ),
            ),
          ],
        ),
      ];
    } else if (_connectedDevice != null && _connectedDevice!.name != null) {
      var name = _connectedDevice!.name!;
      children = [
        MaterialDialogCard(
          iconData: Icons.warning_rounded,
          title: Text('Device not blank'),
          content: Text(
            'This device already has data on it. To load a physical backup, it must be erased. Erasing will permanently delete all keys on "${name}".',
          ),
          actions: [
            FilledButton.icon(
              style: FilledButton.styleFrom(
                backgroundColor: theme.colorScheme.error,
                foregroundColor: theme.colorScheme.onError,
              ),
              icon: Icon(Icons.delete),
              label: Text('Erase "$name"'),
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
      widget.onWalletNameEntered(
        _walletNameController.text.trim(),
        bitcoinNetwork,
      );
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
            maxLength: keyNameMaxLength(),
            inputFormatters: [nameInputFormatter],
            autofocus: true,
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
  final Function(int? threshold) onThresholdEntered;
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
  String get titleText => 'Wallet Threshold (Optional)';
}

class _EnterThresholdViewState extends State<_EnterThresholdView> {
  final _formKey = GlobalKey<FormState>();
  final _thresholdController = TextEditingController();
  final _thresholdFocusNode = FocusNode();
  int? _threshold;
  bool _specifyThreshold = false;

  @override
  void initState() {
    super.initState();
    final initialThreshold = widget.initialThreshold;
    if (initialThreshold != null) {
      _threshold = initialThreshold;
      _specifyThreshold = true;
      _thresholdController.text = initialThreshold.toString();
    }
  }

  @override
  void dispose() {
    _thresholdController.dispose();
    _thresholdFocusNode.dispose();
    super.dispose();
  }

  void _handleSubmit() {
    if (_specifyThreshold && _formKey.currentState!.validate()) {
      widget.onThresholdEntered(_threshold);
    } else if (!_specifyThreshold) {
      widget.onThresholdEntered(null);
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return Focus(
      autofocus: true,
      onKeyEvent: (node, event) {
        if (event is KeyDownEvent &&
            event.logicalKey == LogicalKeyboardKey.enter) {
          _handleSubmit();
          return KeyEventResult.handled;
        }
        return KeyEventResult.ignored;
      },
      child: Form(
        key: _formKey,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'If you know the threshold of the wallet, enter it here. Otherwise, we will determine it as we go.',
              style: theme.textTheme.bodyMedium,
            ),
            const SizedBox(height: 24),
            RadioGroup<bool>(
              groupValue: _specifyThreshold,
              onChanged: (value) {
                setState(() {
                  _specifyThreshold = value ?? false;
                  if (!_specifyThreshold) {
                    _threshold = null;
                  }
                });
              },
              child: Column(
                children: [
                  Card.outlined(
                    child: InkWell(
                      onTap: () {
                        setState(() {
                          _specifyThreshold = false;
                          _threshold = null;
                        });
                      },
                      borderRadius: BorderRadius.circular(12),
                      child: Padding(
                        padding: const EdgeInsets.all(16.0),
                        child: Row(
                          children: [
                            Radio<bool>(value: false),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                "I'm not sure",
                                style: theme.textTheme.bodyLarge,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  const SizedBox(height: 12),
                  Card.outlined(
                    child: InkWell(
                      onTap: () {
                        setState(() {
                          _specifyThreshold = true;
                        });
                        WidgetsBinding.instance.addPostFrameCallback((_) {
                          _thresholdFocusNode.requestFocus();
                        });
                      },
                      borderRadius: BorderRadius.circular(12),
                      child: Padding(
                        padding: const EdgeInsets.all(16.0),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Row(
                              children: [
                                Radio<bool>(value: true),
                                const SizedBox(width: 8),
                                Expanded(
                                  child: Text(
                                    "I know the threshold",
                                    style: theme.textTheme.bodyLarge,
                                  ),
                                ),
                              ],
                            ),
                            const SizedBox(height: 16),
                            TextFormField(
                              controller: _thresholdController,
                              focusNode: _thresholdFocusNode,
                              enabled: _specifyThreshold,
                              keyboardType: TextInputType.number,
                              inputFormatters: [
                                FilteringTextInputFormatter.digitsOnly,
                              ],
                              decoration: const InputDecoration(
                                labelText: 'Threshold',
                                border: OutlineInputBorder(),
                                hintText: 'Number of keys needed',
                              ),
                              validator: (value) {
                                if (!_specifyThreshold) return null;
                                if (value == null || value.isEmpty) {
                                  return 'Please enter a threshold';
                                }
                                final threshold = int.tryParse(value);
                                if (threshold == null || threshold < 1) {
                                  return 'Threshold must be at least 1';
                                }
                                return null;
                              },
                              onChanged: (value) {
                                setState(() {
                                  _threshold = int.tryParse(value);
                                });
                              },
                              onFieldSubmitted: (_) => _handleSubmit(),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 24),
            Align(
              alignment: AlignmentDirectional.centerEnd,
              child: FilledButton(
                child: const Text('Continue'),
                onPressed: _handleSubmit,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _EnterDeviceNameView extends StatefulWidget with _TitledWidget {
  final Function(String)? onDeviceName;
  final VoidCallback? onDisconnected;
  final TargetDevice targetDevice;
  final String? name;
  const _EnterDeviceNameView({
    required this.targetDevice,
    this.name,
    this.onDeviceName,
    this.onDisconnected,
  });

  @override
  State<_EnterDeviceNameView> createState() => _EnterDeviceNameViewState();

  @override
  String get titleText => 'Device name';
}

class _EnterDeviceNameViewState extends State<_EnterDeviceNameView> {
  @override
  void initState() {
    super.initState();
    // Listen for device disconnection
    widget.targetDevice.onDisconnected.then((_) {
      if (mounted) {
        widget.onDisconnected?.call();
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          "If in doubt you can use the name written on the backup or make up a new one.",
          style: theme.textTheme.bodyMedium,
        ),
        const SizedBox(height: 16),
        DeviceNameField(
          id: widget.targetDevice.id,
          mode: DeviceNameMode.preview,
          buttonText: 'Continue',
          initialValue: widget.name,
          onNamed: (name) {
            widget.onDeviceName?.call(name);
          },
        ),
      ],
    );
  }
}

class _EnterBackupView extends StatefulWidget with _TitledWidget {
  final Stream<EnterPhysicalBackupState> stream;
  final Function(PhysicalBackupPhase)? onFinished;
  final Function(String)? onError;
  final VoidCallback? onCancel;
  final DeviceId deviceId;
  final String? deviceName;

  const _EnterBackupView({
    required this.stream,
    required this.deviceId,
    this.deviceName,
    this.onFinished,
    this.onError,
    this.onCancel,
  });

  @override
  State<_EnterBackupView> createState() => _EnterBackupViewState();

  @override
  String get titleText => 'Enter backup on device';
}

class _EnterBackupViewState extends State<_EnterBackupView> {
  late final FullscreenActionDialogController<void> _backupController;
  StreamSubscription? _subscription;
  bool _dialogShown = false;

  @override
  void initState() {
    super.initState();

    _backupController = FullscreenActionDialogController(
      title: 'Enter Physical Backup',
      body: (context) {
        final theme = Theme.of(context);
        return Card.filled(
          margin: EdgeInsets.zero,
          color: theme.colorScheme.surfaceContainerHigh,
          child: ListTile(
            leading: Icon(Icons.keyboard_rounded),
            title: Text('Enter backup on ${widget.deviceName ?? "device"}'),
            subtitle: Text(
              'Enter the backup on the device screen. The app will continue automatically once complete.',
            ),
            isThreeLine: true,
            contentPadding: EdgeInsets.symmetric(horizontal: 16),
          ),
        );
      },
      actionButtons: [
        OutlinedButton(
          child: Text('Cancel'),
          onPressed: () async {
            await _backupController.clearAllActionsNeeded();
            widget.onCancel?.call();
          },
        ),
        DeviceActionHint(
          label: 'Enter on device',
          icon: Icons.keyboard_rounded,
        ),
      ],
    );

    _subscription = widget.stream.listen((state) async {
      if (state.entered != null) {
        await _subscription?.cancel();
        await _backupController.clearAllActionsNeeded();
        widget.onFinished?.call(state.entered!);
      }
      if (state.abort != null) {
        await _backupController.clearAllActionsNeeded();
        widget.onError?.call(state.abort!);
      }
    });

    // Show the fullscreen dialog immediately
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!_dialogShown && mounted) {
        _dialogShown = true;
        _backupController.addActionNeeded(context, widget.deviceId);
      }
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _backupController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // Return a loading indicator while the fullscreen dialog is being shown
    return Center(child: CircularProgressIndicator());
  }
}

class _PlugInPromptView extends StatefulWidget with _TitledWidget {
  final RestorationId? continuing;
  final AccessStructureRef? existing;
  final void Function(RecoverShare candidate) onCandidateDetected;

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

  @override
  void initState() {
    super.initState();

    _subscription = coord.waitForRecoveryShare().listen((
      waitForRecoverShareState,
    ) async {
      blankDeviceInserted = false;

      if (waitForRecoverShareState.shares.isNotEmpty) {
        final detectedShare = waitForRecoverShareState.shares.first;
        setState(() {
          widget.onCandidateDetected(detectedShare);
        });
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
    if (blankDeviceInserted) {
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

class _CandidateReadyView extends StatelessWidget with _TitledWidget {
  final RecoverShare candidate;
  final RestorationId? continuing;
  final AccessStructureRef? existing;
  final VoidCallback onConfirm;

  const _CandidateReadyView({
    required this.candidate,
    this.continuing,
    this.existing,
    required this.onConfirm,
  });

  @override
  String get titleText => 'Restore with existing key';

  @override
  Widget build(BuildContext context) {
    final deviceName = coord.getDeviceName(id: candidate.heldBy) ?? '<empty>';

    String title;
    String message;
    String buttonText;

    // Always allow adding the key - compatibility will be shown in the restoration list
    title = 'Key ready';

    if (continuing != null || existing != null) {
      message =
          'Key \'$deviceName\' is ready to be added to wallet \'${candidate.heldShare.keyName}\'.';
      buttonText = 'Add to wallet';
    } else {
      message =
          'Key \'$deviceName\' is part of a wallet called \'${candidate.heldShare.keyName}\'.';
      buttonText = 'Start restoring';
    }

    return MaterialDialogCard(
      key: const ValueKey('candidateReady'),
      iconData: Icons.check_circle,
      title: Text(title),
      content: Text(message, textAlign: TextAlign.center),
      actions: [FilledButton(child: Text(buttonText), onPressed: onConfirm)],
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

class _FirmwareUpgradeView extends StatefulWidget with _TitledWidget {
  final TargetDevice targetDevice;
  final VoidCallback onComplete;
  final VoidCallback onCancel;
  final VoidCallback onDisconnected;

  const _FirmwareUpgradeView({
    super.key,
    required this.targetDevice,
    required this.onComplete,
    required this.onCancel,
    required this.onDisconnected,
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

    // Listen for device disconnection
    // Note: During firmware upgrade, the device will reset which is expected.
    // We only handle unexpected disconnections before the upgrade starts.
    widget.targetDevice.onDisconnected.then((_) {
      if (mounted && !_isUpgrading) {
        widget.onDisconnected();
      }
    });
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

// Error view with retry option
class _ErrorView extends StatefulWidget with _TitledWidget {
  final String title;
  final String message;
  final VoidCallback? onRetry;
  final bool isWarning;

  const _ErrorView({
    required this.title,
    required this.message,
    this.onRetry,
    this.isWarning = false,
  });

  @override
  State<_ErrorView> createState() => _ErrorViewState();

  @override
  String get titleText => isWarning ? 'Warning' : 'Error';
}

class _ErrorViewState extends State<_ErrorView> {
  final ScrollController _scrollController = ScrollController();

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    // Wrap in SingleChildScrollView with NeverScrollableScrollPhysics to consume
    // the scroll from Dialog.fullscreen but prevent actual scrolling
    return SingleChildScrollView(
      physics: NeverScrollableScrollPhysics(),
      child: MaterialDialogCard(
        iconData: widget.isWarning
            ? Icons.warning_amber_rounded
            : Icons.error_outline_rounded,
        title: Text(widget.title),
        content: Container(
          constraints: BoxConstraints(maxHeight: 200),
          child: Scrollbar(
            controller: _scrollController,
            thumbVisibility: true,
            child: SingleChildScrollView(
              controller: _scrollController,
              child: SelectableText(
                widget.message,
                textAlign: TextAlign.center,
              ),
            ),
          ),
        ),
        backgroundColor: widget.isWarning
            ? theme.colorScheme.surfaceContainerHigh
            : theme.colorScheme.errorContainer,
        textColor: widget.isWarning
            ? theme.colorScheme.onSurface
            : theme.colorScheme.onErrorContainer,
        iconColor: widget.isWarning
            ? theme.colorScheme.onSurfaceVariant
            : theme.colorScheme.onErrorContainer,
        actions: [
          // Only show copy button for exceptions (they have stack traces worth copying)
          if (!widget.isWarning)
            OutlinedButton.icon(
              icon: Icon(Icons.copy),
              label: Text('Copy Error'),
              onPressed: () {
                Clipboard.setData(ClipboardData(text: widget.message));
              },
              style: OutlinedButton.styleFrom(
                foregroundColor: theme.colorScheme.onErrorContainer,
                side: BorderSide(color: theme.colorScheme.onErrorContainer),
              ),
            ),
          if (widget.onRetry != null)
            FilledButton.icon(
              icon: Icon(Icons.refresh),
              label: Text('Try Again'),
              onPressed: widget.onRetry,
              style: widget.isWarning
                  ? null // Use default button style for warnings
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

// Unified nonce dialog for both enrollment and physical backup flows
class NonceGenerationPage extends StatefulWidget with _TitledWidget {
  final Stream<NonceReplenishState> stream;
  final String? deviceName;
  final Future<void> onDisconnected;
  final VoidCallback onComplete;
  final VoidCallback onCancel;
  final VoidCallback onDeviceDisconnected;
  final Function(String) onError;

  const NonceGenerationPage({
    required this.stream,
    this.deviceName,
    required this.onDisconnected,
    required this.onComplete,
    required this.onCancel,
    required this.onDeviceDisconnected,
    required this.onError,
  });

  @override
  State<NonceGenerationPage> createState() => _NonceGenerationPageState();

  @override
  String get titleText => 'Preparing Device';
}

class _NonceGenerationPageState extends State<NonceGenerationPage> {
  bool _hasCompleted = false;
  bool _hasErrored = false;
  StreamSubscription? _streamSubscription;

  @override
  void initState() {
    super.initState();
    // Listen for device disconnection
    widget.onDisconnected.then((_) {
      if (mounted && !_hasCompleted) {
        widget.onDeviceDisconnected();
      }
    });
  }

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
              widget.onError('Failed to prepare device: ${snapshot.error}');
            }
          });
        }

        final state = snapshot.data;

        // Handle completion
        if (state != null && !_hasCompleted && !_hasErrored) {
          final isComplete = state.isFinished();
          if (isComplete) {
            _hasCompleted = true;
            // Add a delay to show the completion state before transitioning
            Future.delayed(Durations.long1, () {
              if (mounted) {
                widget.onComplete();
              }
            });
          }
        }

        // Handle abort
        if (state?.abort == true && !_hasCompleted && !_hasErrored) {
          _hasErrored = true;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (mounted) {
              widget.onError('Device disconnected during preparation');
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
    return;
  }
  homeCtx.walletListController.selectRecoveringWallet(restorationId);
}
