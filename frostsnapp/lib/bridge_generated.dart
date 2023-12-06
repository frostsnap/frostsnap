// AUTO GENERATED FILE, DO NOT EDIT.
// Generated by `flutter_rust_bridge`@ 1.82.4.
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
  Stream<PortEvent> subPortEvents({dynamic hint}) {
    return _platform.executeStream(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_sub_port_events(port_),
      parseSuccessData: _wire2api_port_event,
      parseErrorData: null,
      constMeta: kSubPortEventsConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSubPortEventsConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "sub_port_events",
        argNames: [],
      );

  Stream<DeviceListUpdate> subDeviceEvents({dynamic hint}) {
    return _platform.executeStream(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_sub_device_events(port_),
      parseSuccessData: _wire2api_device_list_update,
      parseErrorData: null,
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

  Stream<KeyState> subKeyEvents({dynamic hint}) {
    return _platform.executeStream(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_sub_key_events(port_),
      parseSuccessData: _wire2api_key_state,
      parseErrorData: null,
      constMeta: kSubKeyEventsConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSubKeyEventsConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "sub_key_events",
        argNames: [],
      );

  Future<void> emitKeyEvent({required KeyState event, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_key_state(event);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_emit_key_event(port_, arg0),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kEmitKeyEventConstMeta,
      argValues: [event],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kEmitKeyEventConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "emit_key_event",
        argNames: ["event"],
      );

  Future<void> turnStderrLoggingOn({required Level level, dynamic hint}) {
    var arg0 = api2wire_level(level);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_turn_stderr_logging_on(port_, arg0),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
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
      parseErrorData: null,
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
      {required List<PortDesc> ports, dynamic hint}) {
    var arg0 = _platform.api2wire_list_port_desc(ports);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_announce_available_ports(port_, arg0),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kAnnounceAvailablePortsConstMeta,
      argValues: [ports],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kAnnounceAvailablePortsConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "announce_available_ports",
        argNames: ["ports"],
      );

  Future<void> switchToHostHandlesSerial({dynamic hint}) {
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_switch_to_host_handles_serial(port_),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kSwitchToHostHandlesSerialConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSwitchToHostHandlesSerialConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "switch_to_host_handles_serial",
        argNames: [],
      );

  Future<void> updateNamePreview(
      {required DeviceId id, required String name, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_device_id(id);
    var arg1 = _platform.api2wire_String(name);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_update_name_preview(port_, arg0, arg1),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kUpdateNamePreviewConstMeta,
      argValues: [id, name],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kUpdateNamePreviewConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "update_name_preview",
        argNames: ["id", "name"],
      );

  Future<void> finishNaming(
      {required DeviceId id, required String name, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_device_id(id);
    var arg1 = _platform.api2wire_String(name);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_finish_naming(port_, arg0, arg1),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kFinishNamingConstMeta,
      argValues: [id, name],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kFinishNamingConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "finish_naming",
        argNames: ["id", "name"],
      );

  Future<void> sendCancel({required DeviceId id, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_device_id(id);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_send_cancel(port_, arg0),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kSendCancelConstMeta,
      argValues: [id],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kSendCancelConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "send_cancel",
        argNames: ["id"],
      );

  Future<void> cancelAll({dynamic hint}) {
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_cancel_all(port_),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kCancelAllConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kCancelAllConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "cancel_all",
        argNames: [],
      );

  Future<List<DeviceId>> registeredDevices({dynamic hint}) {
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_registered_devices(port_),
      parseSuccessData: _wire2api_list_device_id,
      parseErrorData: null,
      constMeta: kRegisteredDevicesConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kRegisteredDevicesConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "registered_devices",
        argNames: [],
      );

  Future<void> startCoordinatorThread({dynamic hint}) {
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) => _platform.inner.wire_start_coordinator_thread(port_),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
      constMeta: kStartCoordinatorThreadConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kStartCoordinatorThreadConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "start_coordinator_thread",
        argNames: [],
      );

  KeyState keyState({dynamic hint}) {
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_key_state(),
      parseSuccessData: _wire2api_key_state,
      parseErrorData: null,
      constMeta: kKeyStateConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kKeyStateConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "key_state",
        argNames: [],
      );

  FrostKey? getKey({required KeyId keyId, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_key_id(keyId);
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_get_key(arg0),
      parseSuccessData: _wire2api_opt_box_autoadd_frost_key,
      parseErrorData: null,
      constMeta: kGetKeyConstMeta,
      argValues: [keyId],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kGetKeyConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "get_key",
        argNames: ["keyId"],
      );

  Device? deviceAtIndex({required int index, dynamic hint}) {
    var arg0 = api2wire_usize(index);
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_device_at_index(arg0),
      parseSuccessData: _wire2api_opt_box_autoadd_device,
      parseErrorData: null,
      constMeta: kDeviceAtIndexConstMeta,
      argValues: [index],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kDeviceAtIndexConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "device_at_index",
        argNames: ["index"],
      );

  DeviceListState deviceListState({dynamic hint}) {
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_device_list_state(),
      parseSuccessData: _wire2api_device_list_state,
      parseErrorData: null,
      constMeta: kDeviceListStateConstMeta,
      argValues: [],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kDeviceListStateConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "device_list_state",
        argNames: [],
      );

  Stream<CoordinatorToUserSigningMessage> startSigning(
      {required KeyId keyId,
      required List<DeviceId> devices,
      required String message,
      dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_key_id(keyId);
    var arg1 = _platform.api2wire_list_device_id(devices);
    var arg2 = _platform.api2wire_String(message);
    return _platform.executeStream(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_start_signing(port_, arg0, arg1, arg2),
      parseSuccessData: _wire2api_coordinator_to_user_signing_message,
      parseErrorData: _wire2api_FrbAnyhowException,
      constMeta: kStartSigningConstMeta,
      argValues: [keyId, devices, message],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kStartSigningConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "start_signing",
        argNames: ["keyId", "devices", "message"],
      );

  Stream<CoordinatorToUserKeyGenMessage> generateNewKey(
      {required int threshold, required List<DeviceId> devices, dynamic hint}) {
    var arg0 = api2wire_usize(threshold);
    var arg1 = _platform.api2wire_list_device_id(devices);
    return _platform.executeStream(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_generate_new_key(port_, arg0, arg1),
      parseSuccessData: _wire2api_coordinator_to_user_key_gen_message,
      parseErrorData: null,
      constMeta: kGenerateNewKeyConstMeta,
      argValues: [threshold, devices],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kGenerateNewKeyConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "generate_new_key",
        argNames: ["threshold", "devices"],
      );

  int thresholdMethodFrostKey({required FrostKey that, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_frost_key(that);
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_threshold__method__FrostKey(arg0),
      parseSuccessData: _wire2api_usize,
      parseErrorData: null,
      constMeta: kThresholdMethodFrostKeyConstMeta,
      argValues: [that],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kThresholdMethodFrostKeyConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "threshold__method__FrostKey",
        argNames: ["that"],
      );

  KeyId idMethodFrostKey({required FrostKey that, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_frost_key(that);
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_id__method__FrostKey(arg0),
      parseSuccessData: _wire2api_key_id,
      parseErrorData: null,
      constMeta: kIdMethodFrostKeyConstMeta,
      argValues: [that],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kIdMethodFrostKeyConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "id__method__FrostKey",
        argNames: ["that"],
      );

  String nameMethodFrostKey({required FrostKey that, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_frost_key(that);
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_name__method__FrostKey(arg0),
      parseSuccessData: _wire2api_String,
      parseErrorData: null,
      constMeta: kNameMethodFrostKeyConstMeta,
      argValues: [that],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kNameMethodFrostKeyConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "name__method__FrostKey",
        argNames: ["that"],
      );

  List<Device> devicesMethodFrostKey({required FrostKey that, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_frost_key(that);
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () => _platform.inner.wire_devices__method__FrostKey(arg0),
      parseSuccessData: _wire2api_list_device,
      parseErrorData: null,
      constMeta: kDevicesMethodFrostKeyConstMeta,
      argValues: [that],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta get kDevicesMethodFrostKeyConstMeta =>
      const FlutterRustBridgeTaskConstMeta(
        debugName: "devices__method__FrostKey",
        argNames: ["that"],
      );

  Future<void> satisfyMethodPortOpen(
      {required PortOpen that, String? err, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_port_open(that);
    var arg1 = _platform.api2wire_opt_String(err);
    return _platform.executeNormal(FlutterRustBridgeTask(
      callFfi: (port_) =>
          _platform.inner.wire_satisfy__method__PortOpen(port_, arg0, arg1),
      parseSuccessData: _wire2api_unit,
      parseErrorData: null,
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
      parseErrorData: null,
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
      parseErrorData: null,
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
      parseErrorData: null,
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

  List<DeviceId> namedDevicesMethodDeviceListState(
      {required DeviceListState that, dynamic hint}) {
    var arg0 = _platform.api2wire_box_autoadd_device_list_state(that);
    return _platform.executeSync(FlutterRustBridgeSyncTask(
      callFfi: () =>
          _platform.inner.wire_named_devices__method__DeviceListState(arg0),
      parseSuccessData: _wire2api_list_device_id,
      parseErrorData: null,
      constMeta: kNamedDevicesMethodDeviceListStateConstMeta,
      argValues: [that],
      hint: hint,
    ));
  }

  FlutterRustBridgeTaskConstMeta
      get kNamedDevicesMethodDeviceListStateConstMeta =>
          const FlutterRustBridgeTaskConstMeta(
            debugName: "named_devices__method__DeviceListState",
            argNames: ["that"],
          );

  DropFnType get dropOpaqueFrostsnapCoreCoordinatorFrostKeyState =>
      _platform.inner.drop_opaque_FrostsnapCoreCoordinatorFrostKeyState;
  ShareFnType get shareOpaqueFrostsnapCoreCoordinatorFrostKeyState =>
      _platform.inner.share_opaque_FrostsnapCoreCoordinatorFrostKeyState;
  OpaqueTypeFinalizer get FrostsnapCoreCoordinatorFrostKeyStateFinalizer =>
      _platform.FrostsnapCoreCoordinatorFrostKeyStateFinalizer;

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

  FrbAnyhowException _wire2api_FrbAnyhowException(dynamic raw) {
    return FrbAnyhowException(raw as String);
  }

  FrostsnapCoreCoordinatorFrostKeyState
      _wire2api_FrostsnapCoreCoordinatorFrostKeyState(dynamic raw) {
    return FrostsnapCoreCoordinatorFrostKeyState.fromRaw(raw[0], raw[1], this);
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

  Device _wire2api_box_autoadd_device(dynamic raw) {
    return _wire2api_device(raw);
  }

  DeviceId _wire2api_box_autoadd_device_id(dynamic raw) {
    return _wire2api_device_id(raw);
  }

  FrostKey _wire2api_box_autoadd_frost_key(dynamic raw) {
    return _wire2api_frost_key(raw);
  }

  KeyId _wire2api_box_autoadd_key_id(dynamic raw) {
    return _wire2api_key_id(raw);
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

  CoordinatorToUserKeyGenMessage _wire2api_coordinator_to_user_key_gen_message(
      dynamic raw) {
    switch (raw[0]) {
      case 0:
        return CoordinatorToUserKeyGenMessage_ReceivedShares(
          from: _wire2api_box_autoadd_device_id(raw[1]),
        );
      case 1:
        return CoordinatorToUserKeyGenMessage_CheckKeyGen(
          sessionHash: _wire2api_u8_array_32(raw[1]),
        );
      case 2:
        return CoordinatorToUserKeyGenMessage_KeyGenAck(
          from: _wire2api_box_autoadd_device_id(raw[1]),
        );
      case 3:
        return CoordinatorToUserKeyGenMessage_FinishedKey(
          keyId: _wire2api_box_autoadd_key_id(raw[1]),
        );
      default:
        throw Exception("unreachable");
    }
  }

  CoordinatorToUserSigningMessage _wire2api_coordinator_to_user_signing_message(
      dynamic raw) {
    switch (raw[0]) {
      case 0:
        return CoordinatorToUserSigningMessage_GotShare(
          from: _wire2api_box_autoadd_device_id(raw[1]),
        );
      case 1:
        return CoordinatorToUserSigningMessage_Signed(
          signatures: _wire2api_list_encoded_signature(raw[1]),
        );
      default:
        throw Exception("unreachable");
    }
  }

  Device _wire2api_device(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 2)
      throw Exception('unexpected arr length: expect 2 but see ${arr.length}');
    return Device(
      name: _wire2api_opt_String(arr[0]),
      id: _wire2api_device_id(arr[1]),
    );
  }

  DeviceId _wire2api_device_id(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 1)
      throw Exception('unexpected arr length: expect 1 but see ${arr.length}');
    return DeviceId(
      field0: _wire2api_u8_array_33(arr[0]),
    );
  }

  DeviceListChange _wire2api_device_list_change(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 3)
      throw Exception('unexpected arr length: expect 3 but see ${arr.length}');
    return DeviceListChange(
      kind: _wire2api_device_list_change_kind(arr[0]),
      index: _wire2api_usize(arr[1]),
      device: _wire2api_device(arr[2]),
    );
  }

  DeviceListChangeKind _wire2api_device_list_change_kind(dynamic raw) {
    return DeviceListChangeKind.values[raw as int];
  }

  DeviceListState _wire2api_device_list_state(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 2)
      throw Exception('unexpected arr length: expect 2 but see ${arr.length}');
    return DeviceListState(
      bridge: this,
      devices: _wire2api_list_device(arr[0]),
      stateId: _wire2api_usize(arr[1]),
    );
  }

  DeviceListUpdate _wire2api_device_list_update(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 2)
      throw Exception('unexpected arr length: expect 2 but see ${arr.length}');
    return DeviceListUpdate(
      changes: _wire2api_list_device_list_change(arr[0]),
      state: _wire2api_device_list_state(arr[1]),
    );
  }

  EncodedSignature _wire2api_encoded_signature(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 1)
      throw Exception('unexpected arr length: expect 1 but see ${arr.length}');
    return EncodedSignature(
      field0: _wire2api_u8_array_64(arr[0]),
    );
  }

  FrostKey _wire2api_frost_key(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 1)
      throw Exception('unexpected arr length: expect 1 but see ${arr.length}');
    return FrostKey(
      bridge: this,
      field0: _wire2api_FrostsnapCoreCoordinatorFrostKeyState(arr[0]),
    );
  }

  int _wire2api_i32(dynamic raw) {
    return raw as int;
  }

  KeyId _wire2api_key_id(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 1)
      throw Exception('unexpected arr length: expect 1 but see ${arr.length}');
    return KeyId(
      field0: _wire2api_u8_array_32(arr[0]),
    );
  }

  KeyState _wire2api_key_state(dynamic raw) {
    final arr = raw as List<dynamic>;
    if (arr.length != 1)
      throw Exception('unexpected arr length: expect 1 but see ${arr.length}');
    return KeyState(
      keys: _wire2api_list_frost_key(arr[0]),
    );
  }

  List<Device> _wire2api_list_device(dynamic raw) {
    return (raw as List<dynamic>).map(_wire2api_device).toList();
  }

  List<DeviceId> _wire2api_list_device_id(dynamic raw) {
    return (raw as List<dynamic>).map(_wire2api_device_id).toList();
  }

  List<DeviceListChange> _wire2api_list_device_list_change(dynamic raw) {
    return (raw as List<dynamic>).map(_wire2api_device_list_change).toList();
  }

  List<EncodedSignature> _wire2api_list_encoded_signature(dynamic raw) {
    return (raw as List<dynamic>).map(_wire2api_encoded_signature).toList();
  }

  List<FrostKey> _wire2api_list_frost_key(dynamic raw) {
    return (raw as List<dynamic>).map(_wire2api_frost_key).toList();
  }

  String? _wire2api_opt_String(dynamic raw) {
    return raw == null ? null : _wire2api_String(raw);
  }

  Device? _wire2api_opt_box_autoadd_device(dynamic raw) {
    return raw == null ? null : _wire2api_box_autoadd_device(raw);
  }

  FrostKey? _wire2api_opt_box_autoadd_frost_key(dynamic raw) {
    return raw == null ? null : _wire2api_box_autoadd_frost_key(raw);
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

  PortEvent _wire2api_port_event(dynamic raw) {
    switch (raw[0]) {
      case 0:
        return PortEvent_Open(
          request: _wire2api_box_autoadd_port_open(raw[1]),
        );
      case 1:
        return PortEvent_Write(
          request: _wire2api_box_autoadd_port_write(raw[1]),
        );
      case 2:
        return PortEvent_Read(
          request: _wire2api_box_autoadd_port_read(raw[1]),
        );
      case 3:
        return PortEvent_BytesToRead(
          request: _wire2api_box_autoadd_port_bytes_to_read(raw[1]),
        );
      default:
        throw Exception("unreachable");
    }
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

  U8Array32 _wire2api_u8_array_32(dynamic raw) {
    return U8Array32(_wire2api_uint_8_list(raw));
  }

  U8Array33 _wire2api_u8_array_33(dynamic raw) {
    return U8Array33(_wire2api_uint_8_list(raw));
  }

  U8Array64 _wire2api_u8_array_64(dynamic raw) {
    return U8Array64(_wire2api_uint_8_list(raw));
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
