import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnapp/device.dart';
import 'package:frostsnapp/device_settings.dart';
import 'package:frostsnapp/device_setup.dart';
import 'package:frostsnapp/theme.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';
import 'global.dart';

typedef RemovedDeviceBuilder = Widget Function(
    BuildContext context, ConnectedDevice device, Animation<double> animation);

typedef DeviceBuilder = Widget Function(
    BuildContext context,
    ConnectedDevice device,
    Orientation orientation,
    Animation<double> animation);

const double iconSize = 20.0;

class DeviceList extends StatefulWidget {
  final DeviceBuilder deviceBuilder;

  DeviceList({
    Key? key,
    this.deviceBuilder = buildInteractiveDevice,
  }) : super(key: key);

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
                /* nothing needs to be done to the list. The name will be updated with setState*/
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

    final noDevices = StreamBuilder(
        stream: deviceListSubject,
        builder: (context, snapshot) {
          if (!snapshot.hasData || snapshot.data!.state.devices.isNotEmpty) {
            return SizedBox();
          } else {
            return Text(
              'No devices connected',
              style: TextStyle(color: uninterestedColor, fontSize: 24.0),
            );
          }
        });
    final list = AnimatedList(
        primary:
            true, // I dunno but the scrollbar doesn't work unless you set this
        padding: EdgeInsets.symmetric(vertical: 5),
        shrinkWrap: true,
        key: deviceListKey,
        itemBuilder: (context, index, animation) {
          final device = currentListState.devices[index];
          return widget.deviceBuilder(context, device, orientation, animation);
        },
        initialItemCount: currentListState.devices.length,
        scrollDirection: orientation == Orientation.landscape
            ? Axis.horizontal
            : Axis.vertical);

    return Stack(
      children: [
        Center(
          child: noDevices,
        ),
        Align(
            alignment: Alignment.topCenter,
            child: Scrollbar(thumbVisibility: true, child: list))
      ],
    );
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
        child: Center(child: DeviceWidget(child: child)));
  }
}

class LabeledDeviceText extends StatelessWidget {
  final String? name;

  const LabeledDeviceText(this.name, {super.key});

  @override
  Widget build(BuildContext context) {
    // put a SizedBox to keep the same height even if the FittedBox shrinks the width
    return SizedBox(
        height: 25.0,
        child: FittedBox(
          fit: BoxFit.contain,
          child: Text(name ?? "<unamed>",
              style: TextStyle(fontWeight: FontWeight.bold)),
        ));
  }
}

// XXX: The orientation stuff has no effect at the moment it's just here in case
// we want to come back to it
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

  Widget _builder(BuildContext context, ConnectedDevice device,
      Orientation orientation, Animation<double> animation) {
    final (overrideLabel, icon) = iconAssigner.call(context, device.id);
    final label = overrideLabel ?? LabeledDeviceText(device.name);
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

Widget buildInteractiveDevice(BuildContext context, ConnectedDevice device,
    Orientation orientation, Animation<double> animation) {
  Widget child;
  final List<Widget> children = [];
  final upToDate = device.firmwareDigest! == coord.upgradeFirmwareDigest();

  if (device.name == null) {
    children.add(Spacer(flex: 6));
  } else {
    children.add(LabeledDeviceText(device.name!));
  }

  children.add(Spacer(flex: 3));

  final Widget interaction;

  if (upToDate) {
    if (device.name == null) {
      interaction = TextButton(
        style: TextButton.styleFrom(
            shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(4.0), // Rectangular shape
        )),
        onPressed: () async {
          await Navigator.push(context, MaterialPageRoute(builder: (context) {
            return DeviceSetup(id: device.id);
          }));
        },
        child: Text('New device', style: TextStyle(fontSize: 12.0)),
      );
    } else {
      interaction = IconButton(
        icon: Icon(Icons.settings),
        onPressed: () {
          Navigator.push(
              context,
              MaterialPageRoute(
                  builder: (context) => DeviceSettings(id: device.id)));
        },
      );
    }
  } else {
    interaction = Column(
        mainAxisAlignment: MainAxisAlignment.center,
        mainAxisSize: MainAxisSize.max,
        children: [
          IconButton.outlined(
            onPressed: () {
              FirmwareUpgradeDialog.show(context);
            },
            icon: Icon(Icons.upgrade),
            color: awaitingColor,
          ),
          SizedBox(height: 5.0),
          Text("Upgrade", style: TextStyle(color: textColor, fontSize: 13.0)),
          Text("Firmware", style: TextStyle(color: textColor, fontSize: 13.0))
        ]);
  }

  children.add(interaction);
  children.add(Spacer(flex: 10));

  return DeviceBoxContainer(
      orientation: orientation,
      animation: animation,
      child: Column(children: children, mainAxisSize: MainAxisSize.max));
}
