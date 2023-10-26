import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';
import 'package:frostsnapp/coordinator_keygen.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/main.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

typedef RemovedDeviceBuilder = Widget Function(BuildContext context,
    DeviceId id, String? label, Animation<double> animation);

class DeviceListWidget extends StatefulWidget {
  final Orientation orientation;
  const DeviceListWidget({required this.orientation, super.key});

  @override
  State<StatefulWidget> createState() => DeviceListWidgetState();
}

class DeviceListWidgetState extends State<DeviceListWidget>
    with WidgetsBindingObserver {
  final GlobalKey<AnimatedListState> deviceListKey =
      GlobalKey<AnimatedListState>();

  List<BuildContext>? contextToPopOnSuccess;

  @override
  void initState() {
    super.initState();

    WidgetsBinding.instance.addObserver(this);

    globalDeviceList.subscribe().forEach((event) async {
      switch (event.kind) {
        case DeviceListChangeKind.added:
          {
            deviceListKey.currentState!.insertItem(event.index,
                duration: const Duration(milliseconds: 800));
            setState(() => {});
          }
        case DeviceListChangeKind.removed:
          {
            deviceListKey.currentState!.removeItem(event.index,
                (BuildContext context, Animation<double> animation) {
              return _buildDevice(context, event.id, event.name, animation);
            });
            setState(() => {});
          }
        case DeviceListChangeKind.named:
          {
            if (contextToPopOnSuccess != null) {
              for (var ctx in contextToPopOnSuccess!) {
                Navigator.pop(ctx);
              }
            }
            setState(() => contextToPopOnSuccess = null);
          }
      }
    });
  }

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

  @override
  Widget build(BuildContext context) {
    print(globalDeviceList.length);

    return FrostsnapPage(
      children: [
        Text(
          'Frostsnap',
          style: TextStyle(
            fontSize: 36,
            fontWeight: FontWeight.bold,
            color: Colors.blue,
          ),
        ),
        Expanded(
          child: Container(
            color: Colors.white54,
            child: Center(
              child: AnimatedList(
                key: deviceListKey,
                itemBuilder: _buildItem,
                initialItemCount: globalDeviceList.length,
                scrollDirection: widget.orientation == Orientation.landscape
                    ? Axis.horizontal
                    : Axis.vertical,
              ),
            ),
          ),
        ),
        DoKeyGenButton(namedDevicesCount: globalDeviceList.lengthNamed()),
      ],
    );
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
                    builder: (deviceSetupContex) => DeviceSetup(
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
                                  contextToPopOnSuccess = [
                                    deviceSetupContex,
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
                                              contextToPopOnSuccess = null;
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
    var id = globalDeviceList[index];
    var label = globalDeviceList.getName(id);
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
