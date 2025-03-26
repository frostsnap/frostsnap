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
                  'Keys obtained:',
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
                    label: const Text('Add another key'),
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
  final RestorationId? continuing;
  const WalletRecoveryFlow({super.key, this.continuing});

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
        child = _PlugInPromptView();
        coord.waitForRecoveryShare().listen((waitForRecoverShareState) {
          final connectedDevices = waitForRecoverShareState.connected;
          var detectedShare = waitForRecoverShareState.candidates.firstOrNull;

          // if we've got nothing detected we might have a device that we've
          // alredy recovered from. If so we get the "detectedShare" from our
          // existing state.
          if (connectedDevices.isNotEmpty &&
              widget.continuing != null &&
              detectedShare == null) {
            final restoringState =
                coord.getRestorationState(restorationId: widget.continuing!)!;
            detectedShare = connectedDevices
                .map((connected) {
                  return restoringState.getAlreadyRecoveredShare(
                    deviceId: connected,
                  );
                })
                .firstWhereOrNull((recoverShare) => recoverShare != null);
          }

          if (detectedShare != null && mounted) {
            setState(() {
              candidate = detectedShare;
              if (widget.continuing == null) {
                compatibility = ShareCompatibility.Compatible;
              } else {
                compatibility = coord.checkShareCompatible(
                  restorationId: widget.continuing!,
                  recoverShare: detectedShare!,
                );
              }
              currentStep = 'candidate_ready';
            });
          }
        });
        break;
      case 'candidate_ready':
        child = _CandidateReadyView(
          candidate: candidate!,
          compatibility: compatibility!,
          continuing: widget.continuing,
        );
        break;
      default:
        child = _ChooseMethodView(
          continuing: widget.continuing,
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

class _ChooseMethodView extends StatelessWidget {
  final VoidCallback onDeviceChosen;
  final RestorationId? continuing;

  const _ChooseMethodView({required this.onDeviceChosen, this.continuing});

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          continuing != null
              ? 'Continue wallet recovery'
              : 'Start wallet recovery',
          style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 10),
        Text(
          continuing != null
              ? 'Where is the next key coming from?'
              : 'To start, what kind of key are you starting the wallet recovery from?',
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

class _PlugInPromptView extends StatelessWidget {
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

  const _CandidateReadyView({
    required this.candidate,
    required this.compatibility,
    this.continuing,
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
            continuing != null
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
