import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnapp/serialport.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'global.dart';

typedef RemovedDeviceBuilder = Widget Function(
    BuildContext context, Device device, Animation<double> animation);

typedef DeviceBuilder = Widget Function(BuildContext context, Device device,
    Orientation orientation, Animation<double> animation);

const double iconSize = 20.0;

class DeviceList extends StatefulWidget {
  final DeviceBuilder deviceBuilder;

  const DeviceList({Key? key, required this.deviceBuilder}) : super(key: key);

  @override
  State<StatefulWidget> createState() => _DeviceListState();
}

class _DeviceListState extends State<DeviceList> with WidgetsBindingObserver {
  GlobalKey<AnimatedListState> deviceListKey = GlobalKey<AnimatedListState>();
  StreamSubscription? _subscription;
  late DeviceListState currentListState;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    currentListState = api.deviceListState();
    _subscription = deviceListUpdateStream.listen((update) async {
      if (update.state.stateId != currentListState.stateId + 1) {
        // our states are out of sync somehow -- reset the list.
        //
        // NOTE: This should never happen in practice but I set up these state
        // ids while debugging to exclude states missing as a possible problem.
        setState(() {
          deviceListKey = GlobalKey();
        });
      } else {
        for (final change in update.changes) {
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
        }
      }
      setState(() {
        currentListState = update.state;
      });
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
        key: deviceListKey,
        itemBuilder: (context, index, animation) {
          final device = currentListState.devices[index];
          return widget.deviceBuilder(context, device, orientation, animation);
        },
        initialItemCount: currentListState.devices.length,
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
            maxHeight: isPortrait ? double.maxFinite : 90.0,
            maxWidth: isPortrait ? 300.0 : double.maxFinite),
        // decoration: BoxDecoration(border: Border.all(color: Colors.black, width: 2.0)),
        child: Align(alignment: Alignment.center, child: child));
  }
}

Orientation effectiveOrientation(BuildContext context) {
  // return Orientation.landscape;
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
    return DeviceList(
      deviceBuilder: _builder,
    );
  }

  Widget _builder(BuildContext context, Device device, Orientation orientation,
      Animation<double> animation) {
    final (overrideLabel, icon) = iconAssigner.call(context, device.id);
    final label = overrideLabel ?? LabeledDeviceText(device.name ?? '-');
    return DeviceBoxContainer(
        animation: animation,
        orientation: orientation,
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.center,
          children: icon != null
              ? [
                  label,
                  SizedBox(height: 4),
                  Container(height: iconSize, child: icon),
                  SizedBox(height: 4)
                ]
              : [label],
        ));
  }
}

class MaybeExpandedVertical extends StatelessWidget {
  final Widget child;
  MaybeExpandedVertical({super.key, required this.child});

  @override
  Widget build(BuildContext ctx) {
    return effectiveOrientation(ctx) == Orientation.portrait
        ? Expanded(child: child)
        : child;
  }
}
