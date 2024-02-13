import 'dart:async';
import 'dart:collection';
import 'dart:typed_data';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:frostsnapp/animated_check.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_id_ext.dart';
import 'package:frostsnapp/device_list_widget.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/global.dart';
import 'package:frostsnapp/hex.dart';
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
    final nNamedDevices = widget.deviceListState.namedDevices().length;
    final nDevices = widget.deviceListState.devices.length;
    final int selectedThreshold =
        thresholdSlider ?? (nNamedDevices / 2 + 1).toInt();
    return Column(children: [
      Text(
        nNamedDevices > 0
            ? 'Threshold: ${selectedThreshold.toInt()}-of-$nNamedDevices'
            : nDevices > 0
                ? "Set up devices first in order to generate a key"
                : "Plug in devices to generate a key",
        style: const TextStyle(fontSize: 18.0),
        textAlign: TextAlign.center,
      ),
      SizedBox(
        width: MediaQuery.of(context).size.width * 0.5,
        child: Slider(
            // Force 1 <= threshold <= devicecount
            value: selectedThreshold.toDouble(),
            onChanged: nNamedDevices <= 1
                ? null
                : (newValue) {
                    setState(() {
                      thresholdSlider = newValue.round();
                    });
                  },
            divisions: max(nNamedDevices - 1, 1),
            min: 1,
            max: max(nNamedDevices.toDouble(), 1)),
      ),
      ElevatedButton(
          onPressed: nNamedDevices == 0
              ? null
              : () async {
                  final keyId = await Navigator.push(context,
                      MaterialPageRoute(builder: (context) {
                    final devices = deviceIdSet();
                    devices.addAll(widget.deviceListState.namedDevices());
                    return DoKeyGenScreen(
                      threshold: selectedThreshold,
                      devices: devices,
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
  final int threshold;
  final HashSet<DeviceId> devices;

  const DoKeyGenScreen(
      {Key? key, required this.devices, required this.threshold})
      : super(key: key);

  @override
  _DoKeyGenScreenState createState() => _DoKeyGenScreenState();
}

class _DoKeyGenScreenState extends State<DoKeyGenScreen> {
  HashSet<DeviceId> gotShares = deviceIdSet();

  @override
  void initState() {
    super.initState();
    final deviceRemoved = deviceListChangeStream.firstWhere((change) {
      return change.kind == DeviceListChangeKind.Removed &&
          widget.devices.contains(change.device.id);
    }).then((_) {
      if (mounted) {
        Navigator.pop(context);
        coord.cancelAll();
      }
      return null;
    });
    final keygenStream = coord
        .generateNewKey(
            devices: widget.devices.toList(), threshold: widget.threshold)
        .asBroadcastStream();
    final Future<U8Array32> gotAllShares = keygenStream.transform(
        StreamTransformer<CoordinatorToUserKeyGenMessage,
            U8Array32>.fromHandlers(handleData: (event, sink) {
      final sessionHash =
          event.whenOrNull(checkKeyGen: (sessionHash) => sessionHash);
      if (sessionHash != null) {
        sink.add(sessionHash);
      }
    })).first;
    final Stream<DeviceId> ackUpdates = keygenStream
        .transform(StreamTransformer.fromHandlers(handleData: (event, sink) {
      final deviceId = event.whenOrNull(keyGenAck: (id) => id);
      if (deviceId != null) {
        sink.add(deviceId);
      }
    }));
    keygenStream.forEach((event) {
      if (mounted) {
        event.whenOrNull(
            receivedShares: (id) => setState(() => gotShares.add(id)));
      }
    });
    final devicesFinishedKey = keygenStream
        .asyncMap((event) => event.whenOrNull(finishedKey: (keyId) => keyId))
        .firstWhere((element) => element != null);

    final Future<KeyId?> closeDialogWhen =
        Future.any([devicesFinishedKey, deviceRemoved]);

    gotAllShares.then((sessionHash) async {
      if (mounted) {
        final keyId = await showCheckKeyGenDialog(
            sessionHash: sessionHash,
            ackUpdates: ackUpdates,
            closeOn: closeDialogWhen);
        if (mounted) {
          Navigator.pop(context, keyId);
        }
      }
    });
  }

  @override
  void dispose() {
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
        appBar: AppBar(
          title: const Text('Key Generation'),
        ),
        body: Center(
            child:
                Column(mainAxisAlignment: MainAxisAlignment.start, children: [
          MaybeExpandedVertical(child: DeviceListContainer(
              child: DeviceListWithIcons(iconAssigner: (context, id) {
            if (widget.devices.contains(id)) {
              final Widget icon;
              if (gotShares.contains(id)) {
                icon = AnimatedCheckCircle();
              } else {
                // the aspect ratio stops the circular progress indicator from stretching itself
                icon = const AspectRatio(
                    aspectRatio: 1, child: CircularProgressIndicator());
              }
              return (null, icon);
            }
            return (null, null);
          }))),
          const SizedBox(height: 20),
          const Text("Waiting for devices to generate key",
              style: TextStyle(fontSize: 20))
        ])));
  }

  Future<KeyId?> showCheckKeyGenDialog(
      {required U8Array32 sessionHash,
      required Stream<DeviceId> ackUpdates,
      required Future<KeyId?> closeOn}) {
    final hexBox = toHexBox(Uint8List.fromList(sessionHash));

    return showDialog(
        context: context,
        builder: (context) {
          return AlertDialog(
              actions: [
                ElevatedButton(
                  child: Text("Yes"),
                  onPressed: () async {
                    final keyId = await showDeviceConfirmDialog(
                        sessionHash: sessionHash,
                        ackUpdates: ackUpdates,
                        closeOn: closeOn);
                    if (context.mounted) {
                      Navigator.pop(context, keyId);
                    }
                  },
                ),
                SizedBox(width: 20),
                ElevatedButton(
                    child: Text("No/Cancel"),
                    onPressed: () {
                      Navigator.pop(context, null);
                      coord.cancelAll();
                    }),
              ],
              content: Container(
                  width: Platform.isAndroid ? double.maxFinite : 400.0,
                  height: double.maxFinite,
                  child: Align(
                    alignment: Alignment.center,
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                      Text("Do all the devices show:"),
                      Divider(),
                      hexBox,
                    ]),
                  )));
        });
  }

  Future<KeyId?> showDeviceConfirmDialog(
      {required U8Array32 sessionHash,
      required Stream<DeviceId> ackUpdates,
      required Future<KeyId?> closeOn}) {
    final acks = deviceIdSet();
    final content = StreamBuilder<DeviceId>(
        stream: ackUpdates,
        builder: (context, snap) {
          if (snap.hasData) {
            acks.add(snap.data!);
          }

          final deviceList = DeviceListContainer(
              child: DeviceListWithIcons(
                  key: const Key("dialog-device-list"),
                  iconAssigner: (context, id) {
                    if (widget.devices.contains(id)) {
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

          return Column(mainAxisAlignment: MainAxisAlignment.center, children: [
            Text("Confirm on each device"),
            Divider(),
            MaybeExpandedVertical(child: deviceList),
          ]);
        });

    return showDeviceActionDialog<KeyId>(
      context: context,
      content: content,
      onCancel: () {
        coord.cancelAll();
        Navigator.pop(context);
      },
      complete: closeOn,
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
      MaybeExpandedVertical(
          child: DeviceListContainer(
              child: DeviceList(deviceBuilder: _buildDevice))),
      button,
    ]);
  }

  Widget _buildDevice(BuildContext context, Device device,
      Orientation orientation, Animation<double> animation) {
    Widget child;
    if (device.name == null) {
      child = ElevatedButton(
          onPressed: () {
            coord.updateNamePreview(id: device.id, name: "");
            Navigator.push(context,
                MaterialPageRoute(builder: (deviceSetupContex) {
              final completeWhen = deviceListChangeStream
                  .firstWhere((change) =>
                      change.kind == DeviceListChangeKind.Named &&
                      deviceIdEquals(device.id, change.device.id))
                  .whenComplete(() {
                if (deviceSetupContex.mounted) {
                  Navigator.pop(deviceSetupContex);
                }
              });
              return DeviceSetup(
                deviceId: device.id,
                onCancel: () {
                  coord.sendCancel(id: device.id);
                },
                onSubmitted: (value) async {
                  coord.finishNaming(id: device.id, name: value);
                  await showDeviceActionDialog(
                      context: deviceSetupContex,
                      content: Column(children: [
                        Text("Confirm name '$value' on device"),
                        Divider(),
                        MaybeExpandedVertical(child: DeviceListContainer(child:
                            DeviceListWithIcons(
                                iconAssigner: (context, deviceId) {
                          if (deviceIdEquals(deviceId, device.id)) {
                            final label = LabeledDeviceText("'$value'?");
                            const icon = const Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(Icons.visibility, color: Colors.orange),
                                  SizedBox(width: 4),
                                  Text("Confirm"),
                                ]);
                            return (label, icon);
                          } else {
                            return (null, null);
                          }
                        })))
                      ]),
                      complete: completeWhen,
                      onCancel: () async {
                        await coord.sendCancel(id: device.id);
                      });
                },
                onChanged: (value) async {
                  await coord.updateNamePreview(id: device.id, name: value);
                },
              );
            }));
          },
          child: const Text("NEW DEVICE"));
    } else {
      child = LabeledDeviceText(device.name!);
    }

    return DeviceBoxContainer(
        orientation: orientation, animation: animation, child: child);
  }
}
