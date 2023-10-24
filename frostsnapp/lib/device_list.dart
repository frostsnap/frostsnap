import 'dart:collection';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/do_keygen.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'dart:developer' as developer;

class DeviceListWidget extends StatefulWidget {
  final Orientation orientation;
  // final Function
  const DeviceListWidget({required this.orientation, super.key});

  @override
  State<StatefulWidget> createState() => DeviceListWidgetState();
}

class DeviceListWidgetState extends State<DeviceListWidget>
    with WidgetsBindingObserver {
  final GlobalKey<AnimatedListState> deviceListKey =
      GlobalKey<AnimatedListState>();

  late DeviceList _deviceList;

  @override
  void initState() {
    super.initState();

    WidgetsBinding.instance.addObserver(this);
    _deviceList =
        DeviceList(listKey: deviceListKey, removedDeviceBuilder: _buildDevice);
    global_coordinator.subDeviceEvents().forEach((deviceChanges) {
      for (final change in deviceChanges) {
        switch (change) {
          case DeviceChange_Added(:final id):
            {
              developer.log("Device connected");
            }
          case DeviceChange_Registered(:final id, :final name):
            {
              developer.log("Device registered");
              if (_deviceList.confirmNameDialogue != null) {
                for (var ctx in _deviceList.confirmNameDialogue!) {
                  Navigator.pop(ctx);
                }
                _deviceList.confirmNameDialogue = null;
              }
              setState(() => _deviceList.addDevice(id, name));
            }
          case DeviceChange_Disconnected(:final id):
            {
              developer.log("device disconnected");
              setState(() => _deviceList.removeDevice(id));
            }
          case DeviceChange_NeedsName(:final id):
            {
              setState(() => _deviceList.addUnamedDevice(id));
            }
          case DeviceChange_Renamed(:final id, :final newName, :final oldName):
            {
              setState(() => _deviceList.setName(id, newName));
            }
        }
      }
    });

    @override
    void dispose() {
      WidgetsBinding.instance.removeObserver(this);
      super.dispose();
    }

    // This is meant to make sure we catch any devices plugged in while the app
    // wasn't in foreground but for some reason it doesn't work.
    @override
    void didChangeAppLifecycleState(AppLifecycleState state) {
      if (state == AppLifecycleState.resumed) {
        global_coordinator.scanDevices();
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(children: [
      Text(
        'Frostsnap',
        style: TextStyle(
          fontSize: 36, // Font size
          fontWeight: FontWeight.bold, // Font weight
          color: Colors.blue, // Text color
        ),
      ),
      Expanded(
          child: AnimatedList(
              key: deviceListKey,
              itemBuilder: _buildItem,
              initialItemCount: _deviceList.length,
              scrollDirection: widget.orientation == Orientation.landscape
                  ? Axis.horizontal
                  : Axis.vertical)),
      DoKeyGenButton(devicecount: _deviceList.lengthNamed())
    ]);
  }

  Widget _buildDevice(BuildContext context, DeviceId id, String? label,
      Animation<double> animation) {
    Widget child;
    if (label == null) {
      child = ElevatedButton(
          onPressed: () {
            global_coordinator.updateNamePreview(id, "");
            Navigator.push(
                context,
                MaterialPageRoute(
                    builder: (pageContext) => DeviceSetup(
                          deviceId: id,
                          popInvoked: () async {
                            await global_coordinator.cancel(id);
                            return true;
                          },
                          onSubmitted: (value) async {
                            global_coordinator.finishNaming(id, value);
                            await showDialog<void>(
                                barrierDismissible:
                                    false, // can't dismiss the dialogue
                                context: context,
                                builder: (dialogContext) {
                                  _deviceList.confirmNameDialogue = [
                                    pageContext,
                                    dialogContext
                                  ];
                                  return AlertDialog(
                                      title: const Text("Confirm on Device"),
                                      content: Text(
                                          "Please confirm the name '$value' on the device"),
                                      actions: [
                                        ElevatedButton(
                                            onPressed: () {
                                              global_coordinator.cancel(id);
                                              Navigator.pop(dialogContext);
                                              _deviceList.confirmNameDialogue =
                                                  null;
                                            },
                                            child: const Text("cancel"))
                                      ]);
                                });
                          },
                          onChanged: (value) {
                            global_coordinator.updateNamePreview(id, value);
                          },
                        )));
          },
          child: const Text("NEW DEVICE"));
    } else {
      child = LabeledDeviceText(label);
    }

    return DeviceBoxContainer(
        orientation: widget.orientation, animation: animation, child: child);
  }

  Widget _buildItem(
      BuildContext context, int index, Animation<double> animation) {
    var id = _deviceList[index];
    var label = _deviceList.getName(id);
    return _buildDevice(context, id, label, animation);
  }
}

class DeviceBoxContainer extends StatelessWidget {
  final Animation<double> animation;
  final Widget child;
  final Orientation orientation;

  const DeviceBoxContainer(
      {required this.child,
      required this.orientation,
      required this.animation,
      super.key});

  @override
  Widget build(BuildContext context) {
    var animationBegin = orientation == Orientation.landscape
        ? const Offset(8.0, 0.0)
        : const Offset(0.0, 8.0);
    return Padding(
        padding: const EdgeInsets.all(2.0),
        child: SlideTransition(
            position: animation.drive(
                Tween(begin: animationBegin, end: const Offset(0.0, 0.0))),
            child: SizedBox(
              height: 80.0,
              width: 200.0,
              child: Card(
                color: Colors.white70,
                child: Center(
                  child: child,
                ),
              ),
            )));
  }
}

class LabeledDeviceText extends StatelessWidget {
  final String name;

  const LabeledDeviceText(this.name, {super.key});

  @override
  Widget build(BuildContext context) {
    return Text(name, style: const TextStyle(fontSize: 30));
  }
}

typedef RemovedDeviceBuilder = Widget Function(BuildContext context,
    DeviceId id, String? label, Animation<double> animation);

class DeviceList {
  DeviceList({
    required this.listKey,
    required this.removedDeviceBuilder,
  });

  final GlobalKey<AnimatedListState> listKey;
  final List<DeviceId> _items = [];
  final Map<DeviceId, String> _labels = LinkedHashMap<DeviceId, String>(
    equals: (a, b) => listEquals(a.field0, b.field0),
    hashCode: (a) => Object.hashAll(a.field0),
  );
  final RemovedDeviceBuilder removedDeviceBuilder;
  List<BuildContext>? confirmNameDialogue;

  AnimatedListState? get _animatedList => listKey.currentState;

  _append(DeviceId device) {
    if (indexOf(device) == -1) {
      _items.add(device);
      _animatedList!.insertItem(_items.length - 1,
          duration: const Duration(milliseconds: 800));
    }
  }

  String? getName(DeviceId device) {
    return _labels[device];
  }

  setName(DeviceId device, String label) {
    if (getName(device) != label) {
      _labels[device] = label;
      // final index = indexOf(device);
      // if (index != -1) {
      //   _animatedList!.removeItem(index, (context, animation) {
      //     return removedDeviceBuilder(context, device, getName(device), animation);
      //   });
      //   _animatedList!.insertItem(index, duration: const Duration(microseconds: 0));
      // }
    }
  }

  addDevice(DeviceId device, String label) {
    setName(device, label);
    _append(device);
  }

  addUnamedDevice(DeviceId device) {
    _append(device);
  }

  removeDevice(DeviceId id) {
    var index = indexOf(id);
    if (index != -1) {
      var id = _items.removeAt(index);
      _animatedList!.removeItem(index,
          (BuildContext context, Animation<double> animation) {
        return removedDeviceBuilder(context, id, getName(id), animation);
      });
    }
  }

  int lengthNamed() {
    final filteredList =
        _items.where((element) => getName(element) != null).toList();
    return filteredList.length;
  }

  int get length => _items.length;
  int indexOf(DeviceId item) => _items.indexWhere(
      (element) => listEquals(element.field0.toList(), item.field0.toList()));
  DeviceId operator [](int index) => _items[index];
}
