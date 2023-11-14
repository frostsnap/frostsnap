import 'dart:async';
import 'package:flutter/material.dart';
import 'ffi.dart' if (dart.library.html) 'ffi_web.dart';

import 'dart:collection';
import 'package:flutter/foundation.dart';
import 'package:frostsnapp/bridge_definitions.dart';
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
    required this.state,
    this.name,
  });
  final DeviceListChangeKind kind;
  final int index;
  final DeviceId id;
  final String? name;
  final DeviceListState state;
}

class DeviceListState {
  final List<DeviceId> devices;
  final Map<DeviceId, String> names;

  DeviceListState({required this.devices, required this.names});

  Iterable<DeviceId> namedDevices() {
    return devices.where((id) => names[id] != null);
  }
}

class DeviceList {
  DeviceList() {
    api.subDeviceEvents().forEach((deviceChanges) {
      for (final change in deviceChanges) {
        switch (change) {
          case DeviceChange_Added(:final id):
            {
              developer.log("Device connected");
            }
          case DeviceChange_Registered(:final id, :final name):
            {
              int index = indexOf(id);
              if (index != -1 && state.names[id] != name) {
                state.names[id] = name;
                _emitEvent(DeviceListChangeKind.named, id, index);
              } else {
                state.names[id] = name;
                _append(id);
              }
            }
          case DeviceChange_Disconnected(:final id):
            {
              developer.log("device disconnected");
              var index = indexOf(id);
              if (index != -1) {
                var id = state.devices.removeAt(index);
                _emitEvent(DeviceListChangeKind.removed, id, index);
              }
            }
          case DeviceChange_NeedsName(:final id):
            {
              _append(id);
            }
          case DeviceChange_Renamed(:final id, :final newName, :final oldName):
            {
              state.names[id] = newName;
            }
        }
      }
    });
  }

  final DeviceListState state = DeviceListState(
      devices: [],
      names: LinkedHashMap<DeviceId, String>(
        equals: (a, b) => listEquals(a.field0, b.field0),
        hashCode: (a) => Object.hashAll(a.field0),
      ));

  final StreamController<DeviceListChange> changeStream =
      StreamController.broadcast();

  Stream<DeviceListChange> subscribe() {
    return changeStream.stream;
  }

  _emitEvent(DeviceListChangeKind kind, DeviceId id, int index) {
    changeStream.sink.add(DeviceListChange(
        kind: kind, index: index, id: id, name: state.names[id], state: state));
  }

  _append(DeviceId device) {
    if (indexOf(device) == -1) {
      state.devices.add(device);
      _emitEvent(DeviceListChangeKind.added, device, state.devices.length - 1);
    }
  }

  int indexOf(DeviceId item) =>
      state.devices.indexWhere((element) => deviceIdEquals(element, item));
  DeviceId operator [](int index) => state.devices[index];
}

HashSet<DeviceId> deviceIdSet() {
  return HashSet<DeviceId>(
      equals: (a, b) => listEquals(a.field0, b.field0),
      hashCode: (a) => Object.hashAll(a.field0));
}

bool deviceIdEquals(DeviceId lhs, DeviceId rhs) =>
    listEquals(lhs.field0.toList(), rhs.field0.toList());
