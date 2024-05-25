use super::*;
// Section: wire functions

#[no_mangle]
pub extern "C" fn wire_sub_port_events(port_: i64) {
    wire_sub_port_events_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_sub_device_events(port_: i64) {
    wire_sub_device_events_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_turn_stderr_logging_on(port_: i64, level: i32) {
    wire_turn_stderr_logging_on_impl(port_, level)
}

#[no_mangle]
pub extern "C" fn wire_turn_logcat_logging_on(port_: i64, _level: i32) {
    wire_turn_logcat_logging_on_impl(port_, _level)
}

#[no_mangle]
pub extern "C" fn wire_device_at_index(index: usize) -> support::WireSyncReturn {
    wire_device_at_index_impl(index)
}

#[no_mangle]
pub extern "C" fn wire_device_list_state() -> support::WireSyncReturn {
    wire_device_list_state_impl()
}

#[no_mangle]
pub extern "C" fn wire_get_connected_device(id: *mut wire_DeviceId) -> support::WireSyncReturn {
    wire_get_connected_device_impl(id)
}

#[no_mangle]
pub extern "C" fn wire_load(port_: i64, db_file: *mut wire_uint_8_list) {
    wire_load_impl(port_, db_file)
}

#[no_mangle]
pub extern "C" fn wire_load_host_handles_serial(port_: i64, db_file: *mut wire_uint_8_list) {
    wire_load_host_handles_serial_impl(port_, db_file)
}

#[no_mangle]
pub extern "C" fn wire_echo_key_id(port_: i64, key_id: *mut wire_KeyId) {
    wire_echo_key_id_impl(port_, key_id)
}

#[no_mangle]
pub extern "C" fn wire_psbt_bytes_to_psbt(
    psbt_bytes: *mut wire_uint_8_list,
) -> support::WireSyncReturn {
    wire_psbt_bytes_to_psbt_impl(psbt_bytes)
}

#[no_mangle]
pub extern "C" fn wire_new_qr_reader(port_: i64) {
    wire_new_qr_reader_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_new_qr_encoder(port_: i64, bytes: *mut wire_uint_8_list) {
    wire_new_qr_encoder_impl(port_, bytes)
}

#[no_mangle]
pub extern "C" fn wire_polynomial_identifier(
    frost_key: *mut wire_FrostKey,
) -> support::WireSyncReturn {
    wire_polynomial_identifier_impl(frost_key)
}

#[no_mangle]
pub extern "C" fn wire_txid__method__Transaction(
    that: *mut wire_Transaction,
) -> support::WireSyncReturn {
    wire_txid__method__Transaction_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_ready__method__ConnectedDevice(
    that: *mut wire_ConnectedDevice,
) -> support::WireSyncReturn {
    wire_ready__method__ConnectedDevice_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_needs_firmware_upgrade__method__ConnectedDevice(
    that: *mut wire_ConnectedDevice,
) -> support::WireSyncReturn {
    wire_needs_firmware_upgrade__method__ConnectedDevice_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_threshold__method__FrostKey(
    that: *mut wire_FrostKey,
) -> support::WireSyncReturn {
    wire_threshold__method__FrostKey_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_id__method__FrostKey(that: *mut wire_FrostKey) -> support::WireSyncReturn {
    wire_id__method__FrostKey_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_name__method__FrostKey(that: *mut wire_FrostKey) -> support::WireSyncReturn {
    wire_name__method__FrostKey_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_devices__method__FrostKey(
    that: *mut wire_FrostKey,
) -> support::WireSyncReturn {
    wire_devices__method__FrostKey_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_satisfy__method__PortOpen(
    port_: i64,
    that: *mut wire_PortOpen,
    err: *mut wire_uint_8_list,
) {
    wire_satisfy__method__PortOpen_impl(port_, that, err)
}

#[no_mangle]
pub extern "C" fn wire_satisfy__method__PortRead(
    port_: i64,
    that: *mut wire_PortRead,
    bytes: *mut wire_uint_8_list,
    err: *mut wire_uint_8_list,
) {
    wire_satisfy__method__PortRead_impl(port_, that, bytes, err)
}

#[no_mangle]
pub extern "C" fn wire_satisfy__method__PortWrite(
    port_: i64,
    that: *mut wire_PortWrite,
    err: *mut wire_uint_8_list,
) {
    wire_satisfy__method__PortWrite_impl(port_, that, err)
}

#[no_mangle]
pub extern "C" fn wire_satisfy__method__PortBytesToRead(
    port_: i64,
    that: *mut wire_PortBytesToRead,
    bytes_to_read: u32,
) {
    wire_satisfy__method__PortBytesToRead_impl(port_, that, bytes_to_read)
}

#[no_mangle]
pub extern "C" fn wire_get_device__method__DeviceListState(
    that: *mut wire_DeviceListState,
    id: *mut wire_DeviceId,
) -> support::WireSyncReturn {
    wire_get_device__method__DeviceListState_impl(that, id)
}

#[no_mangle]
pub extern "C" fn wire_sub_tx_state__method__Wallet(
    port_: i64,
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
) {
    wire_sub_tx_state__method__Wallet_impl(port_, that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_tx_state__method__Wallet(
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
) -> support::WireSyncReturn {
    wire_tx_state__method__Wallet_impl(that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_sync_txids__method__Wallet(
    port_: i64,
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
    txids: *mut wire_StringList,
) {
    wire_sync_txids__method__Wallet_impl(port_, that, key_id, txids)
}

#[no_mangle]
pub extern "C" fn wire_sync__method__Wallet(
    port_: i64,
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
) {
    wire_sync__method__Wallet_impl(port_, that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_next_address__method__Wallet(
    port_: i64,
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
) {
    wire_next_address__method__Wallet_impl(port_, that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_addresses_state__method__Wallet(
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
) -> support::WireSyncReturn {
    wire_addresses_state__method__Wallet_impl(that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_send_to__method__Wallet(
    port_: i64,
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
    to_address: *mut wire_uint_8_list,
    value: u64,
    feerate: f64,
) {
    wire_send_to__method__Wallet_impl(port_, that, key_id, to_address, value, feerate)
}

#[no_mangle]
pub extern "C" fn wire_broadcast_tx__method__Wallet(
    port_: i64,
    that: *mut wire_Wallet,
    key_id: *mut wire_KeyId,
    tx: *mut wire_SignedTx,
) {
    wire_broadcast_tx__method__Wallet_impl(port_, that, key_id, tx)
}

#[no_mangle]
pub extern "C" fn wire_psbt_to_unsigned_tx__method__Wallet(
    that: *mut wire_Wallet,
    psbt: *mut wire_Psbt,
    key_id: *mut wire_KeyId,
) -> support::WireSyncReturn {
    wire_psbt_to_unsigned_tx__method__Wallet_impl(that, psbt, key_id)
}

#[no_mangle]
pub extern "C" fn wire_set_available_ports__method__FfiSerial(
    port_: i64,
    that: *mut wire_FfiSerial,
    ports: *mut wire_list_port_desc,
) {
    wire_set_available_ports__method__FfiSerial_impl(port_, that, ports)
}

#[no_mangle]
pub extern "C" fn wire_start_thread__method__Coordinator(port_: i64, that: *mut wire_Coordinator) {
    wire_start_thread__method__Coordinator_impl(port_, that)
}

#[no_mangle]
pub extern "C" fn wire_update_name_preview__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    id: *mut wire_DeviceId,
    name: *mut wire_uint_8_list,
) {
    wire_update_name_preview__method__Coordinator_impl(port_, that, id, name)
}

#[no_mangle]
pub extern "C" fn wire_finish_naming__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    id: *mut wire_DeviceId,
    name: *mut wire_uint_8_list,
) {
    wire_finish_naming__method__Coordinator_impl(port_, that, id, name)
}

#[no_mangle]
pub extern "C" fn wire_send_cancel__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    id: *mut wire_DeviceId,
) {
    wire_send_cancel__method__Coordinator_impl(port_, that, id)
}

#[no_mangle]
pub extern "C" fn wire_cancel_all__method__Coordinator(port_: i64, that: *mut wire_Coordinator) {
    wire_cancel_all__method__Coordinator_impl(port_, that)
}

#[no_mangle]
pub extern "C" fn wire_display_backup__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    id: *mut wire_DeviceId,
    key_id: *mut wire_KeyId,
) {
    wire_display_backup__method__Coordinator_impl(port_, that, id, key_id)
}

#[no_mangle]
pub extern "C" fn wire_key_state__method__Coordinator(
    that: *mut wire_Coordinator,
) -> support::WireSyncReturn {
    wire_key_state__method__Coordinator_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_sub_key_events__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
) {
    wire_sub_key_events__method__Coordinator_impl(port_, that)
}

#[no_mangle]
pub extern "C" fn wire_get_key__method__Coordinator(
    that: *mut wire_Coordinator,
    key_id: *mut wire_KeyId,
) -> support::WireSyncReturn {
    wire_get_key__method__Coordinator_impl(that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_keys_for_device__method__Coordinator(
    that: *mut wire_Coordinator,
    device_id: *mut wire_DeviceId,
) -> support::WireSyncReturn {
    wire_keys_for_device__method__Coordinator_impl(that, device_id)
}

#[no_mangle]
pub extern "C" fn wire_start_signing__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    key_id: *mut wire_KeyId,
    devices: *mut wire_list_device_id,
    message: *mut wire_uint_8_list,
) {
    wire_start_signing__method__Coordinator_impl(port_, that, key_id, devices, message)
}

#[no_mangle]
pub extern "C" fn wire_start_signing_tx__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    key_id: *mut wire_KeyId,
    unsigned_tx: *mut wire_UnsignedTx,
    devices: *mut wire_list_device_id,
) {
    wire_start_signing_tx__method__Coordinator_impl(port_, that, key_id, unsigned_tx, devices)
}

#[no_mangle]
pub extern "C" fn wire_nonces_available__method__Coordinator(
    that: *mut wire_Coordinator,
    id: *mut wire_DeviceId,
) -> support::WireSyncReturn {
    wire_nonces_available__method__Coordinator_impl(that, id)
}

#[no_mangle]
pub extern "C" fn wire_current_nonce__method__Coordinator(
    that: *mut wire_Coordinator,
    id: *mut wire_DeviceId,
) -> support::WireSyncReturn {
    wire_current_nonce__method__Coordinator_impl(that, id)
}

#[no_mangle]
pub extern "C" fn wire_generate_new_key__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    threshold: usize,
    devices: *mut wire_list_device_id,
) {
    wire_generate_new_key__method__Coordinator_impl(port_, that, threshold, devices)
}

#[no_mangle]
pub extern "C" fn wire_persisted_sign_session_description__method__Coordinator(
    that: *mut wire_Coordinator,
    key_id: *mut wire_KeyId,
) -> support::WireSyncReturn {
    wire_persisted_sign_session_description__method__Coordinator_impl(that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_try_restore_signing_session__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
    key_id: *mut wire_KeyId,
) {
    wire_try_restore_signing_session__method__Coordinator_impl(port_, that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_start_firmware_upgrade__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
) {
    wire_start_firmware_upgrade__method__Coordinator_impl(port_, that)
}

#[no_mangle]
pub extern "C" fn wire_upgrade_firmware_digest__method__Coordinator(
    that: *mut wire_Coordinator,
) -> support::WireSyncReturn {
    wire_upgrade_firmware_digest__method__Coordinator_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_cancel_protocol__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
) {
    wire_cancel_protocol__method__Coordinator_impl(port_, that)
}

#[no_mangle]
pub extern "C" fn wire_enter_firmware_upgrade_mode__method__Coordinator(
    port_: i64,
    that: *mut wire_Coordinator,
) {
    wire_enter_firmware_upgrade_mode__method__Coordinator_impl(port_, that)
}

#[no_mangle]
pub extern "C" fn wire_get_device_name__method__Coordinator(
    that: *mut wire_Coordinator,
    id: *mut wire_DeviceId,
) -> support::WireSyncReturn {
    wire_get_device_name__method__Coordinator_impl(that, id)
}

#[no_mangle]
pub extern "C" fn wire_descriptor_for_key__method__BitcoinContext(
    that: *mut wire_BitcoinContext,
    key_id: *mut wire_KeyId,
) -> support::WireSyncReturn {
    wire_descriptor_for_key__method__BitcoinContext_impl(that, key_id)
}

#[no_mangle]
pub extern "C" fn wire_validate_amount__method__BitcoinContext(
    that: *mut wire_BitcoinContext,
    address: *mut wire_uint_8_list,
    value: u64,
) -> support::WireSyncReturn {
    wire_validate_amount__method__BitcoinContext_impl(that, address, value)
}

#[no_mangle]
pub extern "C" fn wire_validate_destination_address__method__BitcoinContext(
    that: *mut wire_BitcoinContext,
    address: *mut wire_uint_8_list,
) -> support::WireSyncReturn {
    wire_validate_destination_address__method__BitcoinContext_impl(that, address)
}

#[no_mangle]
pub extern "C" fn wire_effect__method__SignedTx(
    that: *mut wire_SignedTx,
    key_id: *mut wire_KeyId,
    network: wire_BitcoinNetwork,
) -> support::WireSyncReturn {
    wire_effect__method__SignedTx_impl(that, key_id, network)
}

#[no_mangle]
pub extern "C" fn wire_attach_signatures_to_psbt__method__UnsignedTx(
    port_: i64,
    that: *mut wire_UnsignedTx,
    signatures: *mut wire_list_encoded_signature,
    psbt: *mut wire_Psbt,
) {
    wire_attach_signatures_to_psbt__method__UnsignedTx_impl(port_, that, signatures, psbt)
}

#[no_mangle]
pub extern "C" fn wire_complete__method__UnsignedTx(
    port_: i64,
    that: *mut wire_UnsignedTx,
    signatures: *mut wire_list_encoded_signature,
) {
    wire_complete__method__UnsignedTx_impl(port_, that, signatures)
}

#[no_mangle]
pub extern "C" fn wire_effect__method__UnsignedTx(
    that: *mut wire_UnsignedTx,
    key_id: *mut wire_KeyId,
    network: wire_BitcoinNetwork,
) -> support::WireSyncReturn {
    wire_effect__method__UnsignedTx_impl(that, key_id, network)
}

#[no_mangle]
pub extern "C" fn wire_to_bytes__method__Psbt(that: *mut wire_Psbt) -> support::WireSyncReturn {
    wire_to_bytes__method__Psbt_impl(that)
}

#[no_mangle]
pub extern "C" fn wire_decode_from_bytes__method__QrReader(
    port_: i64,
    that: *mut wire_QrReader,
    bytes: *mut wire_uint_8_list,
) {
    wire_decode_from_bytes__method__QrReader_impl(port_, that, bytes)
}

#[no_mangle]
pub extern "C" fn wire_next__method__QrEncoder(
    that: *mut wire_QrEncoder,
) -> support::WireSyncReturn {
    wire_next__method__QrEncoder_impl(that)
}

// Section: allocate functions

#[no_mangle]
pub extern "C" fn new_ArcMutexVecPortDesc() -> wire_ArcMutexVecPortDesc {
    wire_ArcMutexVecPortDesc::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_ArcRTransaction() -> wire_ArcRTransaction {
    wire_ArcRTransaction::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_BitcoinNetwork() -> wire_BitcoinNetwork {
    wire_BitcoinNetwork::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_BitcoinPsbt() -> wire_BitcoinPsbt {
    wire_BitcoinPsbt::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_ChainSync() -> wire_ChainSync {
    wire_ChainSync::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_FfiCoordinator() -> wire_FfiCoordinator {
    wire_FfiCoordinator::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_FfiQrEncoder() -> wire_FfiQrEncoder {
    wire_FfiQrEncoder::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_FfiQrReader() -> wire_FfiQrReader {
    wire_FfiQrReader::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_FrostsnapCoreBitcoinTransactionTransactionTemplate(
) -> wire_FrostsnapCoreBitcoinTransactionTransactionTemplate {
    wire_FrostsnapCoreBitcoinTransactionTransactionTemplate::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_FrostsnapCoreCoordinatorCoordinatorFrostKey(
) -> wire_FrostsnapCoreCoordinatorCoordinatorFrostKey {
    wire_FrostsnapCoreCoordinatorCoordinatorFrostKey::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_MutexBTreeMapKeyIdStreamSinkTxState(
) -> wire_MutexBTreeMapKeyIdStreamSinkTxState {
    wire_MutexBTreeMapKeyIdStreamSinkTxState::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_MutexFrostsnapWallet() -> wire_MutexFrostsnapWallet {
    wire_MutexFrostsnapWallet::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_PortBytesToReadSender() -> wire_PortBytesToReadSender {
    wire_PortBytesToReadSender::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_PortOpenSender() -> wire_PortOpenSender {
    wire_PortOpenSender::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_PortReadSender() -> wire_PortReadSender {
    wire_PortReadSender::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_PortWriteSender() -> wire_PortWriteSender {
    wire_PortWriteSender::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_RTransaction() -> wire_RTransaction {
    wire_RTransaction::new_with_null_ptr()
}

#[no_mangle]
pub extern "C" fn new_StringList_0(len: i32) -> *mut wire_StringList {
    let wrap = wire_StringList {
        ptr: support::new_leak_vec_ptr(<*mut wire_uint_8_list>::new_with_null_ptr(), len),
        len,
    };
    support::new_leak_box_ptr(wrap)
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_bitcoin_context_0() -> *mut wire_BitcoinContext {
    support::new_leak_box_ptr(wire_BitcoinContext::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_confirmation_time_0() -> *mut wire_ConfirmationTime {
    support::new_leak_box_ptr(wire_ConfirmationTime::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_connected_device_0() -> *mut wire_ConnectedDevice {
    support::new_leak_box_ptr(wire_ConnectedDevice::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_coordinator_0() -> *mut wire_Coordinator {
    support::new_leak_box_ptr(wire_Coordinator::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_device_id_0() -> *mut wire_DeviceId {
    support::new_leak_box_ptr(wire_DeviceId::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_device_list_state_0() -> *mut wire_DeviceListState {
    support::new_leak_box_ptr(wire_DeviceListState::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_ffi_serial_0() -> *mut wire_FfiSerial {
    support::new_leak_box_ptr(wire_FfiSerial::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_frost_key_0() -> *mut wire_FrostKey {
    support::new_leak_box_ptr(wire_FrostKey::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_key_id_0() -> *mut wire_KeyId {
    support::new_leak_box_ptr(wire_KeyId::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_port_bytes_to_read_0() -> *mut wire_PortBytesToRead {
    support::new_leak_box_ptr(wire_PortBytesToRead::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_port_open_0() -> *mut wire_PortOpen {
    support::new_leak_box_ptr(wire_PortOpen::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_port_read_0() -> *mut wire_PortRead {
    support::new_leak_box_ptr(wire_PortRead::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_port_write_0() -> *mut wire_PortWrite {
    support::new_leak_box_ptr(wire_PortWrite::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_psbt_0() -> *mut wire_Psbt {
    support::new_leak_box_ptr(wire_Psbt::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_qr_encoder_0() -> *mut wire_QrEncoder {
    support::new_leak_box_ptr(wire_QrEncoder::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_qr_reader_0() -> *mut wire_QrReader {
    support::new_leak_box_ptr(wire_QrReader::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_signed_tx_0() -> *mut wire_SignedTx {
    support::new_leak_box_ptr(wire_SignedTx::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_transaction_0() -> *mut wire_Transaction {
    support::new_leak_box_ptr(wire_Transaction::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_unsigned_tx_0() -> *mut wire_UnsignedTx {
    support::new_leak_box_ptr(wire_UnsignedTx::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_wallet_0() -> *mut wire_Wallet {
    support::new_leak_box_ptr(wire_Wallet::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_list_connected_device_0(len: i32) -> *mut wire_list_connected_device {
    let wrap = wire_list_connected_device {
        ptr: support::new_leak_vec_ptr(<wire_ConnectedDevice>::new_with_null_ptr(), len),
        len,
    };
    support::new_leak_box_ptr(wrap)
}

#[no_mangle]
pub extern "C" fn new_list_device_id_0(len: i32) -> *mut wire_list_device_id {
    let wrap = wire_list_device_id {
        ptr: support::new_leak_vec_ptr(<wire_DeviceId>::new_with_null_ptr(), len),
        len,
    };
    support::new_leak_box_ptr(wrap)
}

#[no_mangle]
pub extern "C" fn new_list_encoded_signature_0(len: i32) -> *mut wire_list_encoded_signature {
    let wrap = wire_list_encoded_signature {
        ptr: support::new_leak_vec_ptr(<wire_EncodedSignature>::new_with_null_ptr(), len),
        len,
    };
    support::new_leak_box_ptr(wrap)
}

#[no_mangle]
pub extern "C" fn new_list_port_desc_0(len: i32) -> *mut wire_list_port_desc {
    let wrap = wire_list_port_desc {
        ptr: support::new_leak_vec_ptr(<wire_PortDesc>::new_with_null_ptr(), len),
        len,
    };
    support::new_leak_box_ptr(wrap)
}

#[no_mangle]
pub extern "C" fn new_uint_8_list_0(len: i32) -> *mut wire_uint_8_list {
    let ans = wire_uint_8_list {
        ptr: support::new_leak_vec_ptr(Default::default(), len),
        len,
    };
    support::new_leak_box_ptr(ans)
}

// Section: related functions

#[no_mangle]
pub extern "C" fn drop_opaque_ArcMutexVecPortDesc(ptr: *const c_void) {
    unsafe {
        Arc::<Arc<Mutex<Vec<PortDesc>>>>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_ArcMutexVecPortDesc(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Arc<Mutex<Vec<PortDesc>>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_ArcRTransaction(ptr: *const c_void) {
    unsafe {
        Arc::<Arc<RTransaction>>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_ArcRTransaction(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Arc<RTransaction>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_BitcoinNetwork(ptr: *const c_void) {
    unsafe {
        Arc::<bitcoin::Network>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_BitcoinNetwork(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<bitcoin::Network>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_BitcoinPsbt(ptr: *const c_void) {
    unsafe {
        Arc::<BitcoinPsbt>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_BitcoinPsbt(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<BitcoinPsbt>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_ChainSync(ptr: *const c_void) {
    unsafe {
        Arc::<ChainSync>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_ChainSync(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<ChainSync>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_FfiCoordinator(ptr: *const c_void) {
    unsafe {
        Arc::<FfiCoordinator>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_FfiCoordinator(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<FfiCoordinator>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_FfiQrEncoder(ptr: *const c_void) {
    unsafe {
        Arc::<FfiQrEncoder>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_FfiQrEncoder(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<FfiQrEncoder>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_FfiQrReader(ptr: *const c_void) {
    unsafe {
        Arc::<FfiQrReader>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_FfiQrReader(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<FfiQrReader>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate(
    ptr: *const c_void,
) {
    unsafe {
        Arc::<frostsnap_core::bitcoin_transaction::TransactionTemplate>::decrement_strong_count(
            ptr as _,
        );
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate(
    ptr: *const c_void,
) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::bitcoin_transaction::TransactionTemplate>::increment_strong_count(
            ptr as _,
        );
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey(ptr: *const c_void) {
    unsafe {
        Arc::<frostsnap_core::coordinator::CoordinatorFrostKey>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey(
    ptr: *const c_void,
) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::coordinator::CoordinatorFrostKey>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr: *const c_void) {
    unsafe {
        Arc::<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_MutexBTreeMapKeyIdStreamSinkTxState(
    ptr: *const c_void,
) -> *const c_void {
    unsafe {
        Arc::<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_MutexFrostsnapWallet(ptr: *const c_void) {
    unsafe {
        Arc::<Mutex<FrostsnapWallet>>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_MutexFrostsnapWallet(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Mutex<FrostsnapWallet>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_PortBytesToReadSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortBytesToReadSender>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_PortBytesToReadSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortBytesToReadSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_PortOpenSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortOpenSender>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_PortOpenSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortOpenSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_PortReadSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortReadSender>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_PortReadSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortReadSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_PortWriteSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortWriteSender>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_PortWriteSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortWriteSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[no_mangle]
pub extern "C" fn drop_opaque_RTransaction(ptr: *const c_void) {
    unsafe {
        Arc::<RTransaction>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_RTransaction(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<RTransaction>::increment_strong_count(ptr as _);
        ptr
    }
}

// Section: impl Wire2Api

impl Wire2Api<RustOpaque<Arc<Mutex<Vec<PortDesc>>>>> for wire_ArcMutexVecPortDesc {
    fn wire2api(self) -> RustOpaque<Arc<Mutex<Vec<PortDesc>>>> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<Arc<RTransaction>>> for wire_ArcRTransaction {
    fn wire2api(self) -> RustOpaque<Arc<RTransaction>> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<bitcoin::Network>> for wire_BitcoinNetwork {
    fn wire2api(self) -> RustOpaque<bitcoin::Network> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<BitcoinPsbt>> for wire_BitcoinPsbt {
    fn wire2api(self) -> RustOpaque<BitcoinPsbt> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<ChainSync>> for wire_ChainSync {
    fn wire2api(self) -> RustOpaque<ChainSync> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<FfiCoordinator>> for wire_FfiCoordinator {
    fn wire2api(self) -> RustOpaque<FfiCoordinator> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<FfiQrEncoder>> for wire_FfiQrEncoder {
    fn wire2api(self) -> RustOpaque<FfiQrEncoder> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<FfiQrReader>> for wire_FfiQrReader {
    fn wire2api(self) -> RustOpaque<FfiQrReader> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<frostsnap_core::bitcoin_transaction::TransactionTemplate>>
    for wire_FrostsnapCoreBitcoinTransactionTransactionTemplate
{
    fn wire2api(self) -> RustOpaque<frostsnap_core::bitcoin_transaction::TransactionTemplate> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<frostsnap_core::coordinator::CoordinatorFrostKey>>
    for wire_FrostsnapCoreCoordinatorCoordinatorFrostKey
{
    fn wire2api(self) -> RustOpaque<frostsnap_core::coordinator::CoordinatorFrostKey> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>>
    for wire_MutexBTreeMapKeyIdStreamSinkTxState
{
    fn wire2api(self) -> RustOpaque<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<Mutex<FrostsnapWallet>>> for wire_MutexFrostsnapWallet {
    fn wire2api(self) -> RustOpaque<Mutex<FrostsnapWallet>> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<PortBytesToReadSender>> for wire_PortBytesToReadSender {
    fn wire2api(self) -> RustOpaque<PortBytesToReadSender> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<PortOpenSender>> for wire_PortOpenSender {
    fn wire2api(self) -> RustOpaque<PortOpenSender> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<PortReadSender>> for wire_PortReadSender {
    fn wire2api(self) -> RustOpaque<PortReadSender> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<PortWriteSender>> for wire_PortWriteSender {
    fn wire2api(self) -> RustOpaque<PortWriteSender> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<RustOpaque<RTransaction>> for wire_RTransaction {
    fn wire2api(self) -> RustOpaque<RTransaction> {
        unsafe { support::opaque_from_dart(self.ptr as _) }
    }
}
impl Wire2Api<String> for *mut wire_uint_8_list {
    fn wire2api(self) -> String {
        let vec: Vec<u8> = self.wire2api();
        String::from_utf8_lossy(&vec).into_owned()
    }
}
impl Wire2Api<Vec<String>> for *mut wire_StringList {
    fn wire2api(self) -> Vec<String> {
        let vec = unsafe {
            let wrap = support::box_from_leak_ptr(self);
            support::vec_from_leak_ptr(wrap.ptr, wrap.len)
        };
        vec.into_iter().map(Wire2Api::wire2api).collect()
    }
}
impl Wire2Api<BitcoinContext> for wire_BitcoinContext {
    fn wire2api(self) -> BitcoinContext {
        BitcoinContext {
            network: self.network.wire2api(),
        }
    }
}
impl Wire2Api<BitcoinContext> for *mut wire_BitcoinContext {
    fn wire2api(self) -> BitcoinContext {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<BitcoinContext>::wire2api(*wrap).into()
    }
}
impl Wire2Api<ConfirmationTime> for *mut wire_ConfirmationTime {
    fn wire2api(self) -> ConfirmationTime {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<ConfirmationTime>::wire2api(*wrap).into()
    }
}
impl Wire2Api<ConnectedDevice> for *mut wire_ConnectedDevice {
    fn wire2api(self) -> ConnectedDevice {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<ConnectedDevice>::wire2api(*wrap).into()
    }
}
impl Wire2Api<Coordinator> for *mut wire_Coordinator {
    fn wire2api(self) -> Coordinator {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<Coordinator>::wire2api(*wrap).into()
    }
}
impl Wire2Api<DeviceId> for *mut wire_DeviceId {
    fn wire2api(self) -> DeviceId {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<DeviceId>::wire2api(*wrap).into()
    }
}
impl Wire2Api<DeviceListState> for *mut wire_DeviceListState {
    fn wire2api(self) -> DeviceListState {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<DeviceListState>::wire2api(*wrap).into()
    }
}
impl Wire2Api<FfiSerial> for *mut wire_FfiSerial {
    fn wire2api(self) -> FfiSerial {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<FfiSerial>::wire2api(*wrap).into()
    }
}
impl Wire2Api<FrostKey> for *mut wire_FrostKey {
    fn wire2api(self) -> FrostKey {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<FrostKey>::wire2api(*wrap).into()
    }
}
impl Wire2Api<KeyId> for *mut wire_KeyId {
    fn wire2api(self) -> KeyId {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<KeyId>::wire2api(*wrap).into()
    }
}
impl Wire2Api<PortBytesToRead> for *mut wire_PortBytesToRead {
    fn wire2api(self) -> PortBytesToRead {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<PortBytesToRead>::wire2api(*wrap).into()
    }
}
impl Wire2Api<PortOpen> for *mut wire_PortOpen {
    fn wire2api(self) -> PortOpen {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<PortOpen>::wire2api(*wrap).into()
    }
}
impl Wire2Api<PortRead> for *mut wire_PortRead {
    fn wire2api(self) -> PortRead {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<PortRead>::wire2api(*wrap).into()
    }
}
impl Wire2Api<PortWrite> for *mut wire_PortWrite {
    fn wire2api(self) -> PortWrite {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<PortWrite>::wire2api(*wrap).into()
    }
}
impl Wire2Api<Psbt> for *mut wire_Psbt {
    fn wire2api(self) -> Psbt {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<Psbt>::wire2api(*wrap).into()
    }
}
impl Wire2Api<QrEncoder> for *mut wire_QrEncoder {
    fn wire2api(self) -> QrEncoder {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<QrEncoder>::wire2api(*wrap).into()
    }
}
impl Wire2Api<QrReader> for *mut wire_QrReader {
    fn wire2api(self) -> QrReader {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<QrReader>::wire2api(*wrap).into()
    }
}
impl Wire2Api<SignedTx> for *mut wire_SignedTx {
    fn wire2api(self) -> SignedTx {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<SignedTx>::wire2api(*wrap).into()
    }
}
impl Wire2Api<Transaction> for *mut wire_Transaction {
    fn wire2api(self) -> Transaction {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<Transaction>::wire2api(*wrap).into()
    }
}
impl Wire2Api<UnsignedTx> for *mut wire_UnsignedTx {
    fn wire2api(self) -> UnsignedTx {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<UnsignedTx>::wire2api(*wrap).into()
    }
}
impl Wire2Api<Wallet> for *mut wire_Wallet {
    fn wire2api(self) -> Wallet {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<Wallet>::wire2api(*wrap).into()
    }
}
impl Wire2Api<ConfirmationTime> for wire_ConfirmationTime {
    fn wire2api(self) -> ConfirmationTime {
        ConfirmationTime {
            height: self.height.wire2api(),
            time: self.time.wire2api(),
        }
    }
}
impl Wire2Api<ConnectedDevice> for wire_ConnectedDevice {
    fn wire2api(self) -> ConnectedDevice {
        ConnectedDevice {
            name: self.name.wire2api(),
            firmware_digest: self.firmware_digest.wire2api(),
            latest_digest: self.latest_digest.wire2api(),
            id: self.id.wire2api(),
        }
    }
}
impl Wire2Api<Coordinator> for wire_Coordinator {
    fn wire2api(self) -> Coordinator {
        Coordinator(self.field0.wire2api())
    }
}
impl Wire2Api<DeviceId> for wire_DeviceId {
    fn wire2api(self) -> DeviceId {
        DeviceId(self.field0.wire2api())
    }
}
impl Wire2Api<DeviceListState> for wire_DeviceListState {
    fn wire2api(self) -> DeviceListState {
        DeviceListState {
            devices: self.devices.wire2api(),
            state_id: self.state_id.wire2api(),
        }
    }
}
impl Wire2Api<EncodedSignature> for wire_EncodedSignature {
    fn wire2api(self) -> EncodedSignature {
        EncodedSignature(self.field0.wire2api())
    }
}

impl Wire2Api<FfiSerial> for wire_FfiSerial {
    fn wire2api(self) -> FfiSerial {
        FfiSerial {
            available_ports: self.available_ports.wire2api(),
        }
    }
}
impl Wire2Api<FrostKey> for wire_FrostKey {
    fn wire2api(self) -> FrostKey {
        FrostKey(self.field0.wire2api())
    }
}

impl Wire2Api<KeyId> for wire_KeyId {
    fn wire2api(self) -> KeyId {
        KeyId(self.field0.wire2api())
    }
}
impl Wire2Api<Vec<ConnectedDevice>> for *mut wire_list_connected_device {
    fn wire2api(self) -> Vec<ConnectedDevice> {
        let vec = unsafe {
            let wrap = support::box_from_leak_ptr(self);
            support::vec_from_leak_ptr(wrap.ptr, wrap.len)
        };
        vec.into_iter().map(Wire2Api::wire2api).collect()
    }
}
impl Wire2Api<Vec<DeviceId>> for *mut wire_list_device_id {
    fn wire2api(self) -> Vec<DeviceId> {
        let vec = unsafe {
            let wrap = support::box_from_leak_ptr(self);
            support::vec_from_leak_ptr(wrap.ptr, wrap.len)
        };
        vec.into_iter().map(Wire2Api::wire2api).collect()
    }
}
impl Wire2Api<Vec<EncodedSignature>> for *mut wire_list_encoded_signature {
    fn wire2api(self) -> Vec<EncodedSignature> {
        let vec = unsafe {
            let wrap = support::box_from_leak_ptr(self);
            support::vec_from_leak_ptr(wrap.ptr, wrap.len)
        };
        vec.into_iter().map(Wire2Api::wire2api).collect()
    }
}
impl Wire2Api<Vec<PortDesc>> for *mut wire_list_port_desc {
    fn wire2api(self) -> Vec<PortDesc> {
        let vec = unsafe {
            let wrap = support::box_from_leak_ptr(self);
            support::vec_from_leak_ptr(wrap.ptr, wrap.len)
        };
        vec.into_iter().map(Wire2Api::wire2api).collect()
    }
}

impl Wire2Api<PortBytesToRead> for wire_PortBytesToRead {
    fn wire2api(self) -> PortBytesToRead {
        PortBytesToRead {
            id: self.id.wire2api(),
            ready: self.ready.wire2api(),
        }
    }
}
impl Wire2Api<PortDesc> for wire_PortDesc {
    fn wire2api(self) -> PortDesc {
        PortDesc {
            id: self.id.wire2api(),
            vid: self.vid.wire2api(),
            pid: self.pid.wire2api(),
        }
    }
}
impl Wire2Api<PortOpen> for wire_PortOpen {
    fn wire2api(self) -> PortOpen {
        PortOpen {
            id: self.id.wire2api(),
            baud_rate: self.baud_rate.wire2api(),
            ready: self.ready.wire2api(),
        }
    }
}
impl Wire2Api<PortRead> for wire_PortRead {
    fn wire2api(self) -> PortRead {
        PortRead {
            id: self.id.wire2api(),
            len: self.len.wire2api(),
            ready: self.ready.wire2api(),
        }
    }
}
impl Wire2Api<PortWrite> for wire_PortWrite {
    fn wire2api(self) -> PortWrite {
        PortWrite {
            id: self.id.wire2api(),
            bytes: self.bytes.wire2api(),
            ready: self.ready.wire2api(),
        }
    }
}
impl Wire2Api<Psbt> for wire_Psbt {
    fn wire2api(self) -> Psbt {
        Psbt {
            inner: self.inner.wire2api(),
        }
    }
}
impl Wire2Api<QrEncoder> for wire_QrEncoder {
    fn wire2api(self) -> QrEncoder {
        QrEncoder(self.field0.wire2api())
    }
}
impl Wire2Api<QrReader> for wire_QrReader {
    fn wire2api(self) -> QrReader {
        QrReader(self.field0.wire2api())
    }
}
impl Wire2Api<SignedTx> for wire_SignedTx {
    fn wire2api(self) -> SignedTx {
        SignedTx {
            signed_tx: self.signed_tx.wire2api(),
            unsigned_tx: self.unsigned_tx.wire2api(),
        }
    }
}
impl Wire2Api<Transaction> for wire_Transaction {
    fn wire2api(self) -> Transaction {
        Transaction {
            net_value: self.net_value.wire2api(),
            inner: self.inner.wire2api(),
            confirmation_time: self.confirmation_time.wire2api(),
        }
    }
}

impl Wire2Api<[u8; 33]> for *mut wire_uint_8_list {
    fn wire2api(self) -> [u8; 33] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
    }
}
impl Wire2Api<[u8; 64]> for *mut wire_uint_8_list {
    fn wire2api(self) -> [u8; 64] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
    }
}
impl Wire2Api<Vec<u8>> for *mut wire_uint_8_list {
    fn wire2api(self) -> Vec<u8> {
        unsafe {
            let wrap = support::box_from_leak_ptr(self);
            support::vec_from_leak_ptr(wrap.ptr, wrap.len)
        }
    }
}
impl Wire2Api<UnsignedTx> for wire_UnsignedTx {
    fn wire2api(self) -> UnsignedTx {
        UnsignedTx {
            template_tx: self.template_tx.wire2api(),
        }
    }
}

impl Wire2Api<Wallet> for wire_Wallet {
    fn wire2api(self) -> Wallet {
        Wallet {
            inner: self.inner.wire2api(),
            wallet_streams: self.wallet_streams.wire2api(),
            chain_sync: self.chain_sync.wire2api(),
        }
    }
}
// Section: wire structs

#[repr(C)]
#[derive(Clone)]
pub struct wire_ArcMutexVecPortDesc {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_ArcRTransaction {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_BitcoinNetwork {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_BitcoinPsbt {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_ChainSync {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FfiCoordinator {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FfiQrEncoder {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FfiQrReader {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FrostsnapCoreBitcoinTransactionTransactionTemplate {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FrostsnapCoreCoordinatorCoordinatorFrostKey {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_MutexBTreeMapKeyIdStreamSinkTxState {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_MutexFrostsnapWallet {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortBytesToReadSender {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortOpenSender {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortReadSender {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortWriteSender {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_RTransaction {
    ptr: *const core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_StringList {
    ptr: *mut *mut wire_uint_8_list,
    len: i32,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_BitcoinContext {
    network: wire_BitcoinNetwork,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_ConfirmationTime {
    height: u32,
    time: u64,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_ConnectedDevice {
    name: *mut wire_uint_8_list,
    firmware_digest: *mut wire_uint_8_list,
    latest_digest: *mut wire_uint_8_list,
    id: wire_DeviceId,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_Coordinator {
    field0: wire_FfiCoordinator,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_DeviceId {
    field0: *mut wire_uint_8_list,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_DeviceListState {
    devices: *mut wire_list_connected_device,
    state_id: usize,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_EncodedSignature {
    field0: *mut wire_uint_8_list,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FfiSerial {
    available_ports: wire_ArcMutexVecPortDesc,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FrostKey {
    field0: wire_FrostsnapCoreCoordinatorCoordinatorFrostKey,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_KeyId {
    field0: *mut wire_uint_8_list,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_list_connected_device {
    ptr: *mut wire_ConnectedDevice,
    len: i32,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_list_device_id {
    ptr: *mut wire_DeviceId,
    len: i32,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_list_encoded_signature {
    ptr: *mut wire_EncodedSignature,
    len: i32,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_list_port_desc {
    ptr: *mut wire_PortDesc,
    len: i32,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortBytesToRead {
    id: *mut wire_uint_8_list,
    ready: wire_PortBytesToReadSender,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortDesc {
    id: *mut wire_uint_8_list,
    vid: u16,
    pid: u16,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortOpen {
    id: *mut wire_uint_8_list,
    baud_rate: u32,
    ready: wire_PortOpenSender,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortRead {
    id: *mut wire_uint_8_list,
    len: usize,
    ready: wire_PortReadSender,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_PortWrite {
    id: *mut wire_uint_8_list,
    bytes: *mut wire_uint_8_list,
    ready: wire_PortWriteSender,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_Psbt {
    inner: wire_BitcoinPsbt,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_QrEncoder {
    field0: wire_FfiQrEncoder,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_QrReader {
    field0: wire_FfiQrReader,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_SignedTx {
    signed_tx: wire_RTransaction,
    unsigned_tx: wire_UnsignedTx,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_Transaction {
    net_value: i64,
    inner: wire_ArcRTransaction,
    confirmation_time: *mut wire_ConfirmationTime,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_uint_8_list {
    ptr: *mut u8,
    len: i32,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_UnsignedTx {
    template_tx: wire_FrostsnapCoreBitcoinTransactionTransactionTemplate,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_Wallet {
    inner: wire_MutexFrostsnapWallet,
    wallet_streams: wire_MutexBTreeMapKeyIdStreamSinkTxState,
    chain_sync: wire_ChainSync,
}

// Section: impl NewWithNullPtr

pub trait NewWithNullPtr {
    fn new_with_null_ptr() -> Self;
}

impl<T> NewWithNullPtr for *mut T {
    fn new_with_null_ptr() -> Self {
        std::ptr::null_mut()
    }
}

impl NewWithNullPtr for wire_ArcMutexVecPortDesc {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_ArcRTransaction {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_BitcoinNetwork {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_BitcoinPsbt {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_ChainSync {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_FfiCoordinator {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_FfiQrEncoder {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_FfiQrReader {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_FrostsnapCoreBitcoinTransactionTransactionTemplate {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_FrostsnapCoreCoordinatorCoordinatorFrostKey {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_MutexBTreeMapKeyIdStreamSinkTxState {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_MutexFrostsnapWallet {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_PortBytesToReadSender {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_PortOpenSender {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_PortReadSender {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_PortWriteSender {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}
impl NewWithNullPtr for wire_RTransaction {
    fn new_with_null_ptr() -> Self {
        Self {
            ptr: core::ptr::null(),
        }
    }
}

impl NewWithNullPtr for wire_BitcoinContext {
    fn new_with_null_ptr() -> Self {
        Self {
            network: wire_BitcoinNetwork::new_with_null_ptr(),
        }
    }
}

impl Default for wire_BitcoinContext {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_ConfirmationTime {
    fn new_with_null_ptr() -> Self {
        Self {
            height: Default::default(),
            time: Default::default(),
        }
    }
}

impl Default for wire_ConfirmationTime {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_ConnectedDevice {
    fn new_with_null_ptr() -> Self {
        Self {
            name: core::ptr::null_mut(),
            firmware_digest: core::ptr::null_mut(),
            latest_digest: core::ptr::null_mut(),
            id: Default::default(),
        }
    }
}

impl Default for wire_ConnectedDevice {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_Coordinator {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: wire_FfiCoordinator::new_with_null_ptr(),
        }
    }
}

impl Default for wire_Coordinator {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_DeviceId {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: core::ptr::null_mut(),
        }
    }
}

impl Default for wire_DeviceId {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_DeviceListState {
    fn new_with_null_ptr() -> Self {
        Self {
            devices: core::ptr::null_mut(),
            state_id: Default::default(),
        }
    }
}

impl Default for wire_DeviceListState {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_EncodedSignature {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: core::ptr::null_mut(),
        }
    }
}

impl Default for wire_EncodedSignature {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_FfiSerial {
    fn new_with_null_ptr() -> Self {
        Self {
            available_ports: wire_ArcMutexVecPortDesc::new_with_null_ptr(),
        }
    }
}

impl Default for wire_FfiSerial {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_FrostKey {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: wire_FrostsnapCoreCoordinatorCoordinatorFrostKey::new_with_null_ptr(),
        }
    }
}

impl Default for wire_FrostKey {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_KeyId {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: core::ptr::null_mut(),
        }
    }
}

impl Default for wire_KeyId {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_PortBytesToRead {
    fn new_with_null_ptr() -> Self {
        Self {
            id: core::ptr::null_mut(),
            ready: wire_PortBytesToReadSender::new_with_null_ptr(),
        }
    }
}

impl Default for wire_PortBytesToRead {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_PortDesc {
    fn new_with_null_ptr() -> Self {
        Self {
            id: core::ptr::null_mut(),
            vid: Default::default(),
            pid: Default::default(),
        }
    }
}

impl Default for wire_PortDesc {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_PortOpen {
    fn new_with_null_ptr() -> Self {
        Self {
            id: core::ptr::null_mut(),
            baud_rate: Default::default(),
            ready: wire_PortOpenSender::new_with_null_ptr(),
        }
    }
}

impl Default for wire_PortOpen {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_PortRead {
    fn new_with_null_ptr() -> Self {
        Self {
            id: core::ptr::null_mut(),
            len: Default::default(),
            ready: wire_PortReadSender::new_with_null_ptr(),
        }
    }
}

impl Default for wire_PortRead {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_PortWrite {
    fn new_with_null_ptr() -> Self {
        Self {
            id: core::ptr::null_mut(),
            bytes: core::ptr::null_mut(),
            ready: wire_PortWriteSender::new_with_null_ptr(),
        }
    }
}

impl Default for wire_PortWrite {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_Psbt {
    fn new_with_null_ptr() -> Self {
        Self {
            inner: wire_BitcoinPsbt::new_with_null_ptr(),
        }
    }
}

impl Default for wire_Psbt {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_QrEncoder {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: wire_FfiQrEncoder::new_with_null_ptr(),
        }
    }
}

impl Default for wire_QrEncoder {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_QrReader {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: wire_FfiQrReader::new_with_null_ptr(),
        }
    }
}

impl Default for wire_QrReader {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_SignedTx {
    fn new_with_null_ptr() -> Self {
        Self {
            signed_tx: wire_RTransaction::new_with_null_ptr(),
            unsigned_tx: Default::default(),
        }
    }
}

impl Default for wire_SignedTx {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_Transaction {
    fn new_with_null_ptr() -> Self {
        Self {
            net_value: Default::default(),
            inner: wire_ArcRTransaction::new_with_null_ptr(),
            confirmation_time: core::ptr::null_mut(),
        }
    }
}

impl Default for wire_Transaction {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_UnsignedTx {
    fn new_with_null_ptr() -> Self {
        Self {
            template_tx: wire_FrostsnapCoreBitcoinTransactionTransactionTemplate::new_with_null_ptr(
            ),
        }
    }
}

impl Default for wire_UnsignedTx {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

impl NewWithNullPtr for wire_Wallet {
    fn new_with_null_ptr() -> Self {
        Self {
            inner: wire_MutexFrostsnapWallet::new_with_null_ptr(),
            wallet_streams: wire_MutexBTreeMapKeyIdStreamSinkTxState::new_with_null_ptr(),
            chain_sync: wire_ChainSync::new_with_null_ptr(),
        }
    }
}

impl Default for wire_Wallet {
    fn default() -> Self {
        Self::new_with_null_ptr()
    }
}

// Section: sync execution mode utility

#[no_mangle]
pub extern "C" fn free_WireSyncReturn(ptr: support::WireSyncReturn) {
    unsafe {
        let _ = support::box_from_leak_ptr(ptr);
    };
}
