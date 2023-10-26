import 'dart:async';

import 'package:flutter/material.dart';

import 'dart:collection';
import 'package:flutter/foundation.dart';
import 'package:frostsnapp/bridge_definitions.dart';
import 'package:frostsnapp/coordinator.dart';
import 'dart:developer' as developer;

DeviceList globalDeviceList = DeviceList();

enum DeviceListChangeKind {
  added,
  removed,
  named,
}

class DeviceListChange {
  DeviceListChange({
    required this.kind,
    required this.index,
    required this.id,
    this.name,
  });
  final DeviceListChangeKind kind;
  final int index;
  final DeviceId id;
  final String? name;
}

class DeviceList {
  DeviceList() {
    global_coordinator.subDeviceEvents().forEach((deviceChanges) {
      for (final change in deviceChanges) {
        switch (change) {
          case DeviceChange_Added(:final id):
            {
              developer.log("Device connected");
            }
          case DeviceChange_Registered(:final id, :final name):
            {
              int index = indexOf(id);
              if (index != -1 && getName(id) != name) {
                changeStream.sink.add(DeviceListChange(
                    kind: DeviceListChangeKind.named,
                    index: index,
                    id: id,
                    name: name));
              } else {
                // if (this.confirmNameDialogue != null) {
                //   for (var ctx in _deviceList.confirmNameDialogue!) {
                //     Navigator.pop(ctx);
                //   }
                //   _deviceList.confirmNameDialogue = null;
                // }
                _append(id);
              }
              _labels[id] = name;
            }
          case DeviceChange_Disconnected(:final id):
            {
              developer.log("device disconnected");
              var index = indexOf(id);
              if (index != -1) {
                var id = _items.removeAt(index);
                changeStream.sink.add(DeviceListChange(
                    kind: DeviceListChangeKind.removed,
                    index: index,
                    id: id,
                    name: getName(id)));
              }
            }
          case DeviceChange_NeedsName(:final id):
            {
              _append(id);
            }
          case DeviceChange_Renamed(:final id, :final newName, :final oldName):
            {
              _labels[id] = newName;
            }
        }
      }
    });
  }

  final List<DeviceId> _items = [];
  final Map<DeviceId, String> _labels = LinkedHashMap<DeviceId, String>(
    equals: (a, b) => listEquals(a.field0, b.field0),
    hashCode: (a) => Object.hashAll(a.field0),
  );
  final StreamController<DeviceListChange> changeStream =
      StreamController.broadcast();

  Stream<DeviceListChange> subscribe() {
    return changeStream.stream;
  }

  _append(DeviceId device) {
    if (indexOf(device) == -1) {
      _items.add(device);
      changeStream.sink.add(DeviceListChange(
          kind: DeviceListChangeKind.added,
          index: _items.length - 1,
          id: device,
          name: getName(device)));
    }
  }

  String? getName(DeviceId device) {
    return _labels[device];
  }

  int lengthNamed() {
    return _items.where((element) => getName(element) != null).length;
  }

  int get length => _items.length;
  int indexOf(DeviceId item) => _items.indexWhere(
      (element) => listEquals(element.field0.toList(), item.field0.toList()));
  DeviceId operator [](int index) => _items[index];
}
