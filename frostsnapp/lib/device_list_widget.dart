import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:frostsnapp/device_list.dart';
import 'package:frostsnapp/serialport.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

typedef RemovedDeviceBuilder = Widget Function(
    BuildContext context, Device device, Animation<double> animation);

typedef DeviceBuilder = Widget Function(BuildContext context, Device device,
    Orientation orientation, Animation<double> animation);

class DeviceListWidget extends StatefulWidget {
  final DeviceBuilder deviceBuilder;

  const DeviceListWidget({Key? key, required this.deviceBuilder})
      : super(key: key);

  @override
  State<StatefulWidget> createState() => DeviceListWidgetState();
}

class DeviceListWidgetState extends State<DeviceListWidget>
    with WidgetsBindingObserver {
  final GlobalKey<AnimatedListState> deviceListKey =
      GlobalKey<AnimatedListState>();
  StreamSubscription? _subscription;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _subscription = deviceListChangeStream.listen((change) async {
      switch (change.kind) {
        case DeviceListChangeKind.Added:
          {
            deviceListKey.currentState!.insertItem(change.index,
                duration: const Duration(milliseconds: 800));
          }
        case DeviceListChangeKind.Removed:
          {
            deviceListKey.currentState!.removeItem(change.index,
                (BuildContext context, Animation<double> animation) {
              return widget.deviceBuilder(context, change.device,
                  effectiveOrientation(context), animation);
            });
          }
        case DeviceListChangeKind.Named:
          {
            /* do nothing*/
          }
      }
      setState(() => {});
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _subscription?.cancel();
    super.dispose();
  }

  // This is meant to make sure we catch any devices plugged in while the app
  // wasn't in foreground but for some reason it doesn't work.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      globalHostPortHandler.scanDevices();
    }
  }

  @override
  Widget build(BuildContext context) {
    final orientation = effectiveOrientation(context);
    final list = AnimatedList(
        shrinkWrap: true,
        key: deviceListKey,
        itemBuilder: (context, index, animation) {
          final device = api.deviceAtIndex(index: index)!;
          return widget.deviceBuilder(context, device, orientation, animation);
        },
        initialItemCount: api.deviceListState().devices.length,
        scrollDirection: orientation == Orientation.landscape
            ? Axis.horizontal
            : Axis.vertical);

    return list;
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
    final animationBegin = orientation == Orientation.landscape
        ? const Offset(8.0, 0.0)
        : const Offset(0.0, 8.0);
    return SlideTransition(
        position: animation
            .drive(Tween(begin: animationBegin, end: const Offset(0.0, 0.0))),
        child: Padding(
            padding: const EdgeInsets.all(3.0),
            child: Container(
              constraints: BoxConstraints(minHeight: 90.0, minWidth: 150.0),
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

class DeviceListContainer extends StatelessWidget {
  final Widget child;

  const DeviceListContainer({super.key, required this.child});

  @override
  Widget build(BuildContext context) {
    final isPortrait = effectiveOrientation(context) == Orientation.portrait;
    return Container(
        constraints: BoxConstraints(
            maxHeight: isPortrait ? double.maxFinite : 150.0,
            maxWidth: isPortrait ? 300.0 : double.maxFinite),
        // decoration: BoxDecoration(border: Border.all(color: Colors.black, width: 2.0)),
        child: child);
  }
}

Orientation effectiveOrientation(BuildContext context) {
  return Platform.isAndroid
      ? MediaQuery.of(context).orientation
      : Orientation.portrait;
}

// LHS: Override the label, RHS: Set the icon
typedef IconAssigner = (Widget?, Widget?) Function(BuildContext, DeviceId);

class DeviceListWithIcons extends StatelessWidget {
  const DeviceListWithIcons({super.key, required this.iconAssigner});
  final IconAssigner iconAssigner;

  @override
  Widget build(BuildContext context) {
    return DeviceListWidget(
      deviceBuilder: _builder,
    );
  }

  Widget _builder(BuildContext context, Device device, Orientation orientation,
      Animation<double> animation) {
    final (overrideLabel, icon) = iconAssigner.call(context, device.id);
    final _label = overrideLabel ?? LabeledDeviceText(device.name ?? '-');
    return DeviceBoxContainer(
        animation: animation,
        orientation: orientation,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: icon != null
              ? [_label, SizedBox(height: 4), icon, SizedBox(height: 4)]
              : [_label],
        ));
  }
}
