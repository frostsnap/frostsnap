import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/hex.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'package:frostsnapp/theme.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class KeyNamePage extends StatefulWidget {
  @override
  _KeyNamePageState createState() => _KeyNamePageState();
}

class _KeyNamePageState extends State<KeyNamePage> {
  final TextEditingController _keyNameController = TextEditingController();
  final FocusNode _keyNameFocusNode = FocusNode();

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      FocusScope.of(context).requestFocus(_keyNameFocusNode);
    });
  }

  @override
  Widget build(BuildContext context) {
    final _nextPage = _keyNameController.text.isNotEmpty
        ? () async {
            final keyId = await Navigator.push(
              context,
              createRoute(DevicesPage(
                keyName: _keyNameController.text,
              )),
            );
            if (context.mounted && keyId != null) {
              Navigator.pop(context, keyId);
            }
          }
        : null;

    return Scaffold(
      appBar: AppBar(title: Text('Key Name')),
      body: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              "This is the human readable name the device and app will use to refer to the key. The name can never be changed.",
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
                  decoration: InputDecoration(
                    labelText: 'Key name',
                  ),
                  onChanged: (value) {
                    setState(() {}); // Update the UI when the text changes
                  },
                  onSubmitted: (name) {
                    if (name.isNotEmpty) {
                      _nextPage?.call();
                    }
                  }),
            ),
            SizedBox(height: 20),
            Align(
              alignment: Alignment.center,
              child: ElevatedButton.icon(
                onPressed: _nextPage,
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

  DevicesPage({required this.keyName});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text('Devices')),
      body: StreamBuilder(
          stream: deviceListSubject,
          builder: (context, snapshot) {
            if (!snapshot.hasData) {
              return FsProgressIndicator();
            }
            final devices = snapshot.data!.state.devices;
            final Widget prompt;

            final anyNeedUpgrade =
                devices.any((device) => device.needsFirmwareUpgrade());

            final anyNeedsName = devices.any((device) => device.name == null);

            final allDevicesReady = !(anyNeedsName || anyNeedUpgrade);
            final style = TextStyle(fontSize: 16);

            if (anyNeedUpgrade) {
              prompt =
                  Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                Icon(Icons.warning, color: awaitingColor),
                SizedBox(width: 5.0),
                Text(
                  "Some devices need their firmware upgraded before they can be used to generated a key",
                  style: style,
                  softWrap: true,
                  textAlign: TextAlign.center,
                )
              ]);
            } else if (anyNeedsName) {
              prompt = Text("Set up each device before generating a key",
                  style: style);
            } else if (devices.isEmpty) {
              prompt = Text(
                "Insert the devices that will be part of ‘${keyName}’",
                style: style,
                textAlign: TextAlign.center,
              );
            } else {
              prompt = Text(
                "These ${devices.length} devices will be part of ‘${keyName}’",
                style: style,
                textAlign: TextAlign.center,
              );
            }

            return Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.center,
              children: [
                Expanded(child: DeviceList()),
                Container(
                  // Wrap the bottom section in a Container with BoxDecoration
                  decoration: BoxDecoration(
                    color: backgroundPrimaryColor,
                    boxShadow: [
                      BoxShadow(
                        color: shadowColor,
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
                                  final keyId = await Navigator.push(
                                    context,
                                    createRoute(ThresholdPage(
                                      keyName: keyName,
                                      selectedDevices: devices,
                                    )),
                                  );
                                  if (context.mounted && keyId != null) {
                                    Navigator.pop(context, keyId);
                                  }
                                }
                              : null,
                          icon: Icon(Icons.arrow_forward),
                          label: Text('Next'),
                        ),
                      ),
                      SizedBox(height: 20),
                    ],
                  ),
                ),
              ],
            );
          }),
    );
  }
}

class ThresholdPage extends StatefulWidget {
  final String keyName;
  final List<ConnectedDevice> selectedDevices;

  ThresholdPage({required this.keyName, required this.selectedDevices});

  @override
  _ThresholdPageState createState() => _ThresholdPageState();
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
      appBar: AppBar(title: Text('Threshold')),
      body: Padding(
        padding: const EdgeInsets.all(16.0),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            Text(
              "How many devices will be needed to sign under this key?",
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
                  items: List.generate(
                          widget.selectedDevices.length, (index) => index + 1)
                      .map<DropdownMenuItem<int>>((int value) {
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
                Padding(
                  padding: const EdgeInsets.only(left: 8.0),
                  child: Text(
                    'of ${widget.selectedDevices.length} devices will be needed to sign',
                    softWrap: true,
                    style: TextStyle(fontSize: 18),
                    textAlign: TextAlign.center,
                  ),
                ),
              ],
            ),
            SizedBox(height: 20),
            Align(
              alignment: Alignment.center,
              child: ElevatedButton.icon(
                onPressed: () async {
                  final stream = coord
                      .generateNewKey(
                          threshold: _selectedThreshold,
                          devices: widget.selectedDevices
                              .map((device) => device.id)
                              .toList(),
                          keyName: widget.keyName)
                      .toBehaviorSubject();
                  final keyId = await showCheckKeyGenDialog(
                    context: context,
                    stream: stream,
                  );
                  if (keyId != null && context.mounted) {
                    await showBackupDialogue(context: context, keyId: keyId);
                  }

                  if (keyId == null && context.mounted) {
                    coord.cancelProtocol();
                    Navigator.popUntil(context, (route) {
                      return route.settings.name == "DevicesPage";
                    });
                  } else if (context.mounted) {
                    Navigator.pop(context, keyId);
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

// Utility function for creating the page transition
Route createRoute(Widget page) {
  return PageRouteBuilder(
    // So that we can use popUntil to find the route later on.
    settings: RouteSettings(name: page.runtimeType.toString()),
    pageBuilder: (context, animation, secondaryAnimation) => page,
    transitionsBuilder: (context, animation, secondaryAnimation, child) {
      const begin = Offset(1.0, 0.0);
      const end = Offset.zero;
      const curve = Curves.easeInOut;

      final tween =
          Tween(begin: begin, end: end).chain(CurveTween(curve: curve));

      return SlideTransition(
        position: animation.drive(tween),
        child: child,
      );
    },
  );
}

Future<KeyId?> showCheckKeyGenDialog({
  required Stream<KeyGenState> stream,
  required BuildContext context,
}) async {
  final result = await showDeviceActionDialog<KeyId>(
      context: context,
      complete: stream
          .firstWhere(
              (state) => state.aborted != null || state.finished != null)
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
              final gotAllShares = setEquals(gotShares, devices);

              final deviceList = DeviceListWithIcons(
                  key: const Key("dialog-device-list"),
                  iconAssigner: (context, id) {
                    if (devices.contains(id)) {
                      final Widget icon;
                      if (!gotAllShares) {
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
                  });

              final waitingText = Text("waiting for devices to generation key");
              final checkPrompt = Column(children: [
                Text("Confirm all devices show:"),
                SizedBox(height: 10),
                Text(
                  state.sessionHash == null
                      ? ""
                      : toSpacedHex(
                          Uint8List.fromList(state.sessionHash!.sublist(0, 4))),
                  style: TextStyle(
                    fontFamily: 'Courier',
                    fontWeight: FontWeight.bold,
                    fontSize: 25,
                  ),
                ),
                Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                  Text('If they do not then '),
                  TextButton(
                    onPressed: () {
                      coord.cancelProtocol();
                    },
                    style: TextButton.styleFrom(
                        tapTargetSize: MaterialTapTargetSize
                            .shrinkWrap, // Reduce button tap target size
                        backgroundColor: errorColor),
                    child: Text(
                      'cancel',
                      style: TextStyle(fontWeight: FontWeight.bold),
                    ),
                  ),
                  Text("."),
                ]),
                Text("Otherwise your securiy is at risk",
                    style: TextStyle(decoration: TextDecoration.underline)),
              ]);

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
                    )),
                    Expanded(child: deviceList)
                  ]);
            });
      });

  if (result == null) {
    coord.cancelProtocol();
  }
  return result;
}

Future<void> showBackupDialogue(
    {required KeyId keyId, required BuildContext context}) async {
  final frostKey = coord.getKey(keyId: keyId)!;
  final polynomialIdentifier = frostKey.polynomialIdentifier();

  return showDialog(
    context: context,
    builder: (context) {
      return AlertDialog(
          actions: [
            ElevatedButton(
              child: Text("I have written down my backups"),
              onPressed: () {
                coord.cancelAll();
                Navigator.pop(context);
              },
            ),
          ],
          content: SizedBox(
            width: Platform.isAndroid ? double.maxFinite : 400.0,
            child: Align(
              alignment: Alignment.center,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text.rich(TextSpan(
                      text:
                          "Write down each device's backup for this key onto separate pieces of paper. Each piece of paper should look like this with every ",
                      children: [
                        TextSpan(
                          text: 'X',
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        TextSpan(
                          text: ' replaced with the character shown on screen.',
                        )
                      ])),
                  SizedBox(height: 8),
                  Divider(),
                  Center(
                    child: Text.rich(TextSpan(
                      text: 'frost[',
                      children: <TextSpan>[
                        TextSpan(
                          text: 'X',
                          style: TextStyle(fontWeight: FontWeight.bold),
                        ),
                        TextSpan(
                          text: ']',
                        ),
                      ],
                      style: TextStyle(
                          fontFamily: 'Courier',
                          color: textSecondaryColor,
                          fontSize: 20),
                    )),
                  ),
                  Center(
                    child: Text(
                      "xxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxx",
                      style: TextStyle(
                          fontFamily: 'Courier',
                          fontSize: 20,
                          fontWeight: FontWeight.bold,
                          color: textSecondaryColor),
                    ),
                  ),
                  Center(
                      child: Text(
                    "Identifier: ${toSpacedHex(polynomialIdentifier)}",
                    style: TextStyle(fontFamily: 'Courier', fontSize: 18),
                  )),
                  Divider(),
                  SizedBox(height: 16),
                  Text(
                      "Alongside each backup, also record the identifier above."),
                  SizedBox(height: 8),
                  Text(
                      "This identifier is useful for knowing that these share backups belong to the same key and are compatibile."),
                  SizedBox(height: 24),
                  Text(
                      "Any ${frostKey.threshold()} of these backups will provide complete control over this key."),
                  SizedBox(height: 8),
                  Text(
                      "You should store these backups securely in separate locations."),
                ],
              ),
            ),
          ));
    },
  );
}
