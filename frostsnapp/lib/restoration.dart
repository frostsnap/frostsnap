import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnapp/device_setup.dart';
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
  ConnectedDevice? blankDevice;
  RestorationId? manuallyEntered;
  String? walletName;
  int? threshold;
  String? error;

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
      case 'wait_physical_backup_device':
        child = _PlugInBlankView(
          onBlankDeviceConnected: (device) {
            setState(() {
              blankDevice = device;
              currentStep = 'enter_device_name';
            });
          },
        );
        break;

      case 'enter_device_name':
        child = _EnterDeviceNameView(
          deviceId: blankDevice!.id,
          onDeviceName: (name) {
            setState(() => currentStep = 'enter_backup');
          },
        );
      case 'enter_backup':
        child = _EnterBackupView(
          deviceId: blankDevice!.id,
          restorationId: widget.continuing ?? manuallyEntered!,
          onBackupSaved: (backupPhase) async {
            try {
              setState(() {
                currentStep = "physical_backup_success";
              });
            } catch (e) {
              setState(() {
                currentStep = "physical_backup_fail";
                error = e.toString();
              });
            }
          },
          onError: (e) {
            setState(() {
              currentStep = "physical_backup_fail";
              error = e.toString();
            });
          },
        );
        break;
      case 'enter_restoration_details':
        child = _EnterWalletNameView(
          onWalletNameEntered: (walletName) {
            setState(() {
              this.walletName = walletName;
              currentStep = 'enter_threshold';
            });
          },
        );
        break;

      case 'enter_threshold':
        child = _EnterThresholdView(
          walletName: walletName!,
          onThresholdEntered: (threshold, restorationId) {
            setState(() {
              this.threshold = threshold;
              manuallyEntered = restorationId;
              currentStep = 'wait_physical_backup_device';
            });
          },
        );
        break;
      case 'physical_backup_success':
        child = _PhysicalBackupSuccessView(
          deviceName: coord.getDeviceName(id: blankDevice!.id)!,
          onClose: () {
            Navigator.pop(context);
          },
        );
        break;
      case 'physical_backup_fail':
        child = _PhysicalBackupFailView(
          errorMessage: error!,
          onRetry: () {
            setState(() {
              currentStep = 'enter_backup';
              error = null;
            });
          },
          onClose: () {
            Navigator.pop(context);
          },
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
          onPhysicalBackupChosen: () {
            setState(() {
              if (widget.continuing == null) {
                currentStep = "enter_restoration_details";
              } else {
                currentStep = 'wait_physical_backup_device';
              }
            });
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
  final VoidCallback? onPhysicalBackupChosen;
  final MethodChoiceKind kind;

  const _ChooseMethodView({
    required this.kind,
    this.onDeviceChosen,
    this.onPhysicalBackupChosen,
  });

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
              'I have a key backup recorded on physical bit of paper or metal and a blank device I want to load it on to',
            ),
            onTap: onPhysicalBackupChosen,
          ),
        ),
      ],
    );
  }
}

class _PlugInBlankView extends StatefulWidget {
  final Function(ConnectedDevice)? onBlankDeviceConnected;

  const _PlugInBlankView({Key? key, this.onBlankDeviceConnected})
    : super(key: key);

  @override
  State<_PlugInBlankView> createState() => _PlugInBlankViewState();
}

class _PlugInBlankViewState extends State<_PlugInBlankView> {
  StreamSubscription? _subscription;
  ConnectedDevice? connectedDevice;

  @override
  void initState() {
    super.initState();
    _subscription = GlobalStreams.deviceListSubject.listen((update) {
      final candidate = update.state.devices.firstWhereOrNull(
        (device) => device.name == null,
      );
      if (candidate != null) {
        widget.onBlankDeviceConnected?.call(candidate);
        setState(() {
          connectedDevice = candidate;
        });
      } else {
        setState(() {
          connectedDevice = update.state.devices.firstOrNull;
        });
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
    final List<Widget> children;
    final theme = Theme.of(context);
    if (connectedDevice != null && connectedDevice!.name != null) {
      children = [
        Icon(Icons.cancel, color: theme.colorScheme.error, size: 48),
        SizedBox(height: 16),
        Text(
          "The device “${connectedDevice!.name!}” you've plugged in is not blank.\nYou must erase it first to use it.",
          textAlign: TextAlign.center,
          style: theme.textTheme.titleMedium,
        ),
        SizedBox(height: 16),
      ];
    } else {
      children = [
        Icon(Icons.usb, size: 48),
        SizedBox(height: 16),
        Text(
          'Plug in a blank Frostsnap device to restore the physical backup onto',
          textAlign: TextAlign.center,
          style: theme.textTheme.titleMedium,
        ),
        SizedBox(height: 16),
        CircularProgressIndicator(),
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

class _EnterWalletNameView extends StatefulWidget {
  final Function(String walletName) onWalletNameEntered;

  const _EnterWalletNameView({Key? key, required this.onWalletNameEntered})
    : super(key: key);

  @override
  State<_EnterWalletNameView> createState() => _EnterWalletNameViewState();
}

class _EnterWalletNameViewState extends State<_EnterWalletNameView> {
  final _formKey = GlobalKey<FormState>();
  final _walletNameController = TextEditingController();
  bool _isButtonEnabled = false;

  @override
  void initState() {
    super.initState();
    _walletNameController.addListener(_updateButtonState);
  }

  void _updateButtonState() {
    setState(() {
      _isButtonEnabled = _walletNameController.text.isNotEmpty;
    });
  }

  void _submitForm() {
    if (_isButtonEnabled && _formKey.currentState!.validate()) {
      widget.onWalletNameEntered(_walletNameController.text);
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

    return Form(
      key: _formKey,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            "Enter Wallet Name",
            style: theme.textTheme.titleLarge,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 16),
          Text(
            "Enter the same name that is recorded on the physical backup. If it's been lost or damaged you may enter another name.",
            style: theme.textTheme.bodyMedium,
            textAlign: TextAlign.center,
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
          const SizedBox(height: 24),
          ElevatedButton.icon(
            icon: const Icon(Icons.arrow_forward),
            label: const Text('Continue'),
            onPressed: _isButtonEnabled ? _submitForm : null,
          ),
        ],
      ),
    );
  }
}

class _EnterThresholdView extends StatefulWidget {
  final String walletName;
  final Function(int threshold, RestorationId restorationId) onThresholdEntered;

  const _EnterThresholdView({
    Key? key,
    required this.walletName,
    required this.onThresholdEntered,
  }) : super(key: key);

  @override
  State<_EnterThresholdView> createState() => _EnterThresholdViewState();
}

class _EnterThresholdViewState extends State<_EnterThresholdView> {
  final _formKey = GlobalKey<FormState>();
  int _threshold = 2; // Default value

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
            "Enter Recovery Threshold",
            style: theme.textTheme.titleLarge,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 16),
          Text(
            "Enter the threshold that's recorded on the backup. If it's been lost or damaged try and guess it!",
            style: theme.textTheme.bodyMedium,
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 24),
          DropdownButtonFormField<int>(
            value: _threshold,
            decoration: const InputDecoration(
              labelText: 'Threshold',
              border: OutlineInputBorder(),
              hintText: 'Number of keys needed',
            ),
            items:
                List.generate(5, (index) => index + 1)
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
          ElevatedButton.icon(
            icon: const Icon(Icons.arrow_forward),
            label: const Text('Begin Recovery'),
            onPressed: () async {
              if (_formKey.currentState!.validate()) {
                // Create a new restoration ID
                final restorationId = await coord.startRestoringWallet(
                  name: widget.walletName,
                  threshold: _threshold,
                  network: BitcoinNetwork.mainnet(bridge: api),
                );

                widget.onThresholdEntered(_threshold, restorationId);
              }
            },
          ),
        ],
      ),
    );
  }
}

class _EnterDeviceNameView extends StatelessWidget {
  final Function(String)? onDeviceName;
  final DeviceId deviceId;
  const _EnterDeviceNameView({
    required this.deviceId,
    this.onDeviceName,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text("Name the device", style: theme.textTheme.headlineLarge),
        const SizedBox(height: 16),
        Text(
          "Give this device a name. If in doubt you can use the name written on the backup or make up a new one. You can always rename it later",
          style: theme.textTheme.bodyMedium,
        ),
        const SizedBox(height: 16),
        DeviceNameField(
          id: deviceId,
          onNamed: (name) {
            onDeviceName?.call(name);
          },
        ),
      ],
    );
  }
}

class _EnterBackupView extends StatefulWidget {
  final RestorationId restorationId;
  final DeviceId deviceId;
  final Function(PhysicalBackupPhase)? onBackupSaved;
  final Function(String)? onError;

  const _EnterBackupView({
    Key? key,
    required this.deviceId,
    required this.restorationId,
    this.onBackupSaved,
    this.onError,
  }) : super(key: key);

  @override
  State<_EnterBackupView> createState() => _EnterBackupViewState();
}

class _EnterBackupViewState extends State<_EnterBackupView> {
  StreamSubscription? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = coord
        .tellDeviceToEnterPhysicalBackup(
          restorationId: widget.restorationId,
          deviceId: widget.deviceId,
        )
        .listen((state) {
          if (state.saved == true) {
            widget.onBackupSaved?.call(state.entered!);
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
      children: const [
        Icon(Icons.keyboard, size: 48, color: Colors.grey),
        SizedBox(height: 16),
        Text(
          'Enter the backup on the device',
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 18),
        ),
        SizedBox(height: 16),
        CircularProgressIndicator(),
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
  late StreamSubscription _subscription;
  bool blankDeviceInserted = false;
  RecoverShare? alreadyGot;

  @override
  void initState() {
    super.initState();

    _subscription = coord.waitForRecoveryShare().listen((
      waitForRecoverShareState,
    ) {
      var detectedShare = waitForRecoverShareState.recoverable.firstOrNull;

      if (detectedShare != null) {
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
      } else {
        setState(() {
          alreadyGot = waitForRecoverShareState.alreadyGot.firstOrNull;
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
      displayWidget = Column(
        key: const ValueKey('warning-already-got'),
        mainAxisSize: MainAxisSize.min,
        children: [
          // Big warning icon
          Icon(
            Icons.warning_amber,
            size: 80, // Big size for emphasis
            color: theme.colorScheme.error,
          ),
          SizedBox(height: 16),
          // Warning text for the alreadyGot error case
          Text(
            "That device “${coord.getDeviceName(id: alreadyGot!.deviceId())}” is already part of the wallet “${alreadyGot!.keyName()}”",
            style: theme.textTheme.bodyLarge,
            textAlign: TextAlign.center,
          ),
        ],
      );
    } else if (blankDeviceInserted) {
      displayWidget = Column(
        key: const ValueKey('warning-blank'),
        mainAxisSize: MainAxisSize.min,
        children: [
          // Big warning icon
          Icon(Icons.warning_amber, size: 80, color: theme.colorScheme.error),
          SizedBox(height: 16),
          // Warning text for the blank device error case
          Text(
            "The device you plugged in has no key on it",
            style: theme.textTheme.bodyLarge,
            textAlign: TextAlign.center,
          ),
        ],
      );
    } else {
      // No error: show the spinner centered within the space.
      displayWidget = Center(
        child: CircularProgressIndicator(key: const ValueKey('spinner')),
      );
    }

    return Column(
      key: const ValueKey('plugInPrompt'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        Icon(Icons.usb, size: 48, color: Colors.grey),
        SizedBox(height: 16),
        Text(
          'Plug in your Frostsnap device\nto begin wallet recovery',
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 18),
        ),
        SizedBox(height: 16),
        // Fix the overall vertical space for the AnimatedSwitcher.
        // Adjust the height value as needed.
        SizedBox(
          height: 150,
          child: AnimatedSwitcher(
            duration: Duration(milliseconds: 200),
            child: displayWidget,
          ),
        ),
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
                : 'Start recovery';
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

class _PhysicalBackupSuccessView extends StatelessWidget {
  final VoidCallback onClose;
  final String deviceName;

  const _PhysicalBackupSuccessView({
    Key? key,
    required this.onClose,
    required this.deviceName,
  }) : super(key: key);

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
}

class _PhysicalBackupFailView extends StatelessWidget {
  final String errorMessage;
  final VoidCallback onRetry;
  final VoidCallback onClose;

  const _PhysicalBackupFailView({
    Key? key,
    required this.errorMessage,
    required this.onRetry,
    required this.onClose,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      key: const ValueKey('physicalBackupFail'),
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(Icons.error, size: 48, color: theme.colorScheme.error),
        const SizedBox(height: 16),
        Text(
          'Failed to restore physical backup.',
          style: theme.textTheme.headlineMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 8),
        Text(
          errorMessage,
          style: theme.textTheme.bodyMedium,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 24),
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            ElevatedButton.icon(
              icon: const Icon(Icons.refresh),
              label: const Text('Retry'),
              onPressed: onRetry,
            ),
            const SizedBox(width: 16),
            ElevatedButton.icon(
              icon: const Icon(Icons.close),
              label: const Text('Close'),
              onPressed: onClose,
            ),
          ],
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
