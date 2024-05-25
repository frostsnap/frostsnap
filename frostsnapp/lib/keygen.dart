import 'dart:async';
import 'dart:typed_data';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/hex.dart';
import 'package:frostsnapp/stream_ext.dart';
import 'dart:math';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

class KeyGenPage extends StatelessWidget {
  const KeyGenPage({super.key});

  @override
  Widget build(BuildContext context) {
    final deviceList = KeyGenDeviceList(onSuccess: (keyId) {
      Navigator.pop(context, keyId);
    });
    return Scaffold(
        appBar: AppBar(title: const Text("Choose threshold")),
        body: Center(child: deviceList));
  }
}

class DoKeyGenButton extends StatefulWidget {
  final DeviceListState deviceListState;
  final OnSuccess? onSuccess;

  const DoKeyGenButton(
      {super.key, required this.deviceListState, this.onSuccess});

  @override
  _DoKeyGenButtonState createState() => _DoKeyGenButtonState();
}

class _DoKeyGenButtonState extends State<DoKeyGenButton> {
  int? thresholdSlider;

  @override
  Widget build(BuildContext context) {
    final readyDevices =
        widget.deviceListState.devices.where((device) => device.ready());
    final anyNeedUpgrade = widget.deviceListState.devices
        .any((device) => device.needsFirmwareUpgrade());
    final int selectedThreshold =
        thresholdSlider ?? (readyDevices.length / 2 + 1).toInt();

    String prompt =
        'Threshold: ${selectedThreshold.toInt()}-of-${readyDevices.length}';

    if (anyNeedUpgrade) {
      prompt = "Upgrade firmware of all devices first";
    } else if (widget.deviceListState.devices.isEmpty) {
      prompt = "Plug in devices to generate a key";
    } else if (readyDevices.isEmpty) {
      prompt = "Set up devices first in order to generate a key";
    }

    return Column(children: [
      Text(
        prompt,
        style: const TextStyle(fontSize: 18.0),
        textAlign: TextAlign.center,
      ),
      SizedBox(
        width: MediaQuery.of(context).size.width * 0.5,
        child: Slider(
            // Force 1 <= threshold <= devicecount
            value: selectedThreshold.toDouble(),
            onChanged: readyDevices.length <= 1
                ? null
                : (newValue) {
                    setState(() {
                      thresholdSlider = newValue.round();
                    });
                  },
            divisions: max(readyDevices.length - 1, 1),
            min: 1,
            max: max(readyDevices.length.toDouble(), 1)),
      ),
      ElevatedButton(
          onPressed: readyDevices.isEmpty
              ? null
              : () async {
                  final keyId = await Navigator.push(context,
                      MaterialPageRoute(builder: (context) {
                    final stream = coord
                        .generateNewKey(
                            threshold: selectedThreshold,
                            devices: readyDevices.map((e) => e.id).toList())
                        .toBehaviorSubject();
                    return DoKeyGenScreen(
                      stream: stream,
                    );
                  }));
                  if (keyId != null) {
                    widget.onSuccess?.call(keyId);
                  }
                },
          child: const Text('Generate Key',
              style: TextStyle(
                color: Colors.white,
                fontSize: 16.0,
              )))
    ]);
  }
}

typedef OnSuccess = Function(KeyId);

class DoKeyGenScreen extends StatefulWidget {
  final Stream<KeyGenState> stream;
  const DoKeyGenScreen({super.key, required this.stream});

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
      }
      if (mounted) {
        Navigator.pop(context, keyId);
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
                    const SizedBox(height: 20),
                    const Text("Waiting for devices to generate key",
                        style: TextStyle(fontSize: 20))
                  ]));
            }));
  }

  Future<KeyId?> showCheckKeyGenDialog({
    required U8Array32 sessionHash,
    required Stream<KeyGenState> stream,
  }) {
    final hexBox = toHexBox(Uint8List.fromList(sessionHash));

    return showDeviceActionDialog(
        context: context,
        onCancel: () {
          coord.cancelProtocol();
        },
        builder: (context) {
          return Column(mainAxisSize: MainAxisSize.min, children: [
            Text("Do all the devices show:"),
            Divider(),
            hexBox,
            Row(mainAxisAlignment: MainAxisAlignment.center, children: [
              IconButton.outlined(
                onPressed: () async {
                  if (context.mounted) {
                    Navigator.pop(context);
                  }
                },
                iconSize: 35,
                icon: Icon(Icons.cancel),
                color: Colors.red,
              ),
              SizedBox(width: 50),
              IconButton.outlined(
                  onPressed: () async {
                    final keyId = await showDeviceConfirmDialog(
                        sessionHash: sessionHash, stream: stream);
                    if (context.mounted && keyId != null) {
                      Navigator.pop(context, keyId);
                    }
                  },
                  iconSize: 35,
                  icon: Icon(Icons.check),
                  color: Colors.green)
            ])
          ]);
        });

    // return showDialog(
    //     context: context,
    //     builder: (context) {
    //       aborted.then((_) {
    //         if (context.mounted) {
    //           debugPrint("ABORT");
    //           Navigator.pop(context);
    //         }
    //       });
    //       return AlertDialog(
    //           actions: [
    //             ElevatedButton(
    //               child: Text("Yes"),
    //               onPressed: ,
    //             ),
    //             SizedBox(width: 20),
    //             ElevatedButton(
    //                 child: Text("No/Cancel"),
    //                 onPressed: () {
    //                   Navigator.pop(context);
    //                   coord.cancelProtocol();
    //                 }),
    //           ],
    //           content: SizedBox(
    //               width: Platform.isAndroid ? double.maxFinite : 400.0,
    //               height: double.maxFinite,
    //               child: Align(
    //                 alignment: Alignment.center,
    //                 child: ,
    //               )));
    //     });
  }

  Future<KeyId?> showDeviceConfirmDialog({
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
                      Text("Confirm on each device"),
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
                    Text(
                        "Write down each device's backup for this key onto separate pieces of paper:"),
                    SizedBox(height: 8),
                    Divider(),
                    Center(
                      child: Text(
                        "frost[${frostKey.threshold()}]\n xxxx xxxx xxxx \n xxxx xxxx xxxx \n xxxx xxxx xxxx \n xxxx xxxx xxxx \n xxxx xxxx xxx \n",
                        style: TextStyle(fontFamily: 'Courier', fontSize: 20),
                      ),
                    ),
                    Text(
                      "Identifier: ${toSpacedHex(polynomialIdentifier)}",
                      style: TextStyle(fontFamily: 'Courier', fontSize: 18),
                    ),
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

class KeyGenDeviceList extends StatelessWidget {
  final OnSuccess? onSuccess;

  const KeyGenDeviceList({super.key, this.onSuccess});

  @override
  Widget build(BuildContext context) {
    final button = StreamBuilder(
        stream: deviceListSubject.map((update) => update.state),
        builder: (context, snapshot) {
          if (snapshot.hasData) {
            final deviceListState = snapshot.data!;
            return DoKeyGenButton(
                deviceListState: deviceListState, onSuccess: onSuccess);
          } else if (snapshot.hasError) {
            return Text('Error: ${snapshot.error}');
          } else {
            return Text('Unreachable: this is a behavior subject');
          }
        });

    return Column(children: [
      MaybeExpandedVertical(child: DeviceListContainer(child: DeviceList())),
      button,
    ]);
  }
}
