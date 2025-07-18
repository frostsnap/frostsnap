import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnap/animated_check.dart';
import 'package:frostsnap/backup_workflow.dart';
import 'package:frostsnap/contexts.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/device_action.dart';
import 'package:frostsnap/id_ext.dart';
import 'package:frostsnap/device_list.dart';
import 'package:frostsnap/global.dart';
import 'package:frostsnap/hex.dart';
import 'package:frostsnap/settings.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/bitcoin.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/src/rust/api/keygen.dart';
import 'package:frostsnap/stream_ext.dart';
import 'package:frostsnap/progress_indicator.dart';
import 'package:frostsnap/theme.dart';

class KeyNamePage extends StatefulWidget {
  const KeyNamePage({super.key});

  @override
  State<KeyNamePage> createState() => _KeyNamePageState();
}

class _KeyNamePageState extends State<KeyNamePage> {
  final TextEditingController _keyNameController = TextEditingController();
  final FocusNode _keyNameFocusNode = FocusNode();
  BitcoinNetwork bitcoinNetwork = BitcoinNetwork.bitcoin;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      FocusScope.of(context).requestFocus(_keyNameFocusNode);
    });
  }

  @override
  Widget build(BuildContext context) {
    final homeCtx = HomeContext.of(context)!;
    final nextPage = _keyNameController.text.isNotEmpty
        ? () async {
            final masterAppkey = await Navigator.push(
              context,
              createRoute(
                homeCtx.wrap(
                  DevicesPage(
                    keyName: _keyNameController.text,
                    network: bitcoinNetwork,
                  ),
                ),
              ),
            );
            if (context.mounted && masterAppkey != null) {
              Navigator.pop(context, masterAppkey);
            }
          }
        : null;

    final settingsCtx = SettingsContext.of(context)!;

    return Scaffold(
      appBar: FsAppBar(title: Text('Wallet Name')),
      body: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              "Choose a name for this wallet",
              style: TextStyle(fontSize: 18),
              textAlign: TextAlign.center,
            ),
            SizedBox(height: 20),
            ConstrainedBox(
              constraints: BoxConstraints(
                maxWidth: 200, // Set the maximum width for the text box
              ),
              child: TextField(
                controller: _keyNameController,
                focusNode: _keyNameFocusNode,
                textAlign: TextAlign.center,
                maxLength: 20, // Limit the number of characters
                decoration: InputDecoration(labelText: 'Wallet name'),
                onChanged: (value) {
                  setState(() {}); // Update the UI when the text changes
                },
                onSubmitted: (name) {
                  if (name.isNotEmpty) {
                    nextPage?.call();
                  }
                },
              ),
            ),
            SizedBox(height: 20),
            StreamBuilder(
              stream: settingsCtx.developerSettings,
              builder: (context, snap) {
                if (snap.data?.developerMode == true) {
                  return BitcoinNetworkChooser(
                    value: bitcoinNetwork,
                    onChanged: (BitcoinNetwork network) {
                      setState(() {
                        bitcoinNetwork = network;
                      });
                    },
                  );
                } else {
                  return SizedBox();
                }
              },
            ),
            Align(
              alignment: Alignment.center,
              child: ElevatedButton.icon(
                onPressed: nextPage,
                icon: Icon(Icons.arrow_forward),
                label: Text('Next'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class DevicesPage extends StatelessWidget {
  final String keyName;
  final BitcoinNetwork network;

  const DevicesPage({super.key, required this.keyName, required this.network});

  @override
  Widget build(BuildContext context) {
    final homeCtx = HomeContext.of(context)!;
    return Scaffold(
      appBar: FsAppBar(title: Text('Devices')),
      body: StreamBuilder(
        stream: GlobalStreams.deviceListSubject,
        builder: (context, snapshot) {
          final theme = Theme.of(context);

          if (!snapshot.hasData) {
            return FsProgressIndicator();
          }
          final devices = snapshot.data!.state.devices;
          final Widget prompt;

          final anyNeedUpgrade = devices.any(
            (device) => device.needsFirmwareUpgrade(),
          );

          final anyNeedsName = devices.any((device) => device.name == null);

          final allDevicesReady =
              !(anyNeedsName || anyNeedUpgrade || devices.isEmpty);
          final style = TextStyle(fontSize: 16);

          if (anyNeedUpgrade) {
            prompt = Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Icon(Icons.warning, color: theme.colorScheme.secondary),
                SizedBox(width: 5.0),
                Expanded(
                  child: Text(
                    "Some devices need their firmware upgraded before they can be used to generated a key",
                    style: style,
                    softWrap: true,
                    textAlign: TextAlign.center,
                  ),
                ),
              ],
            );
          } else if (anyNeedsName) {
            prompt = Text(
              "Set up each device before generating a key",
              style: style,
            );
          } else if (devices.isEmpty) {
            prompt = Text(
              "Insert the devices that will be part of ‘$keyName’",
              style: style,
              textAlign: TextAlign.center,
            );
          } else {
            prompt = Text(
              "These ${devices.length} devices will be part of ‘$keyName’",
              style: style,
              textAlign: TextAlign.center,
            );
          }

          final mediaQuery = MediaQuery.of(context);
          return Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.center,
            children: [
              Expanded(child: DeviceList()),
              Container(
                // Wrap the bottom section in a Container with BoxDecoration
                decoration: BoxDecoration(
                  color: theme.colorScheme.surfaceContainer,
                  boxShadow: const [
                    BoxShadow(
                      spreadRadius: 1,
                      blurRadius: 8,
                      offset: Offset(0, 4), // Position of the shadow
                    ),
                  ],
                ),
                child: Column(
                  children: [
                    SizedBox(height: 20),
                    prompt,
                    SizedBox(height: 20),
                    Align(
                      alignment: Alignment.center,
                      child: ElevatedButton.icon(
                        onPressed: allDevicesReady
                            ? () async {
                                final masterAppkey = await Navigator.push(
                                  context,
                                  createRoute(
                                    homeCtx.wrap(
                                      ThresholdPage(
                                        network: network,
                                        keyName: keyName,
                                        selectedDevices: devices,
                                      ),
                                    ),
                                  ),
                                );
                                if (context.mounted && masterAppkey != null) {
                                  Navigator.pop(context, masterAppkey);
                                }
                              }
                            : null,
                        icon: Icon(Icons.arrow_forward),
                        label: Text('Next'),
                      ),
                    ),
                    SizedBox(height: 20 + mediaQuery.padding.bottom),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

class ThresholdPage extends StatefulWidget {
  final String keyName;
  final BitcoinNetwork network;
  final List<ConnectedDevice> selectedDevices;

  const ThresholdPage({
    super.key,
    required this.keyName,
    required this.selectedDevices,
    required this.network,
  });

  @override
  State<ThresholdPage> createState() => _ThresholdPageState();
}

class _ThresholdPageState extends State<ThresholdPage> {
  int _selectedThreshold = 1;

  @override
  void initState() {
    super.initState();
    _selectedThreshold = (widget.selectedDevices.length + 1) ~/ 2;
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: FsAppBar(title: Text('Threshold')),
      body: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              "How many devices will need to sign to spend from this wallet?",
              style: TextStyle(fontSize: 18),
              softWrap: true,
              textAlign: TextAlign.center,
            ),
            SizedBox(height: 20),
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                DropdownButton<int>(
                  value: _selectedThreshold,
                  onChanged: (int? newValue) {
                    setState(() {
                      _selectedThreshold = newValue!;
                    });
                  },
                  items:
                      List.generate(
                        widget.selectedDevices.length,
                        (index) => index + 1,
                      ).map<DropdownMenuItem<int>>((int value) {
                        return DropdownMenuItem<int>(
                          value: value,
                          child: Container(
                            width: 70,
                            alignment: Alignment.center,
                            child: Text(value.toString()),
                          ),
                        );
                      }).toList(),
                ),
                Flexible(
                  child: Padding(
                    padding: const EdgeInsets.only(left: 8.0),
                    child: Text(
                      'of ${widget.selectedDevices.length} devices will be needed to sign',
                      softWrap: true,
                      style: TextStyle(fontSize: 18),
                      textAlign: TextAlign.center,
                    ),
                  ),
                ),
              ],
            ),
            SizedBox(height: 20),
            Align(
              alignment: Alignment.center,
              child: ElevatedButton.icon(
                onPressed: () async {
                  // Generate key
                  final stream = coord
                      .generateNewKey(
                        threshold: _selectedThreshold,
                        devices: widget.selectedDevices
                            .map((device) => device.id)
                            .toList(),
                        keyName: widget.keyName,
                        network: widget.network,
                      )
                      .toBehaviorSubject();

                  final accessStructureRef = await showCheckKeyGenDialog(
                    context: context,
                    stream: stream,
                    network: widget.network,
                  );
                  debugPrint("$accessStructureRef");

                  if (!context.mounted) return;

                  if (accessStructureRef == null) {
                    coord.cancelProtocol();
                    Navigator.popUntil(context, (route) => route.isFirst);
                    return;
                  }

                  if (context.mounted) {
                    debugPrint("here");
                    Navigator.popUntil(context, (r) => r.isFirst);
                    final homeCtx = HomeContext.of(context)!;
                    homeCtx.openNewlyCreatedWallet(accessStructureRef.keyId);
                    showWalletCreatedDialog(context, accessStructureRef);
                  }
                },
                icon: Icon(Icons.check),
                label: Text('Start'),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

showWalletCreatedDialog(
  BuildContext context,
  AccessStructureRef accessStructureRef,
) async {
  final accessStructure = coord.getAccessStructure(asRef: accessStructureRef)!;
  final backupManager = FrostsnapContext.of(context)!.backupManager;
  await showDialog(
    context: context,
    barrierDismissible: false,
    builder: (BuildContext context) {
      return BackdropFilter(
        filter: blurFilter,
        child: AlertDialog(
          title: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: const [
              Text(
                'Wallet created!\nNow let\'s secure it.',
                style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
              ),
              Icon(Icons.checklist, size: 40),
            ],
          ),
          content: const Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Before receiving any Bitcoin, you should backup and distribute your Frostsnaps.',
              ),
              SizedBox(height: 16),
              Text(
                'With each of your Frostsnaps you will:',
                style: TextStyle(fontWeight: FontWeight.bold),
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.directions_walk),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Travel to a location where you will store it.',
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.edit),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'Record the backup on the provided backup sheet (~5 mins).',
                    ),
                  ),
                ],
              ),
              SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(Icons.lock),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text('Securely store the Frostsnap and its backup.'),
                  ),
                ],
              ),
            ],
          ),
          actions: [
            TextButton(
              onPressed: () {
                Navigator.of(context).pop();
              },
              child: const Text('Later'),
            ),
            FilledButton(
              onPressed: () {
                Navigator.of(context).pop();
                showBottomSheetOrDialog(
                  context,
                  titleText: 'Backup Checklist',
                  builder: (context, scrollController) =>
                      SuperWalletContext.of(context)!.tryWrapInWalletContext(
                        keyId: accessStructureRef.keyId,
                        child: BackupChecklist(
                          accessStructure: accessStructure,
                          scrollController: scrollController,
                          showAppBar: true,
                        ),
                      ),
                );
              },
              child: const Text('Secure Wallet'),
            ),
          ],
        ),
      );
    },
  );
  await backupManager.startBackupRun(accessStructure: accessStructure);
}

// Utility function for creating the page transition
Route createRoute(Widget page) {
  return PageRouteBuilder(
    // So that we can use popUntil to find the route later on.
    settings: RouteSettings(name: page.runtimeType.toString()),
    pageBuilder: (context, animation, secondaryAnimation) => page,
    transitionsBuilder: (context, animation, secondaryAnimation, child) {
      const begin = Offset(1.0, 0.0);
      const end = Offset.zero;
      const curve = Curves.easeInOutCubicEmphasized;

      final tween = Tween(
        begin: begin,
        end: end,
      ).chain(CurveTween(curve: curve));

      return SlideTransition(position: animation.drive(tween), child: child);
    },
  );
}

Future<AccessStructureRef?> showCheckKeyGenDialog({
  required Stream<KeyGenState> stream,
  required BuildContext context,
  required BitcoinNetwork network,
}) async {
  final accessStructureRef = await showDeviceActionDialog<AccessStructureRef>(
    context: context,
    complete: stream
        .firstWhere((state) => state.aborted != null)
        .then((state) => state.finished),
    builder: (context) {
      return StreamBuilder(
        stream: stream,
        builder: (context, snap) {
          if (!snap.hasData) {
            return FsProgressIndicator();
          }
          final state = snap.data!;
          final devices = deviceIdSet(state.devices);
          final acks = deviceIdSet(state.sessionAcks);
          final gotShares = deviceIdSet(state.gotShares);

          final deviceList = DeviceListWithIcons(
            key: const Key("dialog-device-list"),
            iconAssigner: (context, id) {
              if (devices.contains(id)) {
                final Widget icon;
                if (!state.allShares) {
                  if (gotShares.contains(id)) {
                    icon = AnimatedCheckCircle();
                  } else {
                    icon = FsProgressIndicator();
                  }
                } else {
                  if (acks.contains(id)) {
                    icon = AnimatedCheckCircle();
                  } else {
                    icon = ConfirmPrompt();
                  }
                }

                return (null, icon);
              } else {
                return (null, null);
              }
            },
          );

          final waitingText = Text("Waiting for devices to generate key");
          final checkPrompt = Column(
            children: [
              Text("Confirm all devices show:"),
              SizedBox(height: 10),
              Text(
                state.sessionHash == null
                    ? ""
                    : toSpacedHex(
                        Uint8List.fromList(
                          state.sessionHash!.field0.sublist(0, 4),
                        ),
                      ),
                style: TextStyle(
                  fontFamily: monospaceTextStyle.fontFamily,
                  fontWeight: FontWeight.bold,
                  fontSize: 25,
                ),
              ),
              Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text('If they do not then '),
                  ElevatedButton(
                    onPressed: () {
                      coord.cancelProtocol();
                    },
                    style: ElevatedButton.styleFrom(
                      tapTargetSize: MaterialTapTargetSize
                          .shrinkWrap, // Reduce button tap target size
                      backgroundColor: Theme.of(context).colorScheme.error,
                    ),
                    child: Text(
                      'cancel',
                      style: TextStyle(fontWeight: FontWeight.bold),
                    ),
                  ),
                  Text("."),
                ],
              ),
              Text(
                "Otherwise your securiy is at risk",
                style: TextStyle(decoration: TextDecoration.underline),
              ),
            ],
          );

          return Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              DialogHeader(
                child: Stack(
                  alignment: Alignment.center,
                  children: [
                    Visibility.maintain(
                      visible: state.sessionHash == null,
                      child: waitingText,
                    ),
                    Visibility.maintain(
                      visible: state.sessionHash != null,
                      child: checkPrompt,
                    ),
                  ],
                ),
              ),
              Expanded(child: deviceList),
              DialogFooter(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    Text(
                      '${acks.length}/${devices.length} confirmed',
                      style: TextStyle(fontSize: 14),
                    ),
                    SizedBox(height: 10),
                    FilledButton(
                      onPressed: state.allAcks
                          ? () async {
                              final accessStructureRef = await coord
                                  .finalizeKeygen(keygenId: state.keygenId);
                              if (context.mounted) {
                                Navigator.pop(context, accessStructureRef);
                              }
                            }
                          : null,
                      child: Text("Confirm"),
                    ),
                    SizedBox(height: MediaQuery.of(context).padding.bottom),
                  ],
                ),
              ),
            ],
          );
        },
      );
    },
  );

  if (accessStructureRef == null) {
    coord.cancelProtocol();
  }
  return accessStructureRef;
}
