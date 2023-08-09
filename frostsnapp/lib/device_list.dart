import 'package:flutter/material.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

typedef DeviceId = String;

class UnlabeledDeviceTextField extends StatelessWidget {
  final ValueChanged<String> onNameSubmit;

  UnlabeledDeviceTextField({required this.onNameSubmit, super.key});

  @override
  Widget build(BuildContext context) {
    return TextField(
        onSubmitted: onNameSubmit,
        textAlign: TextAlign.center,
        style: TextStyle(fontSize: 30),
        decoration: InputDecoration(
          hintText: "name this device",
          border: InputBorder.none,
        ));
  }
}

class DeviceListWidget extends StatefulWidget {
  final FfiCoordinator coordinator;
  final Stream<List<DeviceChange>> deviceEvents;
  const DeviceListWidget(
      {required this.coordinator, required this.deviceEvents, super.key});

  @override
  State<StatefulWidget> createState() => DeviceListWidgetState();
}

class DeviceListWidgetState extends State<DeviceListWidget> {
  final GlobalKey<AnimatedListState> deviceListKey =
      GlobalKey<AnimatedListState>();
  late DeviceList _deviceList;

  @override
  void initState() {
    super.initState();
    _deviceList =
        DeviceList(listKey: deviceListKey, removedDeviceBuilder: _buildDevice);
    widget.deviceEvents.forEach((deviceChanges) {
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
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedList(key: deviceListKey, itemBuilder: _buildItem);
  }

  Widget _buildDevice(BuildContext context, DeviceId id, String? label,
      Animation<double> animation) {
    Widget child;
    if (label == null) {
      child = UnlabeledDeviceTextField(onNameSubmit: (name) {
        api.setDeviceLabel(
            coordinator: widget.coordinator, deviceId: id, label: name);
      });
    } else {
      child = LabeledDeviceText(label);
    }

    return DeviceBoxContainer(animation: animation, child: child);
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

  DeviceBoxContainer({required this.child, required this.animation, super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
        padding: const EdgeInsets.all(2.0),
        child: SlideTransition(
            position: animation.drive(Tween(
                begin: const Offset(0.0, 8.0), end: const Offset(0.0, 0.0))),
            child: SizedBox(
              height: 80.0,
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

  LabeledDeviceText(this.name, {super.key});

  @override
  Widget build(BuildContext context) {
    return Text(name, style: TextStyle(fontSize: 30));
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
