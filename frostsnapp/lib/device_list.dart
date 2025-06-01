import 'dart:async';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:frostsnap/device.dart';
import 'package:frostsnap/device_settings.dart';
import 'package:frostsnap/device_setup.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'global.dart';

typedef RemovedDeviceBuilder =
    Widget Function(
      BuildContext context,
      ConnectedDevice device,
      Animation<double> animation,
    );

typedef DeviceBuilder =
    Widget Function(
      BuildContext context, {
      required ConnectedDevice device,
      required Orientation orientation,
      required Animation<double> animation,
      String? previewName,
    });

const double iconSize = 20.0;

class DeviceList extends StatefulWidget {
  final DeviceBuilder deviceBuilder;

  const DeviceList({Key? key, this.deviceBuilder = buildInteractiveDevice})
    : super(key: key);

  @override
  State<StatefulWidget> createState() => _DeviceListState();
}

class _DeviceListState extends State<DeviceList> {
  // with WidgetsBindingObserver
  GlobalKey<AnimatedListState> deviceListKey = GlobalKey<AnimatedListState>();
  StreamSubscription? _subscription;
  late DeviceListState currentListState;

  @override
  void initState() {
    super.initState();
    // WidgetsBinding.instance.addObserver(this);
    currentListState = coord.deviceListState();
    _subscription = GlobalStreams.deviceListUpdateStream.listen((update) async {
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
            case DeviceListChangeKind.added:
              {
                deviceListKey.currentState!.insertItem(
                  change.index,
                  duration: const Duration(milliseconds: 800),
                );
              }
            case DeviceListChangeKind.removed:
              {
                deviceListKey.currentState!.removeItem(change.index, (
                  BuildContext context,
                  Animation<double> animation,
                ) {
                  return widget.deviceBuilder(
                    context,
                    device: change.device,
                    orientation: effectiveOrientation(context),
                    animation: animation,
                  );
                });
              }
            default:
              {
                /* nothing needs to be done for other states*/
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
    // WidgetsBinding.instance.removeObserver(this);
    _subscription?.cancel();
    super.dispose();
  }

  // // This is meant to make sure we catch any devices plugged in while the app
  // // wasn't in foreground but for some reason it doesn't work.
  // @override
  // void didChangeAppLifecycleState(AppLifecycleState state) {
  //   if (state == AppLifecycleState.resumed) {
  //     globalHostPortHandler.scanDevices();
  //   }
  // }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final orientation = effectiveOrientation(context);

    final noDevices = StreamBuilder(
      stream: GlobalStreams.deviceListSubject,
      builder: (context, snapshot) {
        if (!snapshot.hasData || snapshot.data!.state.devices.isNotEmpty) {
          return SizedBox();
        } else {
          return Text(
            'No devices connected',
            style: theme.textTheme.titleMedium,
          );
        }
      },
    );
    final list = AnimatedList(
      primary:
          true, // I dunno but the scrollbar doesn't work unless you set this
      padding: EdgeInsets.symmetric(vertical: 5),
      shrinkWrap: true,
      key: deviceListKey,
      itemBuilder: (context, index, animation) {
        final device = currentListState.devices[index];
        return widget.deviceBuilder(
          context,
          device: device,
          orientation: orientation,
          animation: animation,
        );
      },
      initialItemCount: currentListState.devices.length,
      scrollDirection:
          orientation == Orientation.landscape
              ? Axis.horizontal
              : Axis.vertical,
    );

    return Stack(
      children: [
        Center(child: noDevices),
        Align(
          alignment: Alignment.topCenter,
          child: Scrollbar(thumbVisibility: true, child: list),
        ),
      ],
    );
  }
}

class DeviceBoxContainer extends StatelessWidget {
  final Animation<double> animation;
  final Widget child;
  final Orientation orientation;

  const DeviceBoxContainer({
    required this.child,
    required this.orientation,
    required this.animation,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    final animationBegin =
        orientation == Orientation.landscape
            ? const Offset(8.0, 0.0)
            : const Offset(0.0, 8.0);
    return SlideTransition(
      position: animation.drive(
        Tween(begin: animationBegin, end: const Offset(0.0, 0.0)),
      ),
      child: Center(child: DeviceWidget(child: child)),
    );
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
        child: Text(
          name ?? "<unamed>",
          style: TextStyle(fontWeight: FontWeight.bold),
        ),
      ),
    );
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
    return DeviceList(deviceBuilder: _builder);
  }

  Widget _builder(
    BuildContext context, {
    required ConnectedDevice device,
    required Orientation orientation,
    required Animation<double> animation,
    String? previewName,
  }) {
    final (overrideLabel, icon) = iconAssigner.call(context, device.id);
    final label = overrideLabel ?? LabeledDeviceText(device.name);
    return DeviceBoxContainer(
      animation: animation,
      orientation: orientation,
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.center,
        children:
            icon != null
                ? [
                  label,
                  SizedBox(height: 4),
                  SizedBox(height: iconSize, child: icon),
                  SizedBox(height: 4),
                ]
                : [label],
      ),
    );
  }
}

Widget buildInteractiveDevice(
  BuildContext context, {
  required ConnectedDevice device,
  required Orientation orientation,
  required Animation<double> animation,
  String? previewName,
}) {
  final theme = Theme.of(context);
  final List<Widget> children = [];
  final upToDate = device.firmwareDigest == coord.upgradeFirmwareDigest();
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
        style: ElevatedButton.styleFrom(
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(4.0), // Rectangular shape
          ),
        ),
        onPressed: () async {
          await Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) {
                return DeviceSetup(id: device.id);
              },
            ),
          );
        },
        child: Text(
          'New Device',
          style: theme.textTheme.bodyMedium?.copyWith(
            fontWeight: FontWeight.bold,
          ),
          textAlign: TextAlign.center,
        ),
      );
    } else {
      interaction = IconButton(
        icon: Icon(Icons.settings),
        onPressed: () {
          Navigator.push(
            context,
            MaterialPageRoute(
              builder: (context) => DeviceSettings(id: device.id),
            ),
          );
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
        ),
        SizedBox(height: 6.0),
        Text("Upgrade", style: theme.textTheme.bodyMedium),
        Text("Firmware", style: theme.textTheme.bodyMedium),
      ],
    );
  }
  children.add(interaction);
  children.add(Spacer(flex: 10));
  return DeviceBoxContainer(
    orientation: orientation,
    animation: animation,
    child: Column(mainAxisSize: MainAxisSize.max, children: children),
  );
}
