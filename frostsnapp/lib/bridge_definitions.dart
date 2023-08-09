// AUTO GENERATED FILE, DO NOT EDIT.
// Generated by `flutter_rust_bridge`@ 1.79.0.
// ignore_for_file: non_constant_identifier_names, unused_element, duplicate_ignore, directives_ordering, curly_braces_in_flow_control_structures, unnecessary_lambdas, slash_for_doc_comments, prefer_const_literals_to_create_immutables, implicit_dynamic_list_literal, duplicate_import, unused_import, unnecessary_import, prefer_single_quotes, prefer_const_constructors, use_super_parameters, always_use_package_imports, annotate_overrides, invalid_use_of_protected_member, constant_identifier_names, invalid_use_of_internal_member, prefer_is_empty, unnecessary_const

import 'bridge_generated.io.dart'
    if (dart.library.html) 'bridge_generated.web.dart';
import 'dart:convert';
import 'dart:async';
import 'package:meta/meta.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:uuid/uuid.dart';
import 'package:freezed_annotation/freezed_annotation.dart' hide protected;

part 'bridge_definitions.freezed.dart';

abstract class Native {
  Stream<CoordinatorEvent> initEvents({dynamic hint});

  FlutterRustBridgeTaskConstMeta get kInitEventsConstMeta;

  Stream<List<DeviceChange>> subDeviceEvents({dynamic hint});

  FlutterRustBridgeTaskConstMeta get kSubDeviceEventsConstMeta;

  Future<FfiCoordinator> newFfiCoordinator(
      {required bool hostHandlesSerial, dynamic hint});

  FlutterRustBridgeTaskConstMeta get kNewFfiCoordinatorConstMeta;

  Future<void> turnStderrLoggingOn({required Level level, dynamic hint});

  FlutterRustBridgeTaskConstMeta get kTurnStderrLoggingOnConstMeta;

  Future<void> turnLogcatLoggingOn({required Level level, dynamic hint});

  FlutterRustBridgeTaskConstMeta get kTurnLogcatLoggingOnConstMeta;

  Future<void> announceAvailablePorts(
      {required FfiCoordinator coordinator,
      required List<PortDesc> ports,
      dynamic hint});

  FlutterRustBridgeTaskConstMeta get kAnnounceAvailablePortsConstMeta;

  Future<void> setDeviceLabel(
      {required FfiCoordinator coordinator,
      required String deviceId,
      required String label,
      dynamic hint});

  FlutterRustBridgeTaskConstMeta get kSetDeviceLabelConstMeta;

  Future<void> satisfyMethodPortOpen(
      {required PortOpen that, String? err, dynamic hint});

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortOpenConstMeta;

  Future<void> satisfyMethodPortRead(
      {required PortRead that,
      required Uint8List bytes,
      String? err,
      dynamic hint});

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortReadConstMeta;

  Future<void> satisfyMethodPortWrite(
      {required PortWrite that, String? err, dynamic hint});

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortWriteConstMeta;

  Future<void> satisfyMethodPortBytesToRead(
      {required PortBytesToRead that, required int bytesToRead, dynamic hint});

  FlutterRustBridgeTaskConstMeta get kSatisfyMethodPortBytesToReadConstMeta;

  DropFnType get dropOpaqueFfiCoordinator;
  ShareFnType get shareOpaqueFfiCoordinator;
  OpaqueTypeFinalizer get FfiCoordinatorFinalizer;

  DropFnType get dropOpaquePortBytesToReadSender;
  ShareFnType get shareOpaquePortBytesToReadSender;
  OpaqueTypeFinalizer get PortBytesToReadSenderFinalizer;

  DropFnType get dropOpaquePortOpenSender;
  ShareFnType get shareOpaquePortOpenSender;
  OpaqueTypeFinalizer get PortOpenSenderFinalizer;

  DropFnType get dropOpaquePortReadSender;
  ShareFnType get shareOpaquePortReadSender;
  OpaqueTypeFinalizer get PortReadSenderFinalizer;

  DropFnType get dropOpaquePortWriteSender;
  ShareFnType get shareOpaquePortWriteSender;
  OpaqueTypeFinalizer get PortWriteSenderFinalizer;
}

@sealed
class FfiCoordinator extends FrbOpaque {
  final Native bridge;
  FfiCoordinator.fromRaw(int ptr, int size, this.bridge)
      : super.unsafe(ptr, size);
  @override
  DropFnType get dropFn => bridge.dropOpaqueFfiCoordinator;

  @override
  ShareFnType get shareFn => bridge.shareOpaqueFfiCoordinator;

  @override
  OpaqueTypeFinalizer get staticFinalizer => bridge.FfiCoordinatorFinalizer;
}

@sealed
class PortBytesToReadSender extends FrbOpaque {
  final Native bridge;
  PortBytesToReadSender.fromRaw(int ptr, int size, this.bridge)
      : super.unsafe(ptr, size);
  @override
  DropFnType get dropFn => bridge.dropOpaquePortBytesToReadSender;

  @override
  ShareFnType get shareFn => bridge.shareOpaquePortBytesToReadSender;

  @override
  OpaqueTypeFinalizer get staticFinalizer =>
      bridge.PortBytesToReadSenderFinalizer;
}

@sealed
class PortOpenSender extends FrbOpaque {
  final Native bridge;
  PortOpenSender.fromRaw(int ptr, int size, this.bridge)
      : super.unsafe(ptr, size);
  @override
  DropFnType get dropFn => bridge.dropOpaquePortOpenSender;

  @override
  ShareFnType get shareFn => bridge.shareOpaquePortOpenSender;

  @override
  OpaqueTypeFinalizer get staticFinalizer => bridge.PortOpenSenderFinalizer;
}

@sealed
class PortReadSender extends FrbOpaque {
  final Native bridge;
  PortReadSender.fromRaw(int ptr, int size, this.bridge)
      : super.unsafe(ptr, size);
  @override
  DropFnType get dropFn => bridge.dropOpaquePortReadSender;

  @override
  ShareFnType get shareFn => bridge.shareOpaquePortReadSender;

  @override
  OpaqueTypeFinalizer get staticFinalizer => bridge.PortReadSenderFinalizer;
}

@sealed
class PortWriteSender extends FrbOpaque {
  final Native bridge;
  PortWriteSender.fromRaw(int ptr, int size, this.bridge)
      : super.unsafe(ptr, size);
  @override
  DropFnType get dropFn => bridge.dropOpaquePortWriteSender;

  @override
  ShareFnType get shareFn => bridge.shareOpaquePortWriteSender;

  @override
  OpaqueTypeFinalizer get staticFinalizer => bridge.PortWriteSenderFinalizer;
}

@freezed
sealed class CoordinatorEvent with _$CoordinatorEvent {
  const factory CoordinatorEvent.portOpen({
    required PortOpen request,
  }) = CoordinatorEvent_PortOpen;
  const factory CoordinatorEvent.portWrite({
    required PortWrite request,
  }) = CoordinatorEvent_PortWrite;
  const factory CoordinatorEvent.portRead({
    required PortRead request,
  }) = CoordinatorEvent_PortRead;
  const factory CoordinatorEvent.portBytesToRead({
    required PortBytesToRead request,
  }) = CoordinatorEvent_PortBytesToRead;
}

@freezed
sealed class DeviceChange with _$DeviceChange {
  const factory DeviceChange.added({
    required String id,
  }) = DeviceChange_Added;
  const factory DeviceChange.registered({
    required String id,
    required String label,
  }) = DeviceChange_Registered;
  const factory DeviceChange.disconnected({
    required String id,
  }) = DeviceChange_Disconnected;
}

enum Level {
  Debug,
  Info,
}

class PortBytesToRead {
  final Native bridge;
  final String id;
  final PortBytesToReadSender ready;

  const PortBytesToRead({
    required this.bridge,
    required this.id,
    required this.ready,
  });

  Future<void> satisfy({required int bytesToRead, dynamic hint}) =>
      bridge.satisfyMethodPortBytesToRead(
        that: this,
        bytesToRead: bytesToRead,
      );
}

class PortDesc {
  final String id;
  final int vid;
  final int pid;

  const PortDesc({
    required this.id,
    required this.vid,
    required this.pid,
  });
}

class PortOpen {
  final Native bridge;
  final String id;
  final int baudRate;
  final PortOpenSender ready;

  const PortOpen({
    required this.bridge,
    required this.id,
    required this.baudRate,
    required this.ready,
  });

  Future<void> satisfy({String? err, dynamic hint}) =>
      bridge.satisfyMethodPortOpen(
        that: this,
        err: err,
      );
}

class PortRead {
  final Native bridge;
  final String id;
  final int len;
  final PortReadSender ready;

  const PortRead({
    required this.bridge,
    required this.id,
    required this.len,
    required this.ready,
  });

  Future<void> satisfy({required Uint8List bytes, String? err, dynamic hint}) =>
      bridge.satisfyMethodPortRead(
        that: this,
        bytes: bytes,
        err: err,
      );
}

class PortWrite {
  final Native bridge;
  final String id;
  final Uint8List bytes;
  final PortWriteSender ready;

  const PortWrite({
    required this.bridge,
    required this.id,
    required this.bytes,
    required this.ready,
  });

  Future<void> satisfy({String? err, dynamic hint}) =>
      bridge.satisfyMethodPortWrite(
        that: this,
        err: err,
      );
}
