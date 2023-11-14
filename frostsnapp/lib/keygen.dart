import 'dart:async';
import 'dart:collection';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:frostsnapp/device_action.dart';
import 'package:frostsnapp/device_list_widget.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/hex.dart';
import 'dart:math';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

import 'package:frostsnapp/device_list.dart';

class KeyGenPage extends StatelessWidget {
  const KeyGenPage({super.key});

  @override
  Widget build(BuildContext context) {
    final deviceList =
        DeviceListContainer(child: KeyGenDeviceList(onSuccess: (keyId) {
      Navigator.pop(context, keyId);
    }));
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
    final deviceRemoved = globalDeviceList.subscribe().firstWhere((event) {
      return event.kind == DeviceListChangeKind.removed &&
          widget.devices.contains(event.id);
    }).then((_) {
      if (mounted) {
        Navigator.pop(context);
        api.cancelAll();
      }
      return null;
    });
    final keygenStream = api
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
    final successWhen = keygenStream
        .asyncMap((event) => event.whenOrNull(finishedKey: (keyId) => keyId))
        .firstWhere((element) => element != null);

    final Future<KeyId?> closeDialogWhen =
        Future.any([successWhen, deviceRemoved]);

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
    return WillPopScope(
      onWillPop: () async {
        api.cancelAll();
        return true;
      },
      child: Scaffold(
          appBar: AppBar(
            title: const Text('Key Generation'),
          ),
          body: Center(
              child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                const SizedBox(height: 20),
                const Text("Waiting for devices to generate key",
                    style: TextStyle(fontSize: 20)),
                const SizedBox(height: 20),
                Expanded(child: DeviceListContainer(
                    child: DeviceListWithIcons(iconAssigner: (context, id) {
                  if (widget.devices.contains(id)) {
                    if (gotShares.contains(id)) {
                      return const Icon(Icons.check,
                          key: ValueKey('finished'), color: Colors.green);
                    } else {
                      return const CircularProgressIndicator();
                    }
                  }
                  return null;
                }))),
              ]))),
    );
  }

  Future<KeyId?> showCheckKeyGenDialog(
      {required U8Array32 sessionHash,
      required Stream<DeviceId> ackUpdates,
      required Future<KeyId?> closeOn}) {
    final hexBox = toHexBox(Uint8List.fromList(sessionHash));
    final acks = deviceIdSet();
    final deviceList = StreamBuilder<DeviceId>(
        stream: ackUpdates,
        builder: (context, snap) {
          if (snap.hasData) {
            acks.add(snap.data!);
          }
          return DeviceListContainer(
              child: DeviceListWithIcons(
                  key: const Key("dialog-device-list"),
                  iconAssigner: (context, id) {
                    if (widget.devices.contains(id)) {
                      if (acks.contains(id)) {
                        return const Icon(Icons.check,
                            key: ValueKey('finished'), color: Colors.green);
                      } else {
                        return const Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(Icons.visibility, color: Colors.orange),
                              SizedBox(width: 4),
                              Text("Confirm on device"),
                            ]);
                      }
                    } else {
                      return null;
                    }
                  }));
        });

    return showDeviceActionDialog<KeyId>(
      context: context,
      content: Column(mainAxisSize: MainAxisSize.min, children: [
        const Text("Confirm each devices shows:"),
        const Divider(height: 10),
        Text(hexBox,
            style: const TextStyle(fontFamily: 'Courier', fontSize: 20)),
        const Divider(height: 10),
        Expanded(child: deviceList)
      ]),
      title: const Text("Confirm on each device"),
      onCancel: () {
        api.cancelAll();
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
        initialData: globalDeviceList.state,
        stream: globalDeviceList.subscribe().map((event) => event.state),
        builder: (context, snapshot) {
          if (snapshot.hasData) {
            final deviceListState = snapshot.data!;
            return DoKeyGenButton(
                deviceListState: deviceListState, onSuccess: onSuccess);
          } else if (snapshot.hasError) {
            return Text('Error: ${snapshot.error}');
          } else {
            return const Column(
              children: [
                Text("Please plug in Frostsnap Devices"),
                SizedBox(height: 20),
                CircularProgressIndicator()
              ],
            );
          }
        });

    return Column(mainAxisSize: MainAxisSize.min, children: [
      Expanded(child: DeviceListWidget(deviceBuilder: _buildDevice)),
      button,
      const SizedBox(height: 20),
    ]);
  }

  Widget _buildDevice(BuildContext context, DeviceId id, String? label,
      Orientation orientation, Animation<double> animation) {
    Widget child;
    if (label == null) {
      child = ElevatedButton(
          onPressed: () {
            api.updateNamePreview(id: id, name: "");
            Navigator.push(context,
                MaterialPageRoute(builder: (deviceSetupContex) {
              final completeWhen =
                  globalDeviceList.subscribe().firstWhere((event) {
                return event.kind == DeviceListChangeKind.named &&
                    deviceIdEquals(id, event.id);
              }).whenComplete(() => Navigator.pop(deviceSetupContex));
              return DeviceSetup(
                deviceId: id,
                popInvoked: () async {
                  // This happens when we click back button
                  await api.sendCancel(id: id);
                  return true;
                },
                onSubmitted: (value) async {
                  api.finishNaming(id: id, name: value);
                  await showDeviceActionDialog(
                      context: deviceSetupContex,
                      title: const Text("Confirm name"),
                      content: Text("Confirm name '$value' on device"),
                      complete: completeWhen,
                      onCancel: () async {
                        await api.sendCancel(id: id);
                      });
                },
                onChanged: (value) async {
                  await api.updateNamePreview(id: id, name: value);
                },
              );
            }));
          },
          child: const Text("NEW DEVICE"));
    } else {
      child = LabeledDeviceText(label);
    }

    return DeviceBoxContainer(
        orientation: orientation, animation: animation, child: child);
  }
}
