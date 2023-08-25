// AUTO GENERATED FILE, DO NOT EDIT.
// Generated by `flutter_rust_bridge`@ 1.81.0.
// ignore_for_file: non_constant_identifier_names, unused_element, duplicate_ignore, directives_ordering, curly_braces_in_flow_control_structures, unnecessary_lambdas, slash_for_doc_comments, prefer_const_literals_to_create_immutables, implicit_dynamic_list_literal, duplicate_import, unused_import, unnecessary_import, prefer_single_quotes, prefer_const_constructors, use_super_parameters, always_use_package_imports, annotate_overrides, invalid_use_of_protected_member, constant_identifier_names, invalid_use_of_internal_member, prefer_is_empty, unnecessary_const

import "bridge_definitions.dart";
import 'dart:convert';
import 'dart:async';
import 'package:meta/meta.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:uuid/uuid.dart';
import 'bridge_generated.io.dart'
    if (dart.library.html) 'bridge_generated.web.dart';

class NativeImpl implements Native {
  final NativePlatform _platform;
  factory NativeImpl(ExternalLibrary dylib) =>
      NativeImpl.raw(NativePlatform(dylib));

  /// Only valid on web/WASM platforms.
  factory NativeImpl.wasm(FutureOr<WasmModule> module) =>
      NativeImpl(module as ExternalLibrary);
  NativeImpl.raw(this._platform);
  Stream<CoordinatorEvent> initEvents({dynamic hint}) {
    return _platform.executeStream(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_init_events(port_),
      parseSuccessData: _wire2api_coordinator_event,
      constMeta: kInitEventsConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kInitEventsConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "init_events",
        argNames: [],
      );

  Stream<List<DeviceChange>> subDeviceEvents({dynamic hint}) {
    return _platform.executeStream(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_sub_device_events(port_),
      parseSuccessData: _wire2api_list_device_change,
      constMeta: kSubDeviceEventsConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSubDeviceEventsConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "sub_device_events",
        argNames: [],
      );

  Future<FfiCoordinator> newFfiCoordinator(
      {required bool hostHandlesSerial, dynamic hint}) {
    var arg0 = hostHandlesSerial;
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_new_ffi_coordinator(port_, arg0),
      parseSuccessData: _wire2api_FfiCoordinator,
      constMeta: kNewFfiCoordinatorConstMeta,
      argValues: [hostHandlesSerial],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kNewFfiCoordinatorConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "new_ffi_coordinator",
        argNames: ["hostHandlesSerial"],
      );

  Future<void> turnStderrLoggingOn({required Level level, dynamic hint}) {
    var arg0 = api2wire_level(level);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_turn_stderr_logging_on(port_, arg0),
      parseSuccessData: _wire2api_unit,
      constMeta: kTurnStderrLoggingOnConstMeta,
      argValues: [level],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kTurnStderrLoggingOnConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "turn_stderr_logging_on",
        argNames: ["level"],
      );

  Future<void> turnLogcatLoggingOn({required Level level, dynamic hint}) {
    var arg0 = api2wire_level(level);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_turn_logcat_logging_on(port_, arg0),
      parseSuccessData: _wire2api_unit,
      constMeta: kTurnLogcatLoggingOnConstMeta,
      argValues: [level],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kTurnLogcatLoggingOnConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "turn_logcat_logging_on",
        argNames: ["level"],
      );

  Future<void> announceAvailablePorts(
      {required FfiCoordinator coordinator,
      required List<PortDesc> ports,
      dynamic hint}) {
    var arg0 = _platform.api2wire_FfiCoordinator(coordinator);
    var arg1 = _platform.api2wire_list_port_desc(ports);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_announce_available_ports(port_, arg0, arg1),
      parseSuccessData: _wire2api_unit,
      constMeta: kAnnounceAvailablePortsConstMeta,
      argValues: [coordinator, ports],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kAnnounceAvailablePortsConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "announce_available_ports",
        argNames: ["coordinator", "ports"],
      );

  Future<void> setDeviceLabel(
      {required FfiCoordinator coordinator,
      required String deviceId,
      required String label,
      dynamic hint}) {
    var arg0 = _platform.api2wire_FfiCoordinator(coordinator);
    var arg1 = _platform.api2wire_String(deviceId);
    var arg2 = _platform.api2wire_String(label);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_set_device_label(port_, arg0, arg1, arg2),
      parseSuccessData: _wire2api_unit,
      constMeta: kSetDeviceLabelConstMeta,
      argValues: [coordinator, deviceId, label],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSetDeviceLabelConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "set_device_label",
        argNames: ["coordinator", "deviceId", "label"],
      );

  Future<void> satisfyMethodPortOpen(
      {required PortOpen that, String? err, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_port_open(that);
    var arg1 = _platform.api2wire_opt_String(err);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_satisfy__method__PortOpen(port_, arg0, arg1),
      parseSuccessData: _wire2api_unit,
      constMeta: kSatisfyMethodPortOpenConstMeta,
      argValues: [that, err],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortOpenConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "satisfy__method__PortOpen",
        argNames: ["that", "err"],
      );

  Future<void> satisfyMethodPortRead(
      {required PortRead that,
      required Uint8List bytes,
      String? err,
      dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_port_read(that);
    var arg1 = _platform.api2wire_uint_8_list(bytes);
    var arg2 = _platform.api2wire_opt_String(err);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner
          .wire_satisfy__method__PortRead(port_, arg0, arg1, arg2),
      parseSuccessData: _wire2api_unit,
      constMeta: kSatisfyMethodPortReadConstMeta,
      argValues: [that, bytes, err],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortReadConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "satisfy__method__PortRead",
        argNames: ["that", "bytes", "err"],
      );

  Future<void> satisfyMethodPortWrite(
      {required PortWrite that, String? err, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_port_write(that);
    var arg1 = _platform.api2wire_opt_String(err);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_satisfy__method__PortWrite(port_, arg0, arg1),
      parseSuccessData: _wire2api_unit,
      constMeta: kSatisfyMethodPortWriteConstMeta,
      argValues: [that, err],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortWriteConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "satisfy__method__PortWrite",
        argNames: ["that", "err"],
      );

  Future<void> satisfyMethodPortBytesToRead(
      {required PortBytesToRead that, required int bytesToRead, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_port_bytes_to_read(that);
    var arg1 = api2wire_u32(bytesToRead);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner
          .wire_satisfy__method__PortBytesToRead(port_, arg0, arg1),
      parseSuccessData: _wire2api_unit,
      constMeta: kSatisfyMethodPortBytesToReadConstMeta,
      argValues: [that, bytesToRead],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortBytesToReadConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "satisfy__method__PortBytesToRead",
        argNames: ["that", "bytesToRead"],
      );

  DropFnType get dropOpaqueFfiCoordinator =>
      _platform.inner.drop_opaque_FfiCoordinator;
  ShareFnType get shareOpaqueFfiCoordinator =>
      _platform.inner.share_opaque_FfiCoordinator;
  OpaqueTypeFinalizer get FfiCoordinatorFinalizer =>
      _platform.FfiCoordinatorFinalizer;

  DropFnType get dropOpaquePortBytesToReadSender =>
      _platform.inner.drop_opaque_PortBytesToReadSender;
  ShareFnType get shareOpaquePortBytesToReadSender =>
      _platform.inner.share_opaque_PortBytesToReadSender;
  OpaqueTypeFinalizer get PortBytesToReadSenderFinalizer =>
      _platform.PortBytesToReadSenderFinalizer;

  DropFnType get dropOpaquePortOpenSender =>
      _platform.inner.drop_opaque_PortOpenSender;
  ShareFnType get shareOpaquePortOpenSender =>
      _platform.inner.share_opaque_PortOpenSender;
  OpaqueTypeFinalizer get PortOpenSenderFinalizer =>
      _platform.PortOpenSenderFinalizer;

  DropFnType get dropOpaquePortReadSender =>
      _platform.inner.drop_opaque_PortReadSender;
  ShareFnType get shareOpaquePortReadSender =>
      _platform.inner.share_opaque_PortReadSender;
  OpaqueTypeFinalizer get PortReadSenderFinalizer =>
      _platform.PortReadSenderFinalizer;

  DropFnType get dropOpaquePortWriteSender =>
      _platform.inner.drop_opaque_PortWriteSender;
  ShareFnType get shareOpaquePortWriteSender =>
      _platform.inner.share_opaque_PortWriteSender;
  OpaqueTypeFinalizer get PortWriteSenderFinalizer =>
      _platform.PortWriteSenderFinalizer;

  void dispose() {
    _platform.dispose();
  }
// Section: wire2api

  FfiCoordinator _wire2api_FfiCoordinator(dynamic raw) {
    return FfiCoordinator.fromRaw(raw[0], raw[1], this);
  }

  PortBytesToReadSender _wire2api_PortBytesToReadSender(dynamic raw) {
    return PortBytesToReadSender.fromRaw(raw[0], raw[1], this);
  }

  PortOpenSender _wire2api_PortOpenSender(dynamic raw) {
    return PortOpenSender.fromRaw(raw[0], raw[1], this);
  }

  PortReadSender _wire2api_PortReadSender(dynamic raw) {
    return PortReadSender.fromRaw(raw[0], raw[1], this);
  }

  PortWriteSender _wire2api_PortWriteSender(dynamic raw) {
    return PortWriteSender.fromRaw(raw[0], raw[1], this);
  }

  String _wire2api_String(dynamic raw) {
    return raw as String;
  }

  PortBytesToRead _wire2api_box_autoadd_port_bytes_to_read(dynamic raw) {
    return _wire2api_port_bytes_to_read(raw);
  }

  PortOpen _wire2api_box_autoadd_port_open(dynamic raw) {
    return _wire2api_port_open(raw);
  }

  PortRead _wire2api_box_autoadd_port_read(dynamic raw) {
    return _wire2api_port_read(raw);
  }

  PortWrite _wire2api_box_autoadd_port_write(dynamic raw) {
    return _wire2api_port_write(raw);
  }

  CoordinatorEvent _wire2api_coordinator_event(dynamic raw) {
    switch (raw[0]) {
      case 0:
        return CoordinatorEvent_PortOpen(
          request: _wire2api_box_autoadd_port_open(raw[1]),
        );
      case 1:
        return CoordinatorEvent_PortWrite(
          request: _wire2api_box_autoadd_port_write(raw[1]),
        );
      case 2:
        return CoordinatorEvent_PortRead(
          request: _wire2api_box_autoadd_port_read(raw[1]),
        );
      case 3:
        return CoordinatorEvent_PortBytesToRead(
          request: _wire2api_box_autoadd_port_bytes_to_read(raw[1]),
        );
      default:
        throw Exception("unreachable");
    }
  }

  DeviceChange _wire2api_device_change(dynamic raw) {
    switch (raw[0]) {
      case 0:
        return DeviceChange_Added(
          id: _wire2api_String(raw[1]),
        );
      case 1:
        return DeviceChange_Registered(
          id: _wire2api_String(raw[1]),
          label: _wire2api_String(raw[2]),
        );
      case 2:
        return DeviceChange_Disconnected(
          id: _wire2api_String(raw[1]),
        );
      default:
        throw Exception("unreachable");
    }
  }

  List<DeviceChange> _wire2api_list_device_change(dynamic raw) {
    return (raw as List<dynamic>).map(_wire2api_device_change).toList();
  }

  PortBytesToRead _wire2api_port_bytes_to_read(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 2)
      throw Exception('unexpected arr length: expect 2 but see ${arr.length}');
    return PortBytesToRead(
      bridge: this,
      id: _wire2api_String(arr[0]),
      ready: _wire2api_PortBytesToReadSender(arr[1]),
    );
  }

  PortOpen _wire2api_port_open(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 3)
      throw Exception('unexpected arr length: expect 3 but see ${arr.length}');
    return PortOpen(
      bridge: this,
      id: _wire2api_String(arr[0]),
      baudRate: _wire2api_u32(arr[1]),
      ready: _wire2api_PortOpenSender(arr[2]),
    );
  }

  PortRead _wire2api_port_read(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 3)
      throw Exception('unexpected arr length: expect 3 but see ${arr.length}');
    return PortRead(
      bridge: this,
      id: _wire2api_String(arr[0]),
      len: _wire2api_usize(arr[1]),
      ready: _wire2api_PortReadSender(arr[2]),
    );
  }

  PortWrite _wire2api_port_write(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 3)
      throw Exception('unexpected arr length: expect 3 but see ${arr.length}');
    return PortWrite(
      bridge: this,
      id: _wire2api_String(arr[0]),
      bytes: _wire2api_uint_8_list(arr[1]),
      ready: _wire2api_PortWriteSender(arr[2]),
    );
  }

  int _wire2api_u32(dynamic raw) {
    return raw as int;
  }

  int _wire2api_u8(dynamic raw) {
    return raw as int;
  }

  Uint8List _wire2api_uint_8_list(dynamic raw) {
    return raw as Uint8List;
  }

  void _wire2api_unit(dynamic raw) {
    return;
  }

  int _wire2api_usize(dynamic raw) {
    return castInt(raw);
  }
}

// Section: api2wire

@protected
bool api2wire_bool(bool raw) {
  return raw;
}

@protected
int api2wire_i32(int raw) {
  return raw;
}

@protected
int api2wire_level(Level raw) {
  return api2wire_i32(raw.index);
}

@protected
int api2wire_u16(int raw) {
  return raw;
}

@protected
int api2wire_u32(int raw) {
  return raw;
}

@protected
int api2wire_u8(int raw) {
  return raw;
}

@protected
int api2wire_usize(int raw) {
  return raw;
}
// Section: finalizer
