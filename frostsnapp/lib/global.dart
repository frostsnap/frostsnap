import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:frostsnap/src/rust/api.dart';
import 'package:frostsnap/src/rust/api/coordinator.dart';
import 'package:frostsnap/src/rust/api/device_list.dart';
import 'package:frostsnap/stream_ext.dart';
import 'serialport.dart';

late Coordinator coord;
late Api api;
late HostPortHandler? globalHostPortHandler;

final nameInputFormatter = TextInputFormatter.withFunction((
  oldValue,
  newValue,
) {
  final text = newValue.text;

  // Allow empty string
  if (text.isEmpty) return newValue;

  // reject leading spaces (always wrong)
  if (text.startsWith(' ')) {
    return oldValue;
  }

  // reject double spaces
  if (text.contains('  ')) {
    return oldValue;
  }

  // reject disallowed characters
  if (!RegExp(r"^[a-zA-Z0-9_\-' ]+$").hasMatch(text)) {
    return oldValue;
  }

  return newValue;
});

class GlobalStreams {
  /// Gets new updates from the device list
  static final Stream<DeviceListUpdate> deviceListUpdateStream = coord
      .subDeviceEvents()
      .asBroadcastStream();

  /// Stream of device list changes. Only emits when there is a change.
  static final Stream<DeviceListChange> deviceListChangeStream =
      deviceListUpdateStream.asyncExpand(
        (update) => Stream.fromIterable(update.changes),
      );

  /// DeviceListUpdates as a behavior subject
  static final Stream<DeviceListUpdate> deviceListSubject =
      deviceListUpdateStream.toBehaviorSubject();

  static final Stream<KeyState> keyStateSubject = coord
      .subKeyEvents()
      .toBehaviorSubject();
}

// Global key so that snackbar will always show on top.
final rootScaffoldMessengerKey = GlobalKey<ScaffoldMessengerState>();

final rootNavKey = GlobalKey<NavigatorState>();
