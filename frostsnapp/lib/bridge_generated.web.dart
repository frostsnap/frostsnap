// AUTO GENERATED FILE, DO NOT EDIT.
// Generated by `flutter_rust_bridge`@ 1.82.6.
// ignore_for_file: non_constant_identifier_names, unused_element, duplicate_ignore, directives_ordering, curly_braces_in_flow_control_structures, unnecessary_lambdas, slash_for_doc_comments, prefer_const_literals_to_create_immutables, implicit_dynamic_list_literal, duplicate_import, unused_import, unnecessary_import, prefer_single_quotes, prefer_const_constructors, use_super_parameters, always_use_package_imports, annotate_overrides, invalid_use_of_protected_member, constant_identifier_names, invalid_use_of_internal_member, prefer_is_empty, unnecessary_const

import "bridge_definitions.dart";
import 'dart:convert';
import 'dart:async';
import 'package:meta/meta.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge.dart';
import 'package:uuid/uuid.dart';
import 'bridge_generated.dart';
export 'bridge_generated.dart';

class NativePlatform extends FlutterRustBridgeBase<NativeWire>
    with FlutterRustBridgeSetupMixin {
  NativePlatform(FutureOr<WasmModule> dylib) : super(NativeWire(dylib)) {
    setupMixinConstructor();
  }
  Future<void> setup() => inner.init;

// Section: api2wire

  @protected
  Object api2wire_ArcMutexVecPortDesc(ArcMutexVecPortDesc raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_ChainSync(ChainSync raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_FfiCoordinator(FfiCoordinator raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_FrostsnapCoreCoordinatorFrostKey(
      FrostsnapCoreCoordinatorFrostKey raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_FrostsnapCoreMessageBitcoinTransactionSignTask(
      FrostsnapCoreMessageBitcoinTransactionSignTask raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_MutexBTreeMapKeyIdStreamSinkTxState(
      MutexBTreeMapKeyIdStreamSinkTxState raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_MutexCrateWalletWallet(MutexCrateWalletWallet raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_PortBytesToReadSender(PortBytesToReadSender raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_PortOpenSender(PortOpenSender raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_PortReadSender(PortReadSender raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_PortWriteSender(PortWriteSender raw) {
    return raw.shareOrMove();
  }

  @protected
  Object api2wire_RTransaction(RTransaction raw) {
    return raw.shareOrMove();
  }

  @protected
  String api2wire_String(String raw) {
    return raw;
  }

  @protected
  List<String> api2wire_StringList(List<String> raw) {
    return raw;
  }

  @protected
  List<dynamic> api2wire_box_autoadd_confirmation_time(ConfirmationTime raw) {
    return api2wire_confirmation_time(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_coordinator(Coordinator raw) {
    return api2wire_coordinator(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_device(Device raw) {
    return api2wire_device(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_device_id(DeviceId raw) {
    return api2wire_device_id(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_device_list_state(DeviceListState raw) {
    return api2wire_device_list_state(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_ffi_serial(FfiSerial raw) {
    return api2wire_ffi_serial(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_frost_key(FrostKey raw) {
    return api2wire_frost_key(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_key_id(KeyId raw) {
    return api2wire_key_id(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_port_bytes_to_read(PortBytesToRead raw) {
    return api2wire_port_bytes_to_read(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_port_open(PortOpen raw) {
    return api2wire_port_open(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_port_read(PortRead raw) {
    return api2wire_port_read(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_port_write(PortWrite raw) {
    return api2wire_port_write(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_signed_tx(SignedTx raw) {
    return api2wire_signed_tx(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_transaction(Transaction raw) {
    return api2wire_transaction(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_unsigned_tx(UnsignedTx raw) {
    return api2wire_unsigned_tx(raw);
  }

  @protected
  List<dynamic> api2wire_box_autoadd_wallet(Wallet raw) {
    return api2wire_wallet(raw);
  }

  @protected
  List<dynamic> api2wire_confirmation_time(ConfirmationTime raw) {
    return [api2wire_u32(raw.height), api2wire_u64(raw.time)];
  }

  @protected
  List<dynamic> api2wire_coordinator(Coordinator raw) {
    return [api2wire_FfiCoordinator(raw.field0)];
  }

  @protected
  List<dynamic> api2wire_device(Device raw) {
    return [
      api2wire_opt_String(raw.name),
      api2wire_String(raw.firmwareDigest),
      api2wire_String(raw.latestDigest),
      api2wire_device_id(raw.id)
    ];
  }

  @protected
  List<dynamic> api2wire_device_id(DeviceId raw) {
    return [api2wire_u8_array_33(raw.field0)];
  }

  @protected
  List<dynamic> api2wire_device_list_state(DeviceListState raw) {
    return [api2wire_list_device(raw.devices), api2wire_usize(raw.stateId)];
  }

  @protected
  List<dynamic> api2wire_encoded_signature(EncodedSignature raw) {
    return [api2wire_u8_array_64(raw.field0)];
  }

  @protected
  List<dynamic> api2wire_ffi_serial(FfiSerial raw) {
    return [api2wire_ArcMutexVecPortDesc(raw.availablePorts)];
  }

  @protected
  List<dynamic> api2wire_frost_key(FrostKey raw) {
    return [api2wire_FrostsnapCoreCoordinatorFrostKey(raw.field0)];
  }

  @protected
  Object api2wire_i64(int raw) {
    return castNativeBigInt(raw);
  }

  @protected
  List<dynamic> api2wire_key_id(KeyId raw) {
    return [api2wire_u8_array_32(raw.field0)];
  }

  @protected
  List<dynamic> api2wire_list_device(List<Device> raw) {
    return raw.map(api2wire_device).toList();
  }

  @protected
  List<dynamic> api2wire_list_device_id(List<DeviceId> raw) {
    return raw.map(api2wire_device_id).toList();
  }

  @protected
  List<dynamic> api2wire_list_encoded_signature(List<EncodedSignature> raw) {
    return raw.map(api2wire_encoded_signature).toList();
  }

  @protected
  List<dynamic> api2wire_list_port_desc(List<PortDesc> raw) {
    return raw.map(api2wire_port_desc).toList();
  }

  @protected
  String? api2wire_opt_String(String? raw) {
    return raw == null ? null : api2wire_String(raw);
  }

  @protected
  List<dynamic>? api2wire_opt_box_autoadd_confirmation_time(
      ConfirmationTime? raw) {
    return raw == null ? null : api2wire_box_autoadd_confirmation_time(raw);
  }

  @protected
  List<dynamic> api2wire_port_bytes_to_read(PortBytesToRead raw) {
    return [api2wire_String(raw.id), api2wire_PortBytesToReadSender(raw.ready)];
  }

  @protected
  List<dynamic> api2wire_port_desc(PortDesc raw) {
    return [
      api2wire_String(raw.id),
      api2wire_u16(raw.vid),
      api2wire_u16(raw.pid)
    ];
  }

  @protected
  List<dynamic> api2wire_port_open(PortOpen raw) {
    return [
      api2wire_String(raw.id),
      api2wire_u32(raw.baudRate),
      api2wire_PortOpenSender(raw.ready)
    ];
  }

  @protected
  List<dynamic> api2wire_port_read(PortRead raw) {
    return [
      api2wire_String(raw.id),
      api2wire_usize(raw.len),
      api2wire_PortReadSender(raw.ready)
    ];
  }

  @protected
  List<dynamic> api2wire_port_write(PortWrite raw) {
    return [
      api2wire_String(raw.id),
      api2wire_uint_8_list(raw.bytes),
      api2wire_PortWriteSender(raw.ready)
    ];
  }

  @protected
  List<dynamic> api2wire_signed_tx(SignedTx raw) {
    return [api2wire_RTransaction(raw.inner)];
  }

  @protected
  List<dynamic> api2wire_transaction(Transaction raw) {
    return [
      api2wire_i64(raw.netValue),
      api2wire_RTransaction(raw.inner),
      api2wire_opt_box_autoadd_confirmation_time(raw.confirmationTime)
    ];
  }

  @protected
  Object api2wire_u64(int raw) {
    return castNativeBigInt(raw);
  }

  @protected
  Uint8List api2wire_u8_array_32(U8Array32 raw) {
    return Uint8List.fromList(raw);
  }

  @protected
  Uint8List api2wire_u8_array_33(U8Array33 raw) {
    return Uint8List.fromList(raw);
  }

  @protected
  Uint8List api2wire_u8_array_64(U8Array64 raw) {
    return Uint8List.fromList(raw);
  }

  @protected
  Uint8List api2wire_uint_8_list(Uint8List raw) {
    return raw;
  }

  @protected
  List<dynamic> api2wire_unsigned_tx(UnsignedTx raw) {
    return [api2wire_FrostsnapCoreMessageBitcoinTransactionSignTask(raw.task)];
  }

  @protected
  List<dynamic> api2wire_wallet(Wallet raw) {
    return [
      api2wire_MutexCrateWalletWallet(raw.inner),
      api2wire_MutexBTreeMapKeyIdStreamSinkTxState(raw.walletStreams),
      api2wire_ChainSync(raw.chainSync)
    ];
  }
// Section: finalizer

  late final Finalizer<PlatformPointer> _ArcMutexVecPortDescFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_ArcMutexVecPortDesc);
  Finalizer<PlatformPointer> get ArcMutexVecPortDescFinalizer =>
      _ArcMutexVecPortDescFinalizer;
  late final Finalizer<PlatformPointer> _ChainSyncFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_ChainSync);
  Finalizer<PlatformPointer> get ChainSyncFinalizer => _ChainSyncFinalizer;
  late final Finalizer<PlatformPointer> _FfiCoordinatorFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_FfiCoordinator);
  Finalizer<PlatformPointer> get FfiCoordinatorFinalizer =>
      _FfiCoordinatorFinalizer;
  late final Finalizer<PlatformPointer>
      _FrostsnapCoreCoordinatorFrostKeyFinalizer = Finalizer<PlatformPointer>(
          inner.drop_opaque_FrostsnapCoreCoordinatorFrostKey);
  Finalizer<PlatformPointer> get FrostsnapCoreCoordinatorFrostKeyFinalizer =>
      _FrostsnapCoreCoordinatorFrostKeyFinalizer;
  late final Finalizer<PlatformPointer>
      _FrostsnapCoreMessageBitcoinTransactionSignTaskFinalizer =
      Finalizer<PlatformPointer>(
          inner.drop_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask);
  Finalizer<PlatformPointer>
      get FrostsnapCoreMessageBitcoinTransactionSignTaskFinalizer =>
          _FrostsnapCoreMessageBitcoinTransactionSignTaskFinalizer;
  late final Finalizer<PlatformPointer>
      _MutexBTreeMapKeyIdStreamSinkTxStateFinalizer =
      Finalizer<PlatformPointer>(
          inner.drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState);
  Finalizer<PlatformPointer> get MutexBTreeMapKeyIdStreamSinkTxStateFinalizer =>
      _MutexBTreeMapKeyIdStreamSinkTxStateFinalizer;
  late final Finalizer<PlatformPointer> _MutexCrateWalletWalletFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_MutexCrateWalletWallet);
  Finalizer<PlatformPointer> get MutexCrateWalletWalletFinalizer =>
      _MutexCrateWalletWalletFinalizer;
  late final Finalizer<PlatformPointer> _PortBytesToReadSenderFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_PortBytesToReadSender);
  Finalizer<PlatformPointer> get PortBytesToReadSenderFinalizer =>
      _PortBytesToReadSenderFinalizer;
  late final Finalizer<PlatformPointer> _PortOpenSenderFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_PortOpenSender);
  Finalizer<PlatformPointer> get PortOpenSenderFinalizer =>
      _PortOpenSenderFinalizer;
  late final Finalizer<PlatformPointer> _PortReadSenderFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_PortReadSender);
  Finalizer<PlatformPointer> get PortReadSenderFinalizer =>
      _PortReadSenderFinalizer;
  late final Finalizer<PlatformPointer> _PortWriteSenderFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_PortWriteSender);
  Finalizer<PlatformPointer> get PortWriteSenderFinalizer =>
      _PortWriteSenderFinalizer;
  late final Finalizer<PlatformPointer> _RTransactionFinalizer =
      Finalizer<PlatformPointer>(inner.drop_opaque_RTransaction);
  Finalizer<PlatformPointer> get RTransactionFinalizer =>
      _RTransactionFinalizer;
}

// Section: WASM wire module

@JS('wasm_bindgen')
external NativeWasmModule get wasmModule;

@JS()
@anonymous
class NativeWasmModule implements WasmModule {
  external Object /* Promise */ call([String? moduleName]);
  external NativeWasmModule bind(dynamic thisArg, String moduleName);
  external dynamic /* void */ wire_sub_port_events(NativePortType port_);

  external dynamic /* void */ wire_sub_device_events(NativePortType port_);

  external dynamic /* void */ wire_turn_stderr_logging_on(
      NativePortType port_, int level);

  external dynamic /* void */ wire_turn_logcat_logging_on(
      NativePortType port_, int _level);

  external dynamic /* List<dynamic>? */ wire_device_at_index(int index);

  external dynamic /* List<dynamic> */ wire_device_list_state();

  external dynamic /* List<dynamic> */ wire_get_device(List<dynamic> id);

  external dynamic /* void */ wire_load(NativePortType port_, String db_file);

  external dynamic /* void */ wire_load_host_handles_serial(
      NativePortType port_, String db_file);

  external dynamic /* void */ wire_echo_key_id(
      NativePortType port_, List<dynamic> key_id);

  external dynamic /* String */ wire_txid__method__Transaction(
      List<dynamic> that);

  external dynamic /* bool */ wire_ready__method__Device(List<dynamic> that);

  external dynamic /* bool */ wire_needs_firmware_upgrade__method__Device(
      List<dynamic> that);

  external dynamic /* int */ wire_threshold__method__FrostKey(
      List<dynamic> that);

  external dynamic /* List<dynamic> */ wire_id__method__FrostKey(
      List<dynamic> that);

  external dynamic /* String */ wire_name__method__FrostKey(List<dynamic> that);

  external dynamic /* List<dynamic> */ wire_devices__method__FrostKey(
      List<dynamic> that);

  external dynamic /* void */ wire_satisfy__method__PortOpen(
      NativePortType port_, List<dynamic> that, String? err);

  external dynamic /* void */ wire_satisfy__method__PortRead(
      NativePortType port_, List<dynamic> that, Uint8List bytes, String? err);

  external dynamic /* void */ wire_satisfy__method__PortWrite(
      NativePortType port_, List<dynamic> that, String? err);

  external dynamic /* void */ wire_satisfy__method__PortBytesToRead(
      NativePortType port_, List<dynamic> that, int bytes_to_read);

  external dynamic /* List<dynamic>? */
      wire_get_device__method__DeviceListState(
          List<dynamic> that, List<dynamic> id);

  external dynamic /* void */ wire_set_available_ports__method__FfiSerial(
      NativePortType port_, List<dynamic> that, List<dynamic> ports);

  external dynamic /* void */ wire_start_thread__method__Coordinator(
      NativePortType port_, List<dynamic> that);

  external dynamic /* void */ wire_update_name_preview__method__Coordinator(
      NativePortType port_, List<dynamic> that, List<dynamic> id, String name);

  external dynamic /* void */ wire_finish_naming__method__Coordinator(
      NativePortType port_, List<dynamic> that, List<dynamic> id, String name);

  external dynamic /* void */ wire_send_cancel__method__Coordinator(
      NativePortType port_, List<dynamic> that, List<dynamic> id);

  external dynamic /* void */ wire_cancel_all__method__Coordinator(
      NativePortType port_, List<dynamic> that);

  external dynamic /* void */ wire_display_backup__method__Coordinator(
      NativePortType port_,
      List<dynamic> that,
      List<dynamic> id,
      List<dynamic> key_id);

  external dynamic /* List<dynamic> */ wire_key_state__method__Coordinator(
      List<dynamic> that);

  external dynamic /* void */ wire_sub_key_events__method__Coordinator(
      NativePortType port_, List<dynamic> that);

  external dynamic /* List<dynamic>? */ wire_get_key__method__Coordinator(
      List<dynamic> that, List<dynamic> key_id);

  external dynamic /* List<dynamic> */
      wire_keys_for_device__method__Coordinator(
          List<dynamic> that, List<dynamic> device_id);

  external dynamic /* void */ wire_start_signing__method__Coordinator(
      NativePortType port_,
      List<dynamic> that,
      List<dynamic> key_id,
      List<dynamic> devices,
      String message);

  external dynamic /* void */ wire_start_signing_tx__method__Coordinator(
      NativePortType port_,
      List<dynamic> that,
      List<dynamic> key_id,
      List<dynamic> unsigned_tx,
      List<dynamic> devices);

  external dynamic /* int */ wire_nonces_available__method__Coordinator(
      List<dynamic> that, List<dynamic> id);

  external dynamic /* void */ wire_generate_new_key__method__Coordinator(
      NativePortType port_,
      List<dynamic> that,
      int threshold,
      List<dynamic> devices);

  external dynamic /* List<dynamic>? */
      wire_persisted_sign_session_description__method__Coordinator(
          List<dynamic> that, List<dynamic> key_id);

  external dynamic /* void */
      wire_try_restore_signing_session__method__Coordinator(
          NativePortType port_, List<dynamic> that, List<dynamic> key_id);

  external dynamic /* void */ wire_start_firmware_upgrade__method__Coordinator(
      NativePortType port_, List<dynamic> that);

  external dynamic /* String */
      wire_upgrade_firmware_digest__method__Coordinator(List<dynamic> that);

  external dynamic /* void */ wire_cancel_protocol__method__Coordinator(
      NativePortType port_, List<dynamic> that);

  external dynamic /* void */
      wire_enter_firmware_upgrade_mode__method__Coordinator(
          NativePortType port_, List<dynamic> that);

  external dynamic /* void */ wire_sub_tx_state__method__Wallet(
      NativePortType port_, List<dynamic> that, List<dynamic> key_id);

  external dynamic /* List<dynamic> */ wire_tx_state__method__Wallet(
      List<dynamic> that, List<dynamic> key_id);

  external dynamic /* void */ wire_sync_txids__method__Wallet(
      NativePortType port_,
      List<dynamic> that,
      List<dynamic> key_id,
      List<String> txids);

  external dynamic /* void */ wire_sync__method__Wallet(
      NativePortType port_, List<dynamic> that, List<dynamic> key_id);

  external dynamic /* void */ wire_next_address__method__Wallet(
      NativePortType port_, List<dynamic> that, List<dynamic> key_id);

  external dynamic /* List<dynamic> */ wire_addresses_state__method__Wallet(
      List<dynamic> that, List<dynamic> key_id);

  external dynamic /* String? */
      wire_validate_destination_address__method__Wallet(
          List<dynamic> that, String address);

  external dynamic /* String? */ wire_validate_amount__method__Wallet(
      List<dynamic> that, String address, Object value);

  external dynamic /* void */ wire_send_to__method__Wallet(
      NativePortType port_,
      List<dynamic> that,
      List<dynamic> key_id,
      String to_address,
      Object value,
      double feerate);

  external dynamic /* List<dynamic> */
      wire_complete_unsigned_tx__method__Wallet(List<dynamic> that,
          List<dynamic> unsigned_tx, List<dynamic> signatures);

  external dynamic /* void */ wire_broadcast_tx__method__Wallet(
      NativePortType port_,
      List<dynamic> that,
      List<dynamic> key_id,
      List<dynamic> tx);

  external dynamic /* List<dynamic> */ wire_effect_of_tx__method__Wallet(
      List<dynamic> that, List<dynamic> key_id, Object tx);

  external dynamic /* Object */ wire_tx__method__SignedTx(List<dynamic> that);

  external dynamic /* Object */ wire_tx__method__UnsignedTx(List<dynamic> that);

  external dynamic /*  */ drop_opaque_ArcMutexVecPortDesc(ptr);

  external int /* *const c_void */ share_opaque_ArcMutexVecPortDesc(ptr);

  external dynamic /*  */ drop_opaque_ChainSync(ptr);

  external int /* *const c_void */ share_opaque_ChainSync(ptr);

  external dynamic /*  */ drop_opaque_FfiCoordinator(ptr);

  external int /* *const c_void */ share_opaque_FfiCoordinator(ptr);

  external dynamic /*  */ drop_opaque_FrostsnapCoreCoordinatorFrostKey(ptr);

  external int /* *const c_void */
      share_opaque_FrostsnapCoreCoordinatorFrostKey(ptr);

  external dynamic /*  */
      drop_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(ptr);

  external int /* *const c_void */
      share_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(ptr);

  external dynamic /*  */ drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr);

  external int /* *const c_void */
      share_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr);

  external dynamic /*  */ drop_opaque_MutexCrateWalletWallet(ptr);

  external int /* *const c_void */ share_opaque_MutexCrateWalletWallet(ptr);

  external dynamic /*  */ drop_opaque_PortBytesToReadSender(ptr);

  external int /* *const c_void */ share_opaque_PortBytesToReadSender(ptr);

  external dynamic /*  */ drop_opaque_PortOpenSender(ptr);

  external int /* *const c_void */ share_opaque_PortOpenSender(ptr);

  external dynamic /*  */ drop_opaque_PortReadSender(ptr);

  external int /* *const c_void */ share_opaque_PortReadSender(ptr);

  external dynamic /*  */ drop_opaque_PortWriteSender(ptr);

  external int /* *const c_void */ share_opaque_PortWriteSender(ptr);

  external dynamic /*  */ drop_opaque_RTransaction(ptr);

  external int /* *const c_void */ share_opaque_RTransaction(ptr);
}

// Section: WASM wire connector

class NativeWire extends FlutterRustBridgeWasmWireBase<NativeWasmModule> {
  NativeWire(FutureOr<WasmModule> module)
      : super(WasmModule.cast<NativeWasmModule>(module));

  void wire_sub_port_events(NativePortType port_) =>
      wasmModule.wire_sub_port_events(port_);

  void wire_sub_device_events(NativePortType port_) =>
      wasmModule.wire_sub_device_events(port_);

  void wire_turn_stderr_logging_on(NativePortType port_, int level) =>
      wasmModule.wire_turn_stderr_logging_on(port_, level);

  void wire_turn_logcat_logging_on(NativePortType port_, int _level) =>
      wasmModule.wire_turn_logcat_logging_on(port_, _level);

  dynamic /* List<dynamic>? */ wire_device_at_index(int index) =>
      wasmModule.wire_device_at_index(index);

  dynamic /* List<dynamic> */ wire_device_list_state() =>
      wasmModule.wire_device_list_state();

  dynamic /* List<dynamic> */ wire_get_device(List<dynamic> id) =>
      wasmModule.wire_get_device(id);

  void wire_load(NativePortType port_, String db_file) =>
      wasmModule.wire_load(port_, db_file);

  void wire_load_host_handles_serial(NativePortType port_, String db_file) =>
      wasmModule.wire_load_host_handles_serial(port_, db_file);

  void wire_echo_key_id(NativePortType port_, List<dynamic> key_id) =>
      wasmModule.wire_echo_key_id(port_, key_id);

  dynamic /* String */ wire_txid__method__Transaction(List<dynamic> that) =>
      wasmModule.wire_txid__method__Transaction(that);

  dynamic /* bool */ wire_ready__method__Device(List<dynamic> that) =>
      wasmModule.wire_ready__method__Device(that);

  dynamic /* bool */ wire_needs_firmware_upgrade__method__Device(
          List<dynamic> that) =>
      wasmModule.wire_needs_firmware_upgrade__method__Device(that);

  dynamic /* int */ wire_threshold__method__FrostKey(List<dynamic> that) =>
      wasmModule.wire_threshold__method__FrostKey(that);

  dynamic /* List<dynamic> */ wire_id__method__FrostKey(List<dynamic> that) =>
      wasmModule.wire_id__method__FrostKey(that);

  dynamic /* String */ wire_name__method__FrostKey(List<dynamic> that) =>
      wasmModule.wire_name__method__FrostKey(that);

  dynamic /* List<dynamic> */ wire_devices__method__FrostKey(
          List<dynamic> that) =>
      wasmModule.wire_devices__method__FrostKey(that);

  void wire_satisfy__method__PortOpen(
          NativePortType port_, List<dynamic> that, String? err) =>
      wasmModule.wire_satisfy__method__PortOpen(port_, that, err);

  void wire_satisfy__method__PortRead(NativePortType port_, List<dynamic> that,
          Uint8List bytes, String? err) =>
      wasmModule.wire_satisfy__method__PortRead(port_, that, bytes, err);

  void wire_satisfy__method__PortWrite(
          NativePortType port_, List<dynamic> that, String? err) =>
      wasmModule.wire_satisfy__method__PortWrite(port_, that, err);

  void wire_satisfy__method__PortBytesToRead(
          NativePortType port_, List<dynamic> that, int bytes_to_read) =>
      wasmModule.wire_satisfy__method__PortBytesToRead(
          port_, that, bytes_to_read);

  dynamic /* List<dynamic>? */ wire_get_device__method__DeviceListState(
          List<dynamic> that, List<dynamic> id) =>
      wasmModule.wire_get_device__method__DeviceListState(that, id);

  void wire_set_available_ports__method__FfiSerial(
          NativePortType port_, List<dynamic> that, List<dynamic> ports) =>
      wasmModule.wire_set_available_ports__method__FfiSerial(
          port_, that, ports);

  void wire_start_thread__method__Coordinator(
          NativePortType port_, List<dynamic> that) =>
      wasmModule.wire_start_thread__method__Coordinator(port_, that);

  void wire_update_name_preview__method__Coordinator(NativePortType port_,
          List<dynamic> that, List<dynamic> id, String name) =>
      wasmModule.wire_update_name_preview__method__Coordinator(
          port_, that, id, name);

  void wire_finish_naming__method__Coordinator(NativePortType port_,
          List<dynamic> that, List<dynamic> id, String name) =>
      wasmModule.wire_finish_naming__method__Coordinator(port_, that, id, name);

  void wire_send_cancel__method__Coordinator(
          NativePortType port_, List<dynamic> that, List<dynamic> id) =>
      wasmModule.wire_send_cancel__method__Coordinator(port_, that, id);

  void wire_cancel_all__method__Coordinator(
          NativePortType port_, List<dynamic> that) =>
      wasmModule.wire_cancel_all__method__Coordinator(port_, that);

  void wire_display_backup__method__Coordinator(NativePortType port_,
          List<dynamic> that, List<dynamic> id, List<dynamic> key_id) =>
      wasmModule.wire_display_backup__method__Coordinator(
          port_, that, id, key_id);

  dynamic /* List<dynamic> */ wire_key_state__method__Coordinator(
          List<dynamic> that) =>
      wasmModule.wire_key_state__method__Coordinator(that);

  void wire_sub_key_events__method__Coordinator(
          NativePortType port_, List<dynamic> that) =>
      wasmModule.wire_sub_key_events__method__Coordinator(port_, that);

  dynamic /* List<dynamic>? */ wire_get_key__method__Coordinator(
          List<dynamic> that, List<dynamic> key_id) =>
      wasmModule.wire_get_key__method__Coordinator(that, key_id);

  dynamic /* List<dynamic> */ wire_keys_for_device__method__Coordinator(
          List<dynamic> that, List<dynamic> device_id) =>
      wasmModule.wire_keys_for_device__method__Coordinator(that, device_id);

  void wire_start_signing__method__Coordinator(
          NativePortType port_,
          List<dynamic> that,
          List<dynamic> key_id,
          List<dynamic> devices,
          String message) =>
      wasmModule.wire_start_signing__method__Coordinator(
          port_, that, key_id, devices, message);

  void wire_start_signing_tx__method__Coordinator(
          NativePortType port_,
          List<dynamic> that,
          List<dynamic> key_id,
          List<dynamic> unsigned_tx,
          List<dynamic> devices) =>
      wasmModule.wire_start_signing_tx__method__Coordinator(
          port_, that, key_id, unsigned_tx, devices);

  dynamic /* int */ wire_nonces_available__method__Coordinator(
          List<dynamic> that, List<dynamic> id) =>
      wasmModule.wire_nonces_available__method__Coordinator(that, id);

  void wire_generate_new_key__method__Coordinator(NativePortType port_,
          List<dynamic> that, int threshold, List<dynamic> devices) =>
      wasmModule.wire_generate_new_key__method__Coordinator(
          port_, that, threshold, devices);

  dynamic /* List<dynamic>? */
      wire_persisted_sign_session_description__method__Coordinator(
              List<dynamic> that, List<dynamic> key_id) =>
          wasmModule
              .wire_persisted_sign_session_description__method__Coordinator(
                  that, key_id);

  void wire_try_restore_signing_session__method__Coordinator(
          NativePortType port_, List<dynamic> that, List<dynamic> key_id) =>
      wasmModule.wire_try_restore_signing_session__method__Coordinator(
          port_, that, key_id);

  void wire_start_firmware_upgrade__method__Coordinator(
          NativePortType port_, List<dynamic> that) =>
      wasmModule.wire_start_firmware_upgrade__method__Coordinator(port_, that);

  dynamic /* String */ wire_upgrade_firmware_digest__method__Coordinator(
          List<dynamic> that) =>
      wasmModule.wire_upgrade_firmware_digest__method__Coordinator(that);

  void wire_cancel_protocol__method__Coordinator(
          NativePortType port_, List<dynamic> that) =>
      wasmModule.wire_cancel_protocol__method__Coordinator(port_, that);

  void wire_enter_firmware_upgrade_mode__method__Coordinator(
          NativePortType port_, List<dynamic> that) =>
      wasmModule.wire_enter_firmware_upgrade_mode__method__Coordinator(
          port_, that);

  void wire_sub_tx_state__method__Wallet(
          NativePortType port_, List<dynamic> that, List<dynamic> key_id) =>
      wasmModule.wire_sub_tx_state__method__Wallet(port_, that, key_id);

  dynamic /* List<dynamic> */ wire_tx_state__method__Wallet(
          List<dynamic> that, List<dynamic> key_id) =>
      wasmModule.wire_tx_state__method__Wallet(that, key_id);

  void wire_sync_txids__method__Wallet(NativePortType port_, List<dynamic> that,
          List<dynamic> key_id, List<String> txids) =>
      wasmModule.wire_sync_txids__method__Wallet(port_, that, key_id, txids);

  void wire_sync__method__Wallet(
          NativePortType port_, List<dynamic> that, List<dynamic> key_id) =>
      wasmModule.wire_sync__method__Wallet(port_, that, key_id);

  void wire_next_address__method__Wallet(
          NativePortType port_, List<dynamic> that, List<dynamic> key_id) =>
      wasmModule.wire_next_address__method__Wallet(port_, that, key_id);

  dynamic /* List<dynamic> */ wire_addresses_state__method__Wallet(
          List<dynamic> that, List<dynamic> key_id) =>
      wasmModule.wire_addresses_state__method__Wallet(that, key_id);

  dynamic /* String? */ wire_validate_destination_address__method__Wallet(
          List<dynamic> that, String address) =>
      wasmModule.wire_validate_destination_address__method__Wallet(
          that, address);

  dynamic /* String? */ wire_validate_amount__method__Wallet(
          List<dynamic> that, String address, Object value) =>
      wasmModule.wire_validate_amount__method__Wallet(that, address, value);

  void wire_send_to__method__Wallet(
          NativePortType port_,
          List<dynamic> that,
          List<dynamic> key_id,
          String to_address,
          Object value,
          double feerate) =>
      wasmModule.wire_send_to__method__Wallet(
          port_, that, key_id, to_address, value, feerate);

  dynamic /* List<dynamic> */ wire_complete_unsigned_tx__method__Wallet(
          List<dynamic> that,
          List<dynamic> unsigned_tx,
          List<dynamic> signatures) =>
      wasmModule.wire_complete_unsigned_tx__method__Wallet(
          that, unsigned_tx, signatures);

  void wire_broadcast_tx__method__Wallet(NativePortType port_,
          List<dynamic> that, List<dynamic> key_id, List<dynamic> tx) =>
      wasmModule.wire_broadcast_tx__method__Wallet(port_, that, key_id, tx);

  dynamic /* List<dynamic> */ wire_effect_of_tx__method__Wallet(
          List<dynamic> that, List<dynamic> key_id, Object tx) =>
      wasmModule.wire_effect_of_tx__method__Wallet(that, key_id, tx);

  dynamic /* Object */ wire_tx__method__SignedTx(List<dynamic> that) =>
      wasmModule.wire_tx__method__SignedTx(that);

  dynamic /* Object */ wire_tx__method__UnsignedTx(List<dynamic> that) =>
      wasmModule.wire_tx__method__UnsignedTx(that);

  dynamic /*  */ drop_opaque_ArcMutexVecPortDesc(ptr) =>
      wasmModule.drop_opaque_ArcMutexVecPortDesc(ptr);

  int /* *const c_void */ share_opaque_ArcMutexVecPortDesc(ptr) =>
      wasmModule.share_opaque_ArcMutexVecPortDesc(ptr);

  dynamic /*  */ drop_opaque_ChainSync(ptr) =>
      wasmModule.drop_opaque_ChainSync(ptr);

  int /* *const c_void */ share_opaque_ChainSync(ptr) =>
      wasmModule.share_opaque_ChainSync(ptr);

  dynamic /*  */ drop_opaque_FfiCoordinator(ptr) =>
      wasmModule.drop_opaque_FfiCoordinator(ptr);

  int /* *const c_void */ share_opaque_FfiCoordinator(ptr) =>
      wasmModule.share_opaque_FfiCoordinator(ptr);

  dynamic /*  */ drop_opaque_FrostsnapCoreCoordinatorFrostKey(ptr) =>
      wasmModule.drop_opaque_FrostsnapCoreCoordinatorFrostKey(ptr);

  int /* *const c_void */ share_opaque_FrostsnapCoreCoordinatorFrostKey(ptr) =>
      wasmModule.share_opaque_FrostsnapCoreCoordinatorFrostKey(ptr);

  dynamic /*  */ drop_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(
          ptr) =>
      wasmModule
          .drop_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(ptr);

  int /* *const c_void */
      share_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(ptr) =>
          wasmModule
              .share_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(ptr);

  dynamic /*  */ drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr) =>
      wasmModule.drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr);

  int /* *const c_void */ share_opaque_MutexBTreeMapKeyIdStreamSinkTxState(
          ptr) =>
      wasmModule.share_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr);

  dynamic /*  */ drop_opaque_MutexCrateWalletWallet(ptr) =>
      wasmModule.drop_opaque_MutexCrateWalletWallet(ptr);

  int /* *const c_void */ share_opaque_MutexCrateWalletWallet(ptr) =>
      wasmModule.share_opaque_MutexCrateWalletWallet(ptr);

  dynamic /*  */ drop_opaque_PortBytesToReadSender(ptr) =>
      wasmModule.drop_opaque_PortBytesToReadSender(ptr);

  int /* *const c_void */ share_opaque_PortBytesToReadSender(ptr) =>
      wasmModule.share_opaque_PortBytesToReadSender(ptr);

  dynamic /*  */ drop_opaque_PortOpenSender(ptr) =>
      wasmModule.drop_opaque_PortOpenSender(ptr);

  int /* *const c_void */ share_opaque_PortOpenSender(ptr) =>
      wasmModule.share_opaque_PortOpenSender(ptr);

  dynamic /*  */ drop_opaque_PortReadSender(ptr) =>
      wasmModule.drop_opaque_PortReadSender(ptr);

  int /* *const c_void */ share_opaque_PortReadSender(ptr) =>
      wasmModule.share_opaque_PortReadSender(ptr);

  dynamic /*  */ drop_opaque_PortWriteSender(ptr) =>
      wasmModule.drop_opaque_PortWriteSender(ptr);

  int /* *const c_void */ share_opaque_PortWriteSender(ptr) =>
      wasmModule.share_opaque_PortWriteSender(ptr);

  dynamic /*  */ drop_opaque_RTransaction(ptr) =>
      wasmModule.drop_opaque_RTransaction(ptr);

  int /* *const c_void */ share_opaque_RTransaction(ptr) =>
      wasmModule.share_opaque_RTransaction(ptr);
}
