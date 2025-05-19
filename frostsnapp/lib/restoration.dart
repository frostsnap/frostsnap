import 'dart:async';
import 'package:flutter/material.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/settings.dart';
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
    final theme = Theme.of(context);
    final progressDescription = switch (restoringKey.problem) {
      null => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.check_circle, color: theme.colorScheme.primary),
          SizedBox(width: 10),
          Flexible(
            child: Text(
              'You have enough keys to restore the wallet. You can continue recovering more keys if you have more backups available, or add them later under settings.',
              softWrap: true,
            ),
          ),
        ],
      ),
      RestorationProblem_NotEnoughShares(:final needMore) => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.pending_rounded),
          SizedBox(width: 10),
          Flexible(
            child: Text(
              needMore == 1
                  ? '1 more key needed'
                  : '$needMore more keys needed',
              softWrap: true,
            ),
          ),
        ],
      ),
      RestorationProblem_InvalidShares() => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.warning, color: theme.colorScheme.error),
          SizedBox(width: 10),
          Flexible(
            child: Text(
              "Remove incompatible shares before continuing",
              softWrap: true,
            ),
          ),
        ],
      ),
    };

    return CustomScrollView(
      slivers: [
        SliverAppBar(
          pinned: true,
          title: Text("Restoring Wallet “${restoringKey.name}”"),
          // Optionally add more configuration like flexibleSpace, actions, etc.
        ),
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.all(20.0),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const SizedBox(height: 20),
                Text(
                  "At least ${restoringKey.threshold} keys are needed to restore this wallet.",
                  style: Theme.of(context).textTheme.titleSmall,
                ),
                const SizedBox(height: 10),
                Text(
                  'Keys restored:',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 10),
                // Wrap the list in a Column; using ListView here would require shrinkWrap/physics adjustments.
                Column(
                  children:
                      restoringKey.sharesObtained.map((share) {
                        final deleteButton = IconButton(
                          icon: const Icon(Icons.delete),
                          tooltip: 'Remove share',
                          onPressed: () async {
                            await coord.deleteRestorationShare(
                              restorationId: restoringKey.restorationId,
                              deviceId: share.deviceId,
                            );
                          },
                        );
                        final deviceName =
                            coord.getDeviceName(id: share.deviceId) ??
                            '<empty>';
                        return Card(
                          elevation: 2,
                          margin: const EdgeInsets.symmetric(vertical: 4.0),
                          child: ListTile(
                            leading: Icon(Icons.key),
                            trailing: Row(
                              mainAxisSize:
                                  MainAxisSize.min, // keep row compact
                              children: [
                                ...switch (share.validity) {
                                  RestorationShareValidity.Valid => [
                                    Tooltip(
                                      message: "valid key",
                                      child: Icon(
                                        Icons.check_circle,
                                        color:
                                            Theme.of(
                                              context,
                                            ).colorScheme.primary,
                                      ),
                                    ),
                                  ],
                                  RestorationShareValidity.Unknown => [
                                    deleteButton,
                                    const SizedBox(width: 8),
                                    Tooltip(
                                      message: "validity to be determined",
                                      child: Icon(
                                        Icons.pending_rounded,
                                        color:
                                            Theme.of(
                                              context,
                                            ).colorScheme.primary,
                                      ),
                                    ),
                                  ],
                                  RestorationShareValidity.Invalid => [
                                    deleteButton,
                                    const SizedBox(width: 8),
                                    Tooltip(
                                      message:
                                          "this share is incompatible with the other shares",
                                      child: Icon(
                                        Icons.warning,
                                        color:
                                            Theme.of(context).colorScheme.error,
                                      ),
                                    ),
                                  ],
                                },
                                // delete button
                              ],
                            ),
                            title: Row(
                              children: [
                                Tooltip(
                                  message: "the key number",
                                  child: Text(
                                    "#${share.index}",
                                    style: TextStyle(
                                      color:
                                          Theme.of(context).colorScheme.primary,
                                    ),
                                  ),
                                ),
                                SizedBox(width: 10),
                                Text(deviceName),
                              ],
                            ),
                          ),
                        );
                      }).toList(),
                ),
                const SizedBox(height: 10),
                Center(child: progressDescription),
                const SizedBox(height: 10),
                Center(
                  child: ElevatedButton.icon(
                    icon: const Icon(Icons.add),
                    label: const Text('Restore another key'),
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
                      label: const Text('Restore'),
                      onPressed:
                          restoringKey.problem == null
                              ? () async {
                                try {
                                  final accessStructureRef = await coord
                                      .finishRestoring(
                                        restorationId:
                                            restoringKey.restorationId,
                                      );
                                  onWalletRecovered(accessStructureRef);
                                } catch (e) {
                                  if (context.mounted) {
                                    showErrorSnackbarBottom(
                                      context,
                                      "failed to recover wallet: $e",
                                    );
                                  }
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
  State<WalletRecoveryFlow> createState() => _WalletRecoveryFlowState();
}

class _WalletRecoveryFlowState extends State<WalletRecoveryFlow> {
  late final MethodChoiceKind kind;
  String currentStep = 'start';
  RecoverShare? candidate;
  ShareCompatibility? compatibility;
  ConnectedDevice? blankDevice;
  RestorationId? restorationId;
  String? walletName;
  BitcoinNetwork? bitcoinNetwork;
  int? threshold;
  String? error;

  @override
  void initState() {
    super.initState();
    if (widget.continuing != null) {
      kind = MethodChoiceKind.continueRecovery;
      restorationId = widget.continuing!;
      final state = coord.getRestorationState(restorationId: restorationId!)!;
      threshold = state.threshold();
      walletName = state.keyName();
      bitcoinNetwork =
          state.bitcoinNetwork() ?? BitcoinNetwork.mainnet(bridge: api);
    } else if (widget.existing != null) {
      kind = MethodChoiceKind.addToWallet;
    } else {
      kind = MethodChoiceKind.startRecovery;
    }
  }

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
        final stream = coord.tellDeviceToEnterPhysicalBackup(
          deviceId: blankDevice!.id,
        );
        child = _EnterBackupView(
          stream: stream,
          onFinished: (backupPhase) async {
            try {
              if (kind == MethodChoiceKind.addToWallet) {
                await coord.tellDeviceToConsolidatePhysicalBackup(
                  accessStructureRef: widget.existing!,
                  phase: backupPhase,
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
                    currentStep = "physical_backup_success";
                  });
                } else {
                  setState(() {
                    currentStep = "physical_backup_fail";
                  });
                }
              }
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
          onWalletNameEntered: (walletName, bitcoinNetwork) {
            setState(() {
              this.walletName = walletName;
              this.bitcoinNetwork = bitcoinNetwork;
              currentStep = 'enter_threshold';
            });
          },
        );
        break;

      case 'enter_threshold':
        child = _EnterThresholdView(
          walletName: walletName!,
          network: bitcoinNetwork!,
          onThresholdEntered: (threshold) {
            setState(() {
              this.threshold = threshold;
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
          errorMessage: error,
          compatibility: compatibility,
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
        child = _ChooseMethodView(
          kind: kind,
          onDeviceChosen: () {
            setState(() => currentStep = 'wait_device');
          },
          onPhysicalBackupChosen: () {
            setState(() {
              switch (kind) {
                case MethodChoiceKind.startRecovery:
                  currentStep = "enter_restoration_details";
                  break;
                default:
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

enum MethodChoiceKind { startRecovery, continueRecovery, addToWallet }

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
      case MethodChoiceKind.startRecovery:
        title = "Start wallet recovery";
        subtitle =
            'What kind of key are you starting the wallet recovery from?';
        break;
      case MethodChoiceKind.continueRecovery:
        title = 'Continue wallet recovery';
        subtitle = 'Where is the next key coming from?';
        break;

      case MethodChoiceKind.addToWallet:
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
          style: theme.textTheme.titleMedium,
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
            minTileHeight: 80,
            leading: const ImageIcon(
              AssetImage('assets/icons/device2.png'),
              size: 30.0,
            ),
            title: const Text('Existing key'),
            subtitle: const Text(
              "I have a Frostsnap which already has a key for the wallet.",
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
            minTileHeight: 80,
            leading: const Icon(Icons.description, size: 30),
            title: const Text('Physical backup'),
            subtitle: const Text(
              'I have a physically recorded backup and blank Frostsnap device.',
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
  ConnectedDevice? _connectedDevice;

  @override
  void initState() {
    super.initState();
    _subscription = GlobalStreams.deviceListSubject.listen((update) {
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
    if (_connectedDevice != null && _connectedDevice!.name != null) {
      var name = _connectedDevice!.name!;
      children = [
        Icon(Icons.cancel, color: theme.colorScheme.error, size: 48),
        SizedBox(height: 16),
        Text(
          "The device “$name” you've plugged in is not blank.\nYou must erase the device before loading a physical backup onto it.",
          textAlign: TextAlign.center,
          style: theme.textTheme.titleMedium,
        ),
        SizedBox(height: 16),
        ElevatedButton.icon(
          icon: Icon(Icons.delete),
          label: Text("erase “$name”"),
          onPressed: () {
            coord.wipeDeviceData(deviceId: _connectedDevice!.id);
          },
        ),
        SizedBox(height: 16),
      ];
    } else {
      children = [
        Icon(Icons.usb, size: 48),
        SizedBox(height: 16),
        Text(
          'Plug in a blank Frostsnap device to restore the physical backup onto.',
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
  final Function(String walletName, BitcoinNetwork network) onWalletNameEntered;

  const _EnterWalletNameView({Key? key, required this.onWalletNameEntered})
    : super(key: key);

  @override
  State<_EnterWalletNameView> createState() => _EnterWalletNameViewState();
}

class _EnterWalletNameViewState extends State<_EnterWalletNameView> {
  final _formKey = GlobalKey<FormState>();
  final _walletNameController = TextEditingController();
  BitcoinNetwork bitcoinNetwork = BitcoinNetwork.mainnet(bridge: api);
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
    final developerMode =
        SettingsContext.of(context)!.settings.isInDeveloperMode();

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
            "Enter the same name that is recorded on the physical backup. If it has been lost or damaged you may enter another name.",
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
  final BitcoinNetwork network;
  final Function(int threshold) onThresholdEntered;

  const _EnterThresholdView({
    Key? key,
    required this.walletName,
    required this.onThresholdEntered,
    required this.network,
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
            "Enter the threshold that's recorded on the backup.",
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
                widget.onThresholdEntered(_threshold);
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
  const _EnterDeviceNameView({required this.deviceId, this.onDeviceName});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          "Name This Device",
          style: theme.textTheme.titleLarge,
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 16),
        Text(
          "If in doubt you can use the name written on the backup or make up a new one. You can always rename it later.",
          style: theme.textTheme.bodyMedium,
          textAlign: TextAlign.center,
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
  final Stream<EnterPhysicalBackupState> stream;
  final Function(PhysicalBackupPhase)? onFinished;
  final Function(String)? onError;

  const _EnterBackupView({
    Key? key,
    required this.stream,
    this.onFinished,
    this.onError,
  }) : super(key: key);

  @override
  State<_EnterBackupView> createState() => _EnterBackupViewState();
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
            compatibility = coord.checkRecoverShareCompatible(
              accessStructureRef: widget.existing!,
              recoverShare: detectedShare,
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
            "The connected device “${coord.getDeviceName(id: alreadyGot!.deviceId())}” is already part of the wallet “${alreadyGot!.keyName()}”",
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

    final String prompt;

    if (widget.continuing != null) {
      final name =
          coord
              .getRestorationState(restorationId: widget.continuing!)!
              .keyName();
      prompt = 'Plug in a Frostsnap to continue recovering "$name"';
    } else if (widget.existing != null) {
      final name = coord.getFrostKey(keyId: widget.existing!.keyId)!.keyName();
      prompt = 'Plug in a Frostsnap to add it to "$name"';
    } else {
      prompt = 'Plug in your Frostsnap device\nto begin wallet recovery';
    }

    return Column(
      key: const ValueKey('plugInPrompt'),
      mainAxisSize: MainAxisSize.min,
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        Icon(Icons.usb, size: 48, color: Colors.grey),
        SizedBox(height: 16),
        Text(
          prompt,
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
      case ShareCompatibility_Compatible() ||
          // we ignore the problem of different wallet names on the shares for now.
          // This happens when you eneter a physical backup and enter a different
          // name for the wallet than devices you later try to add to the wallet.
          // We just carry on with the cosmetic SNAFU.
          ShareCompatibility_NameMismatch():
        icon = const Icon(Icons.check_circle, size: 48, color: Colors.green);

        if (continuing != null || existing != null) {
          message =
              'The key "$deviceName" can be added to "${candidate.keyName()}"!';
          buttonText = 'Add key to ${candidate.keyName()}';
        } else {
          message =
              'The key "$deviceName" is part of a wallet called "${candidate.keyName()}"!';
          buttonText = 'Start recovery';
        }

        buttonAction = () async {
          try {
            RestorationId? restorationId;
            if (continuing != null) {
              await coord.continueRestoringWalletFromDeviceShare(
                restorationId: continuing!,
                recoverShare: candidate,
              );
            } else if (existing != null) {
              await coord.recoverShare(
                accessStructureRef: existing!,
                recoverShare: candidate,
              );
            } else {
              restorationId = await coord.startRestoringWalletFromDeviceShare(
                recoverShare: candidate,
              );
            }

            if (context.mounted) {
              Navigator.pop(context, restorationId);
            }
          } catch (e) {
            if (context.mounted) {
              showErrorSnackbarBottom(context, "failed to recover key: $e");
            }
          }
        };

        break;

      case ShareCompatibility_AlreadyGotIt():
        icon = const Icon(Icons.info, size: 48, color: Colors.blue);
        message = 'You’ve already recovered "$deviceName".';
        buttonText = 'Close';
        buttonAction = () => Navigator.pop(context);
        break;

      case ShareCompatibility_Incompatible():
        icon = const Icon(Icons.error, size: 48, color: Colors.red);
        message =
            'This key "$deviceName" is part of a different wallet called "${candidate.keyName()}"';
        buttonText = 'Close';
        buttonAction = () => Navigator.pop(context);
        break;

      case ShareCompatibility_ConflictsWith(:final deviceId, :final index):
        icon = const Icon(Icons.error, size: 48, color: Colors.red);
        message =
            "You've already restored backup #$index on ‘${coord.getDeviceName(id: deviceId)!}’ and it doesn't match the one you just entered. Consider removing that key from the restoration first.";
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
    final theme = Theme.of(context);

    final String compatMessage = switch (compatibility) {
      ShareCompatibility_ConflictsWith(:final deviceId, :final index) =>
        "You've already restored backup #$index on ‘${coord.getDeviceName(id: deviceId)!}’ and it doesn't match the one you just entered. Are you sure that this backup is for this wallet?",
      _ => "Unknown error of kind: $compatibility",
    };

    final String message = errorMessage ?? compatMessage;
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
          message,
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
