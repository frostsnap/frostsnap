import 'dart:async';
import 'dart:typed_data';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/hex.dart';
import 'package:frostsnapp/stream_ext.dart';
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
      body: Padding(
          padding: const EdgeInsets.all(16.0),
          child: StreamBuilder(
              stream: deviceListSubject,
              builder: (context, snapshot) {
                if (!snapshot.hasData) {
                  return CircularProgressIndicator();
                }
                final devices = snapshot.data!.state.devices;
                final Widget prompt;

                final anyNeedUpgrade =
                    devices.any((device) => device.needsFirmwareUpgrade());

                final anyNeedsName =
                    devices.any((device) => device.name == null);

                final allDevicesReady = !(anyNeedsName || anyNeedUpgrade);
                final style = TextStyle(fontSize: 18);

                if (anyNeedUpgrade) {
                  prompt = Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(Icons.warning, color: Colors.orange),
                        SizedBox(width: 5.0),
                        Text(
                          "Some devices need their firmware upgraded before they can be used to generated a key",
                          style: style,
                          textAlign: TextAlign.center,
                        )
                      ]);
                } else if (anyNeedsName) {
                  prompt = Text("Set up each device before generating a key");
                } else if (devices.isEmpty) {
                  prompt = Text(
                    "Insert the devices that will be part of ‘${keyName}’",
                    style: style,
                    textAlign: TextAlign.center,
                  );
                } else {
                  prompt = Text(
                    "These devices will be part of ‘${keyName}’",
                    style: style,
                    textAlign: TextAlign.center,
                  );
                }

                return Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.center,
                  children: [
                    prompt,
                    SizedBox(height: 20),
                    MaybeExpandedVertical(
                        child: DeviceListContainer(child: DeviceList())),
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
                        )),
                  ],
                );
              })),
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
                  // Handle the completion of the process here
                  final keyId = await Navigator.push(context,
                      MaterialPageRoute(builder: (context) {
                    final stream = coord
                        .generateNewKey(
                            threshold: _selectedThreshold,
                            devices: widget.selectedDevices
                                .map((device) => device.id)
                                .toList(),
                            keyName: widget.keyName)
                        .toBehaviorSubject();
                    return DoKeyGenScreen(
                      stream: stream,
                      keyName: widget.keyName,
                    );
                  }));

                  if (context.mounted) {
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

class DoKeyGenScreen extends StatefulWidget {
  final Stream<KeyGenState> stream;
  final String keyName;
  const DoKeyGenScreen(
      {super.key, required this.stream, required this.keyName});

  @override
  State<DoKeyGenScreen> createState() => _DoKeyGenScreenState();
}

class _DoKeyGenScreenState extends State<DoKeyGenScreen> {
  late Future aborted;

  @override
  void initState() {
    super.initState();
    aborted = widget.stream.firstWhere((state) => state.aborted != null);
    aborted.then((state) {
      if (mounted) {
        Navigator.pop(context);
        showErrorSnackbar(context, state.aborted!);
      }
    });

    widget.stream
        .firstWhere((state) => state.sessionHash != null)
        .then((state) async {
      final keyId = await showCheckKeyGenDialog(
          sessionHash: state.sessionHash!, stream: widget.stream);
      if (keyId != null) {
        await showBackupDialogue(keyId: keyId);
        if (mounted) {
          Navigator.pop(context, keyId);
        }
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(
          title: const Text('Key Generation'),
        ),
        body: StreamBuilder(
            stream: widget.stream,
            builder: (context, snap) {
              if (!snap.hasData) {
                return CircularProgressIndicator();
              }

              final state = snap.data!;
              final gotShares = deviceIdSet(state.gotShares);
              final devices = deviceIdSet(state.devices);

              return Center(
                  child: Column(
                      mainAxisAlignment: MainAxisAlignment.start,
                      children: [
                    const Text("Waiting for devices to generate key",
                        style: TextStyle(fontSize: 20)),
                    MaybeExpandedVertical(child: DeviceListContainer(
                        child: DeviceListWithIcons(iconAssigner: (context, id) {
                      if (devices.contains(id)) {
                        final Widget icon;
                        if (gotShares.contains(id)) {
                          icon = AnimatedCheckCircle();
                        } else {
                          // the aspect ratio stops the circular progress indicator from stretching itself
                          icon = const AspectRatio(
                              aspectRatio: 1,
                              child: CircularProgressIndicator());
                        }
                        return (null, icon);
                      }
                      return (null, null);
                    }))),
                  ]));
            }));
  }

  Future<KeyId?> showCheckKeyGenDialog({
    required U8Array32 sessionHash,
    required Stream<KeyGenState> stream,
  }) {
    return showDeviceActionDialog<KeyId>(
        context: context,
        onCancel: () {
          coord.cancelProtocol();
        },
        complete: stream
            .firstWhere(
                (state) => state.aborted != null || state.finished != null)
            .then((state) => state.finished),
        builder: (context) {
          return StreamBuilder(
              stream: stream,
              builder: (context, snap) {
                if (!snap.hasData) {
                  return CircularProgressIndicator();
                }
                final state = snap.data!;
                final devices = deviceIdSet(state.devices);
                final acks = deviceIdSet(state.sessionAcks);

                final deviceList = DeviceListContainer(
                    child: DeviceListWithIcons(
                        key: const Key("dialog-device-list"),
                        iconAssigner: (context, id) {
                          if (devices.contains(id)) {
                            final Widget icon;
                            if (acks.contains(id)) {
                              icon = AnimatedCheckCircle();
                            } else {
                              icon = const Row(
                                  mainAxisAlignment: MainAxisAlignment.center,
                                  children: [
                                    Icon(Icons.touch_app, color: Colors.orange),
                                    SizedBox(width: 4),
                                    Text("Confirm"),
                                  ]);
                            }
                            return (null, icon);
                          } else {
                            return (null, null);
                          }
                        }));

                return Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text("Confirm all devices show:"),
                      SizedBox(height: 10),
                      Text(
                        toSpacedHex(
                            Uint8List.fromList(sessionHash.sublist(0, 4))),
                        style: TextStyle(
                          fontFamily: 'Courier',
                          fontWeight: FontWeight.bold,
                          fontSize: 25,
                        ),
                      ),
                      Row(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Text('If they do not then '),
                            TextButton(
                              onPressed: () {
                                coord.cancelProtocol();
                              },
                              style: TextButton.styleFrom(
                                  tapTargetSize: MaterialTapTargetSize
                                      .shrinkWrap, // Reduce button tap target size
                                  backgroundColor: Colors.red),
                              child: Text(
                                'cancel',
                                style: TextStyle(
                                    fontWeight: FontWeight.bold,
                                    color: Colors.white),
                              ),
                            ),
                            Text("."),
                          ]),
                      Text("Otherwise your securiy is at risk",
                          style:
                              TextStyle(decoration: TextDecoration.underline)),
                      Divider(),
                      MaybeExpandedVertical(child: deviceList),
                    ]);
              });
        });
  }

  Future<void> showBackupDialogue({required KeyId keyId}) async {
    final frostKey = coord.getKey(keyId: keyId)!;
    final polynomialIdentifier = api.polynomialIdentifier(frostKey: frostKey);

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
                            style: TextStyle(
                                fontWeight: FontWeight.bold,
                                color: Colors.black),
                          ),
                          TextSpan(
                            text:
                                ' replaced with the character shown on screen.',
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
                            style: TextStyle(
                                fontWeight: FontWeight.bold,
                                color: Colors.black),
                          ),
                          TextSpan(
                            text: ']',
                          ),
                        ],
                        style: TextStyle(
                            fontFamily: 'Courier',
                            color: Colors.grey,
                            fontSize: 20), // Base style for the whole text
                      )),
                    ),
                    Center(
                      child: Text(
                        "xxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxxx\nxxxx xxxx xxx",
                        style: TextStyle(
                            fontFamily: 'Courier',
                            fontSize: 20,
                            fontWeight: FontWeight.bold),
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
}
