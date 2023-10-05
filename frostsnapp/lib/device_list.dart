import 'package:flutter/material.dart';
import 'package:frostsnapp/coordinator.dart';
import 'package:frostsnapp/do_keygen.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

typedef DeviceId = String;

class UnlabeledDeviceTextField extends StatelessWidget {
  final ValueChanged<String> onNameSubmit;

  const UnlabeledDeviceTextField({required this.onNameSubmit, super.key});

  @override
  Widget build(BuildContext context) {
    return TextField(
        onSubmitted: onNameSubmit,
        textAlign: TextAlign.center,
        style: TextStyle(fontSize: 30),
        decoration: InputDecoration(
          hintText: "name me",
          hintStyle: TextStyle(color: Colors.grey.withOpacity(0.6)),
          border: InputBorder.none,
        ));
  }
}

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
              setState(() => _deviceList.append(id));
            }
          case DeviceChange_Registered(:final id, :final label):
            {
              setState(() => _deviceList.setName(id, label));
            }
          case DeviceChange_Disconnected(:final id):
            {
              setState(() => _deviceList.removeDevice(id));
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
    // Only show the keygen button if there are some devices
    bool isKeygenButtonVisible = _deviceList.length > 0;

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
      DoKeyGenButton(isvisible: isKeygenButtonVisible)
    ]);
  }

  Widget _buildDevice(BuildContext context, DeviceId id, String? label,
      Animation<double> animation) {
    Widget child;
    if (label == null) {
      child = UnlabeledDeviceTextField(onNameSubmit: (name) {
        global_coordinator.setDeviceLabel(id, name);
      });
    } else {
      child = LabeledDeviceText(label);
    }

    return DeviceBoxContainer(
        orientation: widget.orientation, animation: animation, child: child);
  }

  Widget _buildItem(
      BuildContext context, int index, Animation<double> animation) {
    var id = _deviceList[index];
    var label = _deviceList._labels[id];
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
  final Map<DeviceId, String> _labels = {};
  final RemovedDeviceBuilder removedDeviceBuilder;

  AnimatedListState? get _animatedList => listKey.currentState;

  void append(DeviceId device) {
    _items.add(device);
    _animatedList!.insertItem(_items.length - 1,
        duration: const Duration(milliseconds: 800));
  }

  void setName(DeviceId device, String label) {
    _labels[device] = label;
  }

  removeDevice(DeviceId id) {
    var index = _items.indexOf(id);
    if (index != -1) {
      _items.removeAt(index);
      _animatedList!.removeItem(index,
          (BuildContext context, Animation<double> animation) {
        return removedDeviceBuilder(context, id, _labels[id], animation);
      });
    }
  }

  get labels => _labels;

  int get length => _items.length;

  String operator [](int index) => _items[index];

  int indexOf(String item) => _items.indexOf(item);
}
