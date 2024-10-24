use super::*;
// Section: wire functions

#[wasm_bindgen]
pub fn wire_sub_port_events(port_: MessagePort) {
    wire_sub_port_events_impl(port_)
}

#[wasm_bindgen]
pub fn wire_sub_device_events(port_: MessagePort) {
    wire_sub_device_events_impl(port_)
}

#[wasm_bindgen]
pub fn wire_log(level: i32, message: String) -> support::WireSyncReturn {
    wire_log_impl(level, message)
}

#[wasm_bindgen]
pub fn wire_turn_stderr_logging_on(port_: MessagePort, level: i32) {
    wire_turn_stderr_logging_on_impl(port_, level)
}

#[wasm_bindgen]
pub fn wire_turn_logcat_logging_on(port_: MessagePort, level: i32) {
    wire_turn_logcat_logging_on_impl(port_, level)
}

#[wasm_bindgen]
pub fn wire_device_at_index(index: usize) -> support::WireSyncReturn {
    wire_device_at_index_impl(index)
}

#[wasm_bindgen]
pub fn wire_device_list_state() -> support::WireSyncReturn {
    wire_device_list_state_impl()
}

#[wasm_bindgen]
pub fn wire_get_connected_device(id: JsValue) -> support::WireSyncReturn {
    wire_get_connected_device_impl(id)
}

#[wasm_bindgen]
pub fn wire_load(port_: MessagePort, app_dir: String) {
    wire_load_impl(port_, app_dir)
}

#[wasm_bindgen]
pub fn wire_load_host_handles_serial(port_: MessagePort, app_dir: String) {
    wire_load_host_handles_serial_impl(port_, app_dir)
}

#[wasm_bindgen]
pub fn wire_echo_key_id(port_: MessagePort, key_id: JsValue) {
    wire_echo_key_id_impl(port_, key_id)
}

#[wasm_bindgen]
pub fn wire_psbt_bytes_to_psbt(psbt_bytes: Box<[u8]>) -> support::WireSyncReturn {
    wire_psbt_bytes_to_psbt_impl(psbt_bytes)
}

#[wasm_bindgen]
pub fn wire_new_qr_reader(port_: MessagePort) {
    wire_new_qr_reader_impl(port_)
}

#[wasm_bindgen]
pub fn wire_new_qr_encoder(port_: MessagePort, bytes: Box<[u8]>) {
    wire_new_qr_encoder_impl(port_, bytes)
}

#[wasm_bindgen]
pub fn wire_txid__method__Transaction(that: JsValue) -> support::WireSyncReturn {
    wire_txid__method__Transaction_impl(that)
}

#[wasm_bindgen]
pub fn wire_ready__method__ConnectedDevice(that: JsValue) -> support::WireSyncReturn {
    wire_ready__method__ConnectedDevice_impl(that)
}

#[wasm_bindgen]
pub fn wire_needs_firmware_upgrade__method__ConnectedDevice(
    that: JsValue,
) -> support::WireSyncReturn {
    wire_needs_firmware_upgrade__method__ConnectedDevice_impl(that)
}

#[wasm_bindgen]
pub fn wire_threshold__method__FrostKey(that: JsValue) -> support::WireSyncReturn {
    wire_threshold__method__FrostKey_impl(that)
}

#[wasm_bindgen]
pub fn wire_id__method__FrostKey(that: JsValue) -> support::WireSyncReturn {
    wire_id__method__FrostKey_impl(that)
}

#[wasm_bindgen]
pub fn wire_key_name__method__FrostKey(that: JsValue) -> support::WireSyncReturn {
    wire_key_name__method__FrostKey_impl(that)
}

#[wasm_bindgen]
pub fn wire_devices__method__FrostKey(that: JsValue) -> support::WireSyncReturn {
    wire_devices__method__FrostKey_impl(that)
}

#[wasm_bindgen]
pub fn wire_polynomial_identifier__method__FrostKey(that: JsValue) -> support::WireSyncReturn {
    wire_polynomial_identifier__method__FrostKey_impl(that)
}

#[wasm_bindgen]
pub fn wire_satisfy__method__PortOpen(port_: MessagePort, that: JsValue, err: Option<String>) {
    wire_satisfy__method__PortOpen_impl(port_, that, err)
}

#[wasm_bindgen]
pub fn wire_satisfy__method__PortRead(
    port_: MessagePort,
    that: JsValue,
    bytes: Box<[u8]>,
    err: Option<String>,
) {
    wire_satisfy__method__PortRead_impl(port_, that, bytes, err)
}

#[wasm_bindgen]
pub fn wire_satisfy__method__PortWrite(port_: MessagePort, that: JsValue, err: Option<String>) {
    wire_satisfy__method__PortWrite_impl(port_, that, err)
}

#[wasm_bindgen]
pub fn wire_satisfy__method__PortBytesToRead(
    port_: MessagePort,
    that: JsValue,
    bytes_to_read: u32,
) {
    wire_satisfy__method__PortBytesToRead_impl(port_, that, bytes_to_read)
}

#[wasm_bindgen]
pub fn wire_get_device__method__DeviceListState(
    that: JsValue,
    id: JsValue,
) -> support::WireSyncReturn {
    wire_get_device__method__DeviceListState_impl(that, id)
}

#[wasm_bindgen]
pub fn wire_sub_tx_state__method__Wallet(port_: MessagePort, that: JsValue, key_id: JsValue) {
    wire_sub_tx_state__method__Wallet_impl(port_, that, key_id)
}

#[wasm_bindgen]
pub fn wire_tx_state__method__Wallet(that: JsValue, key_id: JsValue) -> support::WireSyncReturn {
    wire_tx_state__method__Wallet_impl(that, key_id)
}

#[wasm_bindgen]
pub fn wire_sync_txids__method__Wallet(
    port_: MessagePort,
    that: JsValue,
    key_id: JsValue,
    txids: JsValue,
) {
    wire_sync_txids__method__Wallet_impl(port_, that, key_id, txids)
}

#[wasm_bindgen]
pub fn wire_sync__method__Wallet(port_: MessagePort, that: JsValue, key_id: JsValue) {
    wire_sync__method__Wallet_impl(port_, that, key_id)
}

#[wasm_bindgen]
pub fn wire_next_address__method__Wallet(port_: MessagePort, that: JsValue, key_id: JsValue) {
    wire_next_address__method__Wallet_impl(port_, that, key_id)
}

#[wasm_bindgen]
pub fn wire_addresses_state__method__Wallet(
    that: JsValue,
    key_id: JsValue,
) -> support::WireSyncReturn {
    wire_addresses_state__method__Wallet_impl(that, key_id)
}

#[wasm_bindgen]
pub fn wire_send_to__method__Wallet(
    port_: MessagePort,
    that: JsValue,
    key_id: JsValue,
    to_address: String,
    value: u64,
    feerate: f64,
) {
    wire_send_to__method__Wallet_impl(port_, that, key_id, to_address, value, feerate)
}

#[wasm_bindgen]
pub fn wire_broadcast_tx__method__Wallet(
    port_: MessagePort,
    that: JsValue,
    key_id: JsValue,
    tx: JsValue,
) {
    wire_broadcast_tx__method__Wallet_impl(port_, that, key_id, tx)
}

#[wasm_bindgen]
pub fn wire_psbt_to_unsigned_tx__method__Wallet(
    that: JsValue,
    psbt: JsValue,
    key_id: JsValue,
) -> support::WireSyncReturn {
    wire_psbt_to_unsigned_tx__method__Wallet_impl(that, psbt, key_id)
}

#[wasm_bindgen]
pub fn wire_signet__static_method__BitcoinNetwork() -> support::WireSyncReturn {
    wire_signet__static_method__BitcoinNetwork_impl()
}

#[wasm_bindgen]
pub fn wire_mainnet__static_method__BitcoinNetwork() -> support::WireSyncReturn {
    wire_mainnet__static_method__BitcoinNetwork_impl()
}

#[wasm_bindgen]
pub fn wire_from_string__static_method__BitcoinNetwork(string: String) -> support::WireSyncReturn {
    wire_from_string__static_method__BitcoinNetwork_impl(string)
}

#[wasm_bindgen]
pub fn wire_supported_networks__static_method__BitcoinNetwork() -> support::WireSyncReturn {
    wire_supported_networks__static_method__BitcoinNetwork_impl()
}

#[wasm_bindgen]
pub fn wire_name__method__BitcoinNetwork(that: JsValue) -> support::WireSyncReturn {
    wire_name__method__BitcoinNetwork_impl(that)
}

#[wasm_bindgen]
pub fn wire_is_mainnet__method__BitcoinNetwork(that: JsValue) -> support::WireSyncReturn {
    wire_is_mainnet__method__BitcoinNetwork_impl(that)
}

#[wasm_bindgen]
pub fn wire_descriptor_for_key__method__BitcoinNetwork(
    that: JsValue,
    key_id: JsValue,
) -> support::WireSyncReturn {
    wire_descriptor_for_key__method__BitcoinNetwork_impl(that, key_id)
}

#[wasm_bindgen]
pub fn wire_validate_amount__method__BitcoinNetwork(
    that: JsValue,
    address: String,
    value: u64,
) -> support::WireSyncReturn {
    wire_validate_amount__method__BitcoinNetwork_impl(that, address, value)
}

#[wasm_bindgen]
pub fn wire_validate_destination_address__method__BitcoinNetwork(
    that: JsValue,
    address: String,
) -> support::WireSyncReturn {
    wire_validate_destination_address__method__BitcoinNetwork_impl(that, address)
}

#[wasm_bindgen]
pub fn wire_default_electrum_server__method__BitcoinNetwork(
    that: JsValue,
) -> support::WireSyncReturn {
    wire_default_electrum_server__method__BitcoinNetwork_impl(that)
}

#[wasm_bindgen]
pub fn wire_set_available_ports__method__FfiSerial(
    port_: MessagePort,
    that: JsValue,
    ports: JsValue,
) {
    wire_set_available_ports__method__FfiSerial_impl(port_, that, ports)
}

#[wasm_bindgen]
pub fn wire_start_thread__method__Coordinator(port_: MessagePort, that: JsValue) {
    wire_start_thread__method__Coordinator_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_update_name_preview__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    id: JsValue,
    name: String,
) {
    wire_update_name_preview__method__Coordinator_impl(port_, that, id, name)
}

#[wasm_bindgen]
pub fn wire_finish_naming__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    id: JsValue,
    name: String,
) {
    wire_finish_naming__method__Coordinator_impl(port_, that, id, name)
}

#[wasm_bindgen]
pub fn wire_send_cancel__method__Coordinator(port_: MessagePort, that: JsValue, id: JsValue) {
    wire_send_cancel__method__Coordinator_impl(port_, that, id)
}

#[wasm_bindgen]
pub fn wire_display_backup__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    id: JsValue,
    key_id: JsValue,
) {
    wire_display_backup__method__Coordinator_impl(port_, that, id, key_id)
}

#[wasm_bindgen]
pub fn wire_key_state__method__Coordinator(that: JsValue) -> support::WireSyncReturn {
    wire_key_state__method__Coordinator_impl(that)
}

#[wasm_bindgen]
pub fn wire_sub_key_events__method__Coordinator(port_: MessagePort, that: JsValue) {
    wire_sub_key_events__method__Coordinator_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_get_key__method__Coordinator(
    that: JsValue,
    key_id: JsValue,
) -> support::WireSyncReturn {
    wire_get_key__method__Coordinator_impl(that, key_id)
}

#[wasm_bindgen]
pub fn wire_get_key_name__method__Coordinator(
    that: JsValue,
    key_id: JsValue,
) -> support::WireSyncReturn {
    wire_get_key_name__method__Coordinator_impl(that, key_id)
}

#[wasm_bindgen]
pub fn wire_keys_for_device__method__Coordinator(
    that: JsValue,
    device_id: JsValue,
) -> support::WireSyncReturn {
    wire_keys_for_device__method__Coordinator_impl(that, device_id)
}

#[wasm_bindgen]
pub fn wire_start_signing__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    key_id: JsValue,
    devices: JsValue,
    message: String,
) {
    wire_start_signing__method__Coordinator_impl(port_, that, key_id, devices, message)
}

#[wasm_bindgen]
pub fn wire_start_signing_tx__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    key_id: JsValue,
    unsigned_tx: JsValue,
    devices: JsValue,
) {
    wire_start_signing_tx__method__Coordinator_impl(port_, that, key_id, unsigned_tx, devices)
}

#[wasm_bindgen]
pub fn wire_nonces_available__method__Coordinator(
    that: JsValue,
    id: JsValue,
) -> support::WireSyncReturn {
    wire_nonces_available__method__Coordinator_impl(that, id)
}

#[wasm_bindgen]
pub fn wire_current_nonce__method__Coordinator(
    that: JsValue,
    id: JsValue,
) -> support::WireSyncReturn {
    wire_current_nonce__method__Coordinator_impl(that, id)
}

#[wasm_bindgen]
pub fn wire_generate_new_key__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    threshold: u16,
    devices: JsValue,
    key_name: String,
) {
    wire_generate_new_key__method__Coordinator_impl(port_, that, threshold, devices, key_name)
}

#[wasm_bindgen]
pub fn wire_persisted_sign_session_description__method__Coordinator(
    that: JsValue,
    key_id: JsValue,
) -> support::WireSyncReturn {
    wire_persisted_sign_session_description__method__Coordinator_impl(that, key_id)
}

#[wasm_bindgen]
pub fn wire_try_restore_signing_session__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    key_id: JsValue,
) {
    wire_try_restore_signing_session__method__Coordinator_impl(port_, that, key_id)
}

#[wasm_bindgen]
pub fn wire_start_firmware_upgrade__method__Coordinator(port_: MessagePort, that: JsValue) {
    wire_start_firmware_upgrade__method__Coordinator_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_upgrade_firmware_digest__method__Coordinator(that: JsValue) -> support::WireSyncReturn {
    wire_upgrade_firmware_digest__method__Coordinator_impl(that)
}

#[wasm_bindgen]
pub fn wire_cancel_protocol__method__Coordinator(port_: MessagePort, that: JsValue) {
    wire_cancel_protocol__method__Coordinator_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_enter_firmware_upgrade_mode__method__Coordinator(port_: MessagePort, that: JsValue) {
    wire_enter_firmware_upgrade_mode__method__Coordinator_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_get_device_name__method__Coordinator(
    that: JsValue,
    id: JsValue,
) -> support::WireSyncReturn {
    wire_get_device_name__method__Coordinator_impl(that, id)
}

#[wasm_bindgen]
pub fn wire_final_keygen_ack__method__Coordinator(port_: MessagePort, that: JsValue) {
    wire_final_keygen_ack__method__Coordinator_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_check_share_on_device__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    device_id: JsValue,
    key_id: JsValue,
) {
    wire_check_share_on_device__method__Coordinator_impl(port_, that, device_id, key_id)
}

#[wasm_bindgen]
pub fn wire_effect__method__SignedTx(
    that: JsValue,
    key_id: JsValue,
    network: JsValue,
) -> support::WireSyncReturn {
    wire_effect__method__SignedTx_impl(that, key_id, network)
}

#[wasm_bindgen]
pub fn wire_attach_signatures_to_psbt__method__UnsignedTx(
    port_: MessagePort,
    that: JsValue,
    signatures: JsValue,
    psbt: JsValue,
) {
    wire_attach_signatures_to_psbt__method__UnsignedTx_impl(port_, that, signatures, psbt)
}

#[wasm_bindgen]
pub fn wire_complete__method__UnsignedTx(port_: MessagePort, that: JsValue, signatures: JsValue) {
    wire_complete__method__UnsignedTx_impl(port_, that, signatures)
}

#[wasm_bindgen]
pub fn wire_effect__method__UnsignedTx(
    that: JsValue,
    key_id: JsValue,
    network: JsValue,
) -> support::WireSyncReturn {
    wire_effect__method__UnsignedTx_impl(that, key_id, network)
}

#[wasm_bindgen]
pub fn wire_to_bytes__method__Psbt(that: JsValue) -> support::WireSyncReturn {
    wire_to_bytes__method__Psbt_impl(that)
}

#[wasm_bindgen]
pub fn wire_decode_from_bytes__method__QrReader(
    port_: MessagePort,
    that: JsValue,
    bytes: Box<[u8]>,
) {
    wire_decode_from_bytes__method__QrReader_impl(port_, that, bytes)
}

#[wasm_bindgen]
pub fn wire_next__method__QrEncoder(that: JsValue) -> support::WireSyncReturn {
    wire_next__method__QrEncoder_impl(that)
}

#[wasm_bindgen]
pub fn wire_sub_developer_settings__method__Settings(port_: MessagePort, that: JsValue) {
    wire_sub_developer_settings__method__Settings_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_sub_electrum_settings__method__Settings(port_: MessagePort, that: JsValue) {
    wire_sub_electrum_settings__method__Settings_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_sub_wallet_settings__method__Settings(port_: MessagePort, that: JsValue) {
    wire_sub_wallet_settings__method__Settings_impl(port_, that)
}

#[wasm_bindgen]
pub fn wire_load_wallet__method__Settings(port_: MessagePort, that: JsValue, network: JsValue) {
    wire_load_wallet__method__Settings_impl(port_, that, network)
}

#[wasm_bindgen]
pub fn wire_set_wallet_network__method__Settings(
    port_: MessagePort,
    that: JsValue,
    key_id: JsValue,
    network: JsValue,
) {
    wire_set_wallet_network__method__Settings_impl(port_, that, key_id, network)
}

#[wasm_bindgen]
pub fn wire_set_developer_mode__method__Settings(port_: MessagePort, that: JsValue, value: bool) {
    wire_set_developer_mode__method__Settings_impl(port_, that, value)
}

#[wasm_bindgen]
pub fn wire_check_and_set_electrum_server__method__Settings(
    port_: MessagePort,
    that: JsValue,
    network: JsValue,
    url: String,
) {
    wire_check_and_set_electrum_server__method__Settings_impl(port_, that, network, url)
}

#[wasm_bindgen]
pub fn wire_subscribe_chain_status__method__Settings(
    port_: MessagePort,
    that: JsValue,
    network: JsValue,
) {
    wire_subscribe_chain_status__method__Settings_impl(port_, that, network)
}

// Section: allocate functions

// Section: related functions

#[wasm_bindgen]
pub fn drop_opaque_ArcMutexFrostsnapWallet(ptr: *const c_void) {
    unsafe {
        Arc::<Arc<Mutex<FrostsnapWallet>>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_ArcMutexFrostsnapWallet(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Arc<Mutex<FrostsnapWallet>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_ArcMutexRusqliteConnection(ptr: *const c_void) {
    unsafe {
        Arc::<Arc<Mutex<rusqlite::Connection>>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_ArcMutexRusqliteConnection(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Arc<Mutex<rusqlite::Connection>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_ArcMutexVecPortDesc(ptr: *const c_void) {
    unsafe {
        Arc::<Arc<Mutex<Vec<PortDesc>>>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_ArcMutexVecPortDesc(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Arc<Mutex<Vec<PortDesc>>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_ArcRTransaction(ptr: *const c_void) {
    unsafe {
        Arc::<Arc<RTransaction>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_ArcRTransaction(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Arc<RTransaction>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_ArcWalletStreams(ptr: *const c_void) {
    unsafe {
        Arc::<Arc<WalletStreams>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_ArcWalletStreams(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Arc<WalletStreams>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_BitcoinPsbt(ptr: *const c_void) {
    unsafe {
        Arc::<BitcoinPsbt>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_BitcoinPsbt(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<BitcoinPsbt>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_ChainClient(ptr: *const c_void) {
    unsafe {
        Arc::<ChainClient>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_ChainClient(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<ChainClient>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_FfiCoordinator(ptr: *const c_void) {
    unsafe {
        Arc::<FfiCoordinator>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_FfiCoordinator(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<FfiCoordinator>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_FfiQrEncoder(ptr: *const c_void) {
    unsafe {
        Arc::<FfiQrEncoder>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_FfiQrEncoder(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<FfiQrEncoder>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_FfiQrReader(ptr: *const c_void) {
    unsafe {
        Arc::<FfiQrReader>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_FfiQrReader(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<FfiQrReader>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate(ptr: *const c_void) {
    unsafe {
        Arc::<frostsnap_core::bitcoin_transaction::TransactionTemplate>::decrement_strong_count(
            ptr as _,
        );
    }
}

#[wasm_bindgen]
pub fn share_opaque_FrostsnapCoreBitcoinTransactionTransactionTemplate(
    ptr: *const c_void,
) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::bitcoin_transaction::TransactionTemplate>::increment_strong_count(
            ptr as _,
        );
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey(ptr: *const c_void) {
    unsafe {
        Arc::<frostsnap_core::coordinator::CoordinatorFrostKey>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_FrostsnapCoreCoordinatorCoordinatorFrostKey(
    ptr: *const c_void,
) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::coordinator::CoordinatorFrostKey>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_HashMapRBitcoinNetworkChainClient(ptr: *const c_void) {
    unsafe {
        Arc::<HashMap<RBitcoinNetwork, ChainClient>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_HashMapRBitcoinNetworkChainClient(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<HashMap<RBitcoinNetwork, ChainClient>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_MaybeSinkDeveloperSettings(ptr: *const c_void) {
    unsafe {
        Arc::<MaybeSink<DeveloperSettings>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_MaybeSinkDeveloperSettings(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<MaybeSink<DeveloperSettings>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_MaybeSinkElectrumSettings(ptr: *const c_void) {
    unsafe {
        Arc::<MaybeSink<ElectrumSettings>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_MaybeSinkElectrumSettings(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<MaybeSink<ElectrumSettings>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_MaybeSinkWalletSettings(ptr: *const c_void) {
    unsafe {
        Arc::<MaybeSink<WalletSettings>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_MaybeSinkWalletSettings(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<MaybeSink<WalletSettings>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_MutexHashMapRBitcoinNetworkWallet(ptr: *const c_void) {
    unsafe {
        Arc::<Mutex<HashMap<RBitcoinNetwork, Wallet>>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_MutexHashMapRBitcoinNetworkWallet(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Mutex<HashMap<RBitcoinNetwork, Wallet>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_MutexPersistedRSettings(ptr: *const c_void) {
    unsafe {
        Arc::<Mutex<Persisted<RSettings>>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_MutexPersistedRSettings(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Mutex<Persisted<RSettings>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_PathBuf(ptr: *const c_void) {
    unsafe {
        Arc::<PathBuf>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_PathBuf(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PathBuf>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_PortBytesToReadSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortBytesToReadSender>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_PortBytesToReadSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortBytesToReadSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_PortOpenSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortOpenSender>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_PortOpenSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortOpenSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_PortReadSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortReadSender>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_PortReadSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortReadSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_PortWriteSender(ptr: *const c_void) {
    unsafe {
        Arc::<PortWriteSender>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_PortWriteSender(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<PortWriteSender>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_RBitcoinNetwork(ptr: *const c_void) {
    unsafe {
        Arc::<RBitcoinNetwork>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_RBitcoinNetwork(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<RBitcoinNetwork>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_RTransaction(ptr: *const c_void) {
    unsafe {
        Arc::<RTransaction>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_RTransaction(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<RTransaction>::increment_strong_count(ptr as _);
        ptr
    }
}

// Section: impl Wire2Api

impl Wire2Api<String> for String {
    fn wire2api(self) -> String {
        self
    }
}
impl Wire2Api<Vec<String>> for JsValue {
    fn wire2api(self) -> Vec<String> {
        self.dyn_into::<JsArray>()
            .unwrap()
            .iter()
            .map(Wire2Api::wire2api)
            .collect()
    }
}
impl Wire2Api<BitcoinNetwork> for JsValue {
    fn wire2api(self) -> BitcoinNetwork {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        BitcoinNetwork(self_.get(0).wire2api())
    }
}

impl Wire2Api<ConfirmationTime> for JsValue {
    fn wire2api(self) -> ConfirmationTime {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            2,
            "Expected 2 elements, got {}",
            self_.length()
        );
        ConfirmationTime {
            height: self_.get(0).wire2api(),
            time: self_.get(1).wire2api(),
        }
    }
}
impl Wire2Api<ConnectedDevice> for JsValue {
    fn wire2api(self) -> ConnectedDevice {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            4,
            "Expected 4 elements, got {}",
            self_.length()
        );
        ConnectedDevice {
            name: self_.get(0).wire2api(),
            firmware_digest: self_.get(1).wire2api(),
            latest_digest: self_.get(2).wire2api(),
            id: self_.get(3).wire2api(),
        }
    }
}
impl Wire2Api<Coordinator> for JsValue {
    fn wire2api(self) -> Coordinator {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        Coordinator(self_.get(0).wire2api())
    }
}
impl Wire2Api<DeviceId> for JsValue {
    fn wire2api(self) -> DeviceId {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        DeviceId(self_.get(0).wire2api())
    }
}
impl Wire2Api<DeviceListState> for JsValue {
    fn wire2api(self) -> DeviceListState {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            2,
            "Expected 2 elements, got {}",
            self_.length()
        );
        DeviceListState {
            devices: self_.get(0).wire2api(),
            state_id: self_.get(1).wire2api(),
        }
    }
}
impl Wire2Api<EncodedSignature> for JsValue {
    fn wire2api(self) -> EncodedSignature {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        EncodedSignature(self_.get(0).wire2api())
    }
}

impl Wire2Api<FfiSerial> for JsValue {
    fn wire2api(self) -> FfiSerial {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        FfiSerial {
            available_ports: self_.get(0).wire2api(),
        }
    }
}
impl Wire2Api<FrostKey> for JsValue {
    fn wire2api(self) -> FrostKey {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        FrostKey(self_.get(0).wire2api())
    }
}

impl Wire2Api<KeyId> for JsValue {
    fn wire2api(self) -> KeyId {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        KeyId(self_.get(0).wire2api())
    }
}
impl Wire2Api<Vec<ConnectedDevice>> for JsValue {
    fn wire2api(self) -> Vec<ConnectedDevice> {
        self.dyn_into::<JsArray>()
            .unwrap()
            .iter()
            .map(Wire2Api::wire2api)
            .collect()
    }
}
impl Wire2Api<Vec<DeviceId>> for JsValue {
    fn wire2api(self) -> Vec<DeviceId> {
        self.dyn_into::<JsArray>()
            .unwrap()
            .iter()
            .map(Wire2Api::wire2api)
            .collect()
    }
}
impl Wire2Api<Vec<EncodedSignature>> for JsValue {
    fn wire2api(self) -> Vec<EncodedSignature> {
        self.dyn_into::<JsArray>()
            .unwrap()
            .iter()
            .map(Wire2Api::wire2api)
            .collect()
    }
}
impl Wire2Api<Vec<PortDesc>> for JsValue {
    fn wire2api(self) -> Vec<PortDesc> {
        self.dyn_into::<JsArray>()
            .unwrap()
            .iter()
            .map(Wire2Api::wire2api)
            .collect()
    }
}

impl Wire2Api<Option<String>> for Option<String> {
    fn wire2api(self) -> Option<String> {
        self.map(Wire2Api::wire2api)
    }
}

impl Wire2Api<PortBytesToRead> for JsValue {
    fn wire2api(self) -> PortBytesToRead {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            2,
            "Expected 2 elements, got {}",
            self_.length()
        );
        PortBytesToRead {
            id: self_.get(0).wire2api(),
            ready: self_.get(1).wire2api(),
        }
    }
}
impl Wire2Api<PortDesc> for JsValue {
    fn wire2api(self) -> PortDesc {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            3,
            "Expected 3 elements, got {}",
            self_.length()
        );
        PortDesc {
            id: self_.get(0).wire2api(),
            vid: self_.get(1).wire2api(),
            pid: self_.get(2).wire2api(),
        }
    }
}
impl Wire2Api<PortOpen> for JsValue {
    fn wire2api(self) -> PortOpen {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            3,
            "Expected 3 elements, got {}",
            self_.length()
        );
        PortOpen {
            id: self_.get(0).wire2api(),
            baud_rate: self_.get(1).wire2api(),
            ready: self_.get(2).wire2api(),
        }
    }
}
impl Wire2Api<PortRead> for JsValue {
    fn wire2api(self) -> PortRead {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            3,
            "Expected 3 elements, got {}",
            self_.length()
        );
        PortRead {
            id: self_.get(0).wire2api(),
            len: self_.get(1).wire2api(),
            ready: self_.get(2).wire2api(),
        }
    }
}
impl Wire2Api<PortWrite> for JsValue {
    fn wire2api(self) -> PortWrite {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            3,
            "Expected 3 elements, got {}",
            self_.length()
        );
        PortWrite {
            id: self_.get(0).wire2api(),
            bytes: self_.get(1).wire2api(),
            ready: self_.get(2).wire2api(),
        }
    }
}
impl Wire2Api<Psbt> for JsValue {
    fn wire2api(self) -> Psbt {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        Psbt {
            inner: self_.get(0).wire2api(),
        }
    }
}
impl Wire2Api<QrEncoder> for JsValue {
    fn wire2api(self) -> QrEncoder {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        QrEncoder(self_.get(0).wire2api())
    }
}
impl Wire2Api<QrReader> for JsValue {
    fn wire2api(self) -> QrReader {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        QrReader(self_.get(0).wire2api())
    }
}
impl Wire2Api<Settings> for JsValue {
    fn wire2api(self) -> Settings {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            8,
            "Expected 8 elements, got {}",
            self_.length()
        );
        Settings {
            settings: self_.get(0).wire2api(),
            db: self_.get(1).wire2api(),
            chain_clients: self_.get(2).wire2api(),
            app_directory: self_.get(3).wire2api(),
            loaded_wallets: self_.get(4).wire2api(),
            wallet_settings_stream: self_.get(5).wire2api(),
            developer_settings_stream: self_.get(6).wire2api(),
            electrum_settings_stream: self_.get(7).wire2api(),
        }
    }
}
impl Wire2Api<SignedTx> for JsValue {
    fn wire2api(self) -> SignedTx {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            2,
            "Expected 2 elements, got {}",
            self_.length()
        );
        SignedTx {
            signed_tx: self_.get(0).wire2api(),
            unsigned_tx: self_.get(1).wire2api(),
        }
    }
}
impl Wire2Api<Transaction> for JsValue {
    fn wire2api(self) -> Transaction {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            3,
            "Expected 3 elements, got {}",
            self_.length()
        );
        Transaction {
            net_value: self_.get(0).wire2api(),
            inner: self_.get(1).wire2api(),
            confirmation_time: self_.get(2).wire2api(),
        }
    }
}

impl Wire2Api<[u8; 33]> for Box<[u8]> {
    fn wire2api(self) -> [u8; 33] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
    }
}
impl Wire2Api<[u8; 64]> for Box<[u8]> {
    fn wire2api(self) -> [u8; 64] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
    }
}
impl Wire2Api<Vec<u8>> for Box<[u8]> {
    fn wire2api(self) -> Vec<u8> {
        self.into_vec()
    }
}
impl Wire2Api<UnsignedTx> for JsValue {
    fn wire2api(self) -> UnsignedTx {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        UnsignedTx {
            template_tx: self_.get(0).wire2api(),
        }
    }
}

impl Wire2Api<Wallet> for JsValue {
    fn wire2api(self) -> Wallet {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            4,
            "Expected 4 elements, got {}",
            self_.length()
        );
        Wallet {
            inner: self_.get(0).wire2api(),
            wallet_streams: self_.get(1).wire2api(),
            chain_sync: self_.get(2).wire2api(),
            network: self_.get(3).wire2api(),
        }
    }
}
// Section: impl Wire2Api for JsValue

impl<T> Wire2Api<Option<T>> for JsValue
where
    JsValue: Wire2Api<T>,
{
    fn wire2api(self) -> Option<T> {
        (!self.is_null() && !self.is_undefined()).then(|| self.wire2api())
    }
}
impl Wire2Api<RustOpaque<Arc<Mutex<FrostsnapWallet>>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Arc<Mutex<FrostsnapWallet>>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Arc<Mutex<rusqlite::Connection>>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Arc<Mutex<rusqlite::Connection>>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Arc<Mutex<Vec<PortDesc>>>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Arc<Mutex<Vec<PortDesc>>>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Arc<RTransaction>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Arc<RTransaction>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Arc<WalletStreams>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Arc<WalletStreams>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<BitcoinPsbt>> for JsValue {
    fn wire2api(self) -> RustOpaque<BitcoinPsbt> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<ChainClient>> for JsValue {
    fn wire2api(self) -> RustOpaque<ChainClient> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<FfiCoordinator>> for JsValue {
    fn wire2api(self) -> RustOpaque<FfiCoordinator> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<FfiQrEncoder>> for JsValue {
    fn wire2api(self) -> RustOpaque<FfiQrEncoder> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<FfiQrReader>> for JsValue {
    fn wire2api(self) -> RustOpaque<FfiQrReader> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<frostsnap_core::bitcoin_transaction::TransactionTemplate>> for JsValue {
    fn wire2api(self) -> RustOpaque<frostsnap_core::bitcoin_transaction::TransactionTemplate> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<frostsnap_core::coordinator::CoordinatorFrostKey>> for JsValue {
    fn wire2api(self) -> RustOpaque<frostsnap_core::coordinator::CoordinatorFrostKey> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<HashMap<RBitcoinNetwork, ChainClient>>> for JsValue {
    fn wire2api(self) -> RustOpaque<HashMap<RBitcoinNetwork, ChainClient>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<MaybeSink<DeveloperSettings>>> for JsValue {
    fn wire2api(self) -> RustOpaque<MaybeSink<DeveloperSettings>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<MaybeSink<ElectrumSettings>>> for JsValue {
    fn wire2api(self) -> RustOpaque<MaybeSink<ElectrumSettings>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<MaybeSink<WalletSettings>>> for JsValue {
    fn wire2api(self) -> RustOpaque<MaybeSink<WalletSettings>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Mutex<HashMap<RBitcoinNetwork, Wallet>>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Mutex<HashMap<RBitcoinNetwork, Wallet>>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Mutex<Persisted<RSettings>>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Mutex<Persisted<RSettings>>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<PathBuf>> for JsValue {
    fn wire2api(self) -> RustOpaque<PathBuf> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<PortBytesToReadSender>> for JsValue {
    fn wire2api(self) -> RustOpaque<PortBytesToReadSender> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<PortOpenSender>> for JsValue {
    fn wire2api(self) -> RustOpaque<PortOpenSender> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<PortReadSender>> for JsValue {
    fn wire2api(self) -> RustOpaque<PortReadSender> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<PortWriteSender>> for JsValue {
    fn wire2api(self) -> RustOpaque<PortWriteSender> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<RBitcoinNetwork>> for JsValue {
    fn wire2api(self) -> RustOpaque<RBitcoinNetwork> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<RTransaction>> for JsValue {
    fn wire2api(self) -> RustOpaque<RTransaction> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<String> for JsValue {
    fn wire2api(self) -> String {
        self.as_string().expect("non-UTF-8 string, or not a string")
    }
}
impl Wire2Api<bool> for JsValue {
    fn wire2api(self) -> bool {
        self.is_truthy()
    }
}
impl Wire2Api<f64> for JsValue {
    fn wire2api(self) -> f64 {
        self.unchecked_into_f64() as _
    }
}
impl Wire2Api<i32> for JsValue {
    fn wire2api(self) -> i32 {
        self.unchecked_into_f64() as _
    }
}
impl Wire2Api<i64> for JsValue {
    fn wire2api(self) -> i64 {
        ::std::convert::TryInto::try_into(self.dyn_into::<js_sys::BigInt>().unwrap()).unwrap()
    }
}
impl Wire2Api<LogLevel> for JsValue {
    fn wire2api(self) -> LogLevel {
        (self.unchecked_into_f64() as i32).wire2api()
    }
}
impl Wire2Api<u16> for JsValue {
    fn wire2api(self) -> u16 {
        self.unchecked_into_f64() as _
    }
}
impl Wire2Api<u32> for JsValue {
    fn wire2api(self) -> u32 {
        self.unchecked_into_f64() as _
    }
}
impl Wire2Api<u64> for JsValue {
    fn wire2api(self) -> u64 {
        ::std::convert::TryInto::try_into(self.dyn_into::<js_sys::BigInt>().unwrap()).unwrap()
    }
}
impl Wire2Api<u8> for JsValue {
    fn wire2api(self) -> u8 {
        self.unchecked_into_f64() as _
    }
}
impl Wire2Api<[u8; 33]> for JsValue {
    fn wire2api(self) -> [u8; 33] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
    }
}
impl Wire2Api<[u8; 64]> for JsValue {
    fn wire2api(self) -> [u8; 64] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
    }
}
impl Wire2Api<Vec<u8>> for JsValue {
    fn wire2api(self) -> Vec<u8> {
        self.unchecked_into::<js_sys::Uint8Array>().to_vec().into()
    }
}
impl Wire2Api<usize> for JsValue {
    fn wire2api(self) -> usize {
        self.unchecked_into_f64() as _
    }
}
