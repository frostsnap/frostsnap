import 'dart:async';

import 'package:flutter/material.dart';
import 'package:frostsnapp/global.dart';
import 'package:collection/collection.dart';
import 'package:frostsnapp/snackbar.dart';
import 'ffi.dart';

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
    final sharesNeeded =
        restoringKey.threshold - restoringKey.sharesObtained.length;
    final canFinish = sharesNeeded <= 0;
    final progressDescription =
        sharesNeeded > 0
            ? (sharesNeeded == 1
                ? '1 more key needed'
                : '$sharesNeeded more keys needed')
            : 'You\'ve got enough keys to restore the wallet but you can continue adding more';

    return CustomScrollView(
      slivers: [
        SliverAppBar(
          pinned: true,
          title: Text(restoringKey.name),
          // Optionally add more configuration like flexibleSpace, actions, etc.
        ),
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.all(20.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Wallet Restoration',
                  style: Theme.of(context).textTheme.headlineMedium,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 20),
                Text(
                  'Keys restored:',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 10),
                // Wrap the list in a Column; using ListView here would require shrinkWrap/physics adjustments.
                Column(
                  children:
                      restoringKey.sharesObtained.map((deviceId) {
                        final deviceName =
                            coord.getDeviceName(id: deviceId) ?? '<empty>';
                        return Card(
                          elevation: 2,
                          margin: const EdgeInsets.symmetric(vertical: 4.0),
                          child: ListTile(
                            leading: const Icon(Icons.devices),
                            title: Text(deviceName),
                          ),
                        );
                      }).toList(),
                ),
                const SizedBox(height: 10),
                Center(
                  child: Text(
                    progressDescription,
                    style: Theme.of(context).textTheme.bodyLarge,
                  ),
                ),
                const SizedBox(height: 10),
                Center(
                  child: ElevatedButton.icon(
                    icon: const Icon(Icons.add),
                    label: const Text('restore another key'),
                    style: ElevatedButton.styleFrom(
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(20),
                      ),
                    ),
                    onPressed: () {
                      continueWalletRecoveryFlowDialog(
                        context,
                        restorationId: restoringKey.restorationId,
                      );
                    },
                  ),
                ),
                const SizedBox(height: 20),
                Row(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    TextButton.icon(
                      icon: const Icon(Icons.cancel),
                      label: const Text('Cancel'),
                      onPressed: () {
                        coord.cancelRestoration(
                          restorationId: restoringKey.restorationId,
                        );
                      },
                    ),
                    const SizedBox(width: 10),
                    FilledButton.icon(
                      icon: const Icon(Icons.check_circle),
                      label: const Text('Finish'),
                      onPressed:
                          canFinish
                              ? () async {
                                try {
                                  final accessStructureRef = await coord
                                      .finishRestoring(
                                        restorationId:
                                            restoringKey.restorationId,
                                      );
                                  onWalletRecovered(accessStructureRef);
                                } catch (e) {
                                  showErrorSnackbarBottom(
                                    context,
                                    "failed to recover wallet: $e",
                                  );
                                }
                              }
                              : null,
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

Future<RestorationId?> startWalletRecoveryFlowDialog(
  BuildContext context,
) async {
  final restorationId = await showDialog(
    context: context,
    builder: (context) => const WalletRecoveryFlow(),
  );
  coord.cancelProtocol();
  return restorationId;
}

class WalletRecoveryFlow extends StatefulWidget {
  // We're continuing a restoration session
  final RestorationId? continuing;
  // We're recovering a share for a key that already exists
  final AccessStructureRef? existing;
  const WalletRecoveryFlow({super.key, this.continuing, this.existing});

  @override
  _WalletRecoveryFlowState createState() => _WalletRecoveryFlowState();
}

class _WalletRecoveryFlowState extends State<WalletRecoveryFlow> {
  String currentStep = 'start';
  RecoverShare? candidate;
  ShareCompatibility? compatibility;

  @override
  Widget build(BuildContext context) {
    Widget child;

    switch (currentStep) {
      case 'wait_device':
        child = _PlugInPromptView(
          continuing: widget.continuing,
          existing: widget.existing,
          onCandidateDetected: (detectedShare, compatibility) {
            if (mounted) {
              setState(() {
                candidate = detectedShare;
                this.compatibility = compatibility;
                currentStep = 'candidate_ready';
              });
            }
          },
        );
        break;
      case 'candidate_ready':
        child = _CandidateReadyView(
          candidate: candidate!,
          compatibility: compatibility!,
          continuing: widget.continuing,
          existing: widget.existing,
        );
        break;
      default:
        final MethodChoiceKind kind;
        if (widget.continuing != null) {
          kind = MethodChoiceKind.ContinueRecovery;
        } else if (widget.existing != null) {
          kind = MethodChoiceKind.AddToWallet;
        } else {
          kind = MethodChoiceKind.StartRecovery;
        }

        child = _ChooseMethodView(
          kind: kind,
          onDeviceChosen: () {
            setState(() => currentStep = 'wait_device');
          },
        );
    }

    return Dialog(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      child: ConstrainedBox(
        constraints: const BoxConstraints(
          minWidth: 500, // Choose a suitable fixed width
          maxWidth: 500,
          minHeight:
              320, // Ensure this is large enough for your tallest content
        ),
        child: AnimatedSwitcher(
          duration: const Duration(milliseconds: 400),
          child: Padding(
            key: ValueKey(currentStep),
            padding: const EdgeInsets.all(20.0),
            child: child,
          ),
        ),
      ),
    );
  }
}

enum MethodChoiceKind { StartRecovery, ContinueRecovery, AddToWallet }

class _ChooseMethodView extends StatelessWidget {
  final VoidCallback? onDeviceChosen;
  final MethodChoiceKind kind;

  const _ChooseMethodView({required this.kind, this.onDeviceChosen});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final String title;
    final String subtitle;

    switch (kind) {
      case MethodChoiceKind.StartRecovery:
        title = "Start wallet recovery";
        subtitle =
            'To start, what kind of key are you starting the wallet recovery from?';
        break;
      case MethodChoiceKind.ContinueRecovery:
        title = 'Continue wallet recovery';
        subtitle = 'Where is the next key coming from?';
        break;

      case MethodChoiceKind.AddToWallet:
        title = "Add key to wallet";
        subtitle =
            "⚠ For now, Frostsnap only supports adding keys that were originally part of the wallet when it was created";
        break;
    }

    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          title,
          style: theme.textTheme.titleLarge,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 10),
        Text(
          subtitle,
          style: TextStyle(fontSize: 14),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 20),
        Card(
          elevation: 4,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
          ),
          clipBehavior: Clip.hardEdge,
          child: ListTile(
            leading: const Icon(Icons.devices, size: 30),
            title: const Text('An existing wallet key'),
            subtitle: const Text(
              "I have a Frostsnap device with a key for the wallet",
            ),
            onTap: onDeviceChosen,
          ),
        ),
        const SizedBox(height: 15),
        Card(
          elevation: 4,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
          ),
          clipBehavior: Clip.hardEdge,
          child: ListTile(
            leading: const Icon(Icons.description, size: 30),
            title: const Text('A physical backup'),
            subtitle: const Text(
              'I have a key backup recorded on physical bit of paper or metal',
            ),
            onTap: () {
              // TODO: Implement manual entry
            },
          ),
        ),
      ],
    );
  }
}

class _PlugInPromptView extends StatefulWidget {
  final RestorationId? continuing;
  final AccessStructureRef? existing;
  final void Function(RecoverShare candidate, ShareCompatibility compatibility)
  onCandidateDetected;

  const _PlugInPromptView({
    Key? key,
    this.continuing,
    this.existing,
    required this.onCandidateDetected,
  }) : super(key: key);

  @override
  State<_PlugInPromptView> createState() => _PlugInPromptViewState();
}

class _PlugInPromptViewState extends State<_PlugInPromptView> {
  StreamSubscription? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = coord.waitForRecoveryShare().listen((
      waitForRecoverShareState,
    ) {
      final connectedDevices = waitForRecoverShareState.connected;
      var detectedShare = waitForRecoverShareState.candidates.firstOrNull;

      // If nothing is detected, we might have already recovered from a device.
      if (connectedDevices.isNotEmpty &&
          widget.continuing != null &&
          detectedShare == null) {
        final restoringState =
            coord.getRestorationState(restorationId: widget.continuing!)!;
        detectedShare = connectedDevices
            .map(
              (connected) =>
                  restoringState.getAlreadyRecoveredShare(deviceId: connected),
            )
            .firstWhereOrNull((recoverShare) => recoverShare != null);
      }

      if (detectedShare != null && mounted) {
        final ShareCompatibility compatibility;

        if (widget.continuing != null) {
          compatibility = coord.restorationCheckShareCompatible(
            restorationId: widget.continuing!,
            recoverShare: detectedShare,
          );
        } else if (widget.existing != null) {
          compatibility = coord.checkRecoverShareCompatible(
            recoverShare: detectedShare,
          );
        } else {
          compatibility = ShareCompatibility.Compatible;
        }

        widget.onCandidateDetected(detectedShare, compatibility);
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
      key: const ValueKey('plugInPrompt'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: const [
        Icon(Icons.usb, size: 48, color: Colors.grey),
        SizedBox(height: 16),
        Text(
          'Plug in your Frostsnap device\nto begin wallet recovery',
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 18),
        ),
        SizedBox(height: 16),
        CircularProgressIndicator(),
      ],
    );
  }
}

class _CandidateReadyView extends StatelessWidget {
  final RecoverShare candidate;
  final ShareCompatibility compatibility;
  final RestorationId? continuing;
  final AccessStructureRef? existing;

  const _CandidateReadyView({
    required this.candidate,
    required this.compatibility,
    this.continuing,
    this.existing,
  });

  @override
  Widget build(BuildContext context) {
    final deviceName =
        coord.getDeviceName(id: candidate.deviceId()) ?? '<empty>';

    Widget icon;
    String message;
    String buttonText;
    VoidCallback buttonAction;

    switch (compatibility) {
      case ShareCompatibility.Compatible:
        icon = const Icon(Icons.check_circle, size: 48, color: Colors.green);
        message =
            'Found key "$deviceName" for wallet “${candidate.keyName()}”!';
        buttonText =
            continuing != null || existing != null
                ? 'Add key to ${candidate.keyName()}'
                : 'Begin recovering ${candidate.keyName()}';
        buttonAction = () async {
          if (continuing != null) {
            await coord.continueRestoringWalletFromDeviceShare(
              restorationId: continuing!,
              recoverShare: candidate,
            );
            if (context.mounted) {
              Navigator.pop(context);
            }
          } else if (existing != null) {
            await coord.recoverShare(recoverShare: candidate);
            if (context.mounted) {
              Navigator.pop(context);
            }
          } else {
            final restorationId = await coord
                .startRestoringWalletFromDeviceShare(recoverShare: candidate);

            if (context.mounted) {
              Navigator.pop(context, restorationId);
            }
          }
        };
        break;

      case ShareCompatibility.AlreadyGotIt:
        icon = const Icon(Icons.info, size: 48, color: Colors.blue);
        message = 'You’ve already recovered "$deviceName".';
        buttonText = 'Close';
        buttonAction = () => Navigator.pop(context);
        break;

      case ShareCompatibility.Incompatible:
        icon = const Icon(Icons.error, size: 48, color: Colors.red);
        message =
            'This key "$deviceName" is part of a different wallet called "${candidate.keyName()}"';
        buttonText = 'Close';
        buttonAction = () => Navigator.pop(context);
        break;

      case ShareCompatibility.NameMismatch:
        icon = const Icon(Icons.warning, size: 48, color: Colors.orange);
        message = 'Key name mismatch for "$deviceName".';
        buttonText = 'Close';
        buttonAction = () => Navigator.pop(context);
        break;
    }

    return Column(
      key: const ValueKey('candidateReady'),
      mainAxisSize: MainAxisSize.min,
      children: [
        icon,
        const SizedBox(height: 16),
        Text(
          message,
          style: Theme.of(context).textTheme.headlineMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 16),
        ElevatedButton.icon(
          icon: const Icon(Icons.arrow_forward),
          label: Text(buttonText),
          onPressed: buttonAction,
        ),
      ],
    );
  }
}

continueWalletRecoveryFlowDialog(
  BuildContext context, {
  required RestorationId restorationId,
}) async {
  await showDialog(
    context: context,
    builder: (context) => WalletRecoveryFlow(continuing: restorationId),
  );
  coord.cancelProtocol();
}
