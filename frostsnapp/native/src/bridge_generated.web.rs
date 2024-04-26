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
pub fn wire_turn_stderr_logging_on(port_: MessagePort, level: i32) {
    wire_turn_stderr_logging_on_impl(port_, level)
}

#[wasm_bindgen]
pub fn wire_turn_logcat_logging_on(port_: MessagePort, _level: i32) {
    wire_turn_logcat_logging_on_impl(port_, _level)
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
pub fn wire_get_device(id: JsValue) -> support::WireSyncReturn {
    wire_get_device_impl(id)
}

#[wasm_bindgen]
pub fn wire_load(port_: MessagePort, db_file: String) {
    wire_load_impl(port_, db_file)
}

#[wasm_bindgen]
pub fn wire_load_host_handles_serial(port_: MessagePort, db_file: String) {
    wire_load_host_handles_serial_impl(port_, db_file)
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
pub fn wire_txid__method__Transaction(that: JsValue) -> support::WireSyncReturn {
    wire_txid__method__Transaction_impl(that)
}

#[wasm_bindgen]
pub fn wire_ready__method__Device(that: JsValue) -> support::WireSyncReturn {
    wire_ready__method__Device_impl(that)
}

#[wasm_bindgen]
pub fn wire_needs_firmware_upgrade__method__Device(that: JsValue) -> support::WireSyncReturn {
    wire_needs_firmware_upgrade__method__Device_impl(that)
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
pub fn wire_name__method__FrostKey(that: JsValue) -> support::WireSyncReturn {
    wire_name__method__FrostKey_impl(that)
}

#[wasm_bindgen]
pub fn wire_devices__method__FrostKey(that: JsValue) -> support::WireSyncReturn {
    wire_devices__method__FrostKey_impl(that)
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
pub fn wire_cancel_all__method__Coordinator(port_: MessagePort, that: JsValue) {
    wire_cancel_all__method__Coordinator_impl(port_, that)
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
pub fn wire_generate_new_key__method__Coordinator(
    port_: MessagePort,
    that: JsValue,
    threshold: usize,
    devices: JsValue,
) {
    wire_generate_new_key__method__Coordinator_impl(port_, that, threshold, devices)
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
pub fn wire_validate_destination_address__method__Wallet(
    that: JsValue,
    address: String,
) -> support::WireSyncReturn {
    wire_validate_destination_address__method__Wallet_impl(that, address)
}

#[wasm_bindgen]
pub fn wire_validate_amount__method__Wallet(
    that: JsValue,
    address: String,
    value: u64,
) -> support::WireSyncReturn {
    wire_validate_amount__method__Wallet_impl(that, address, value)
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
pub fn wire_complete_unsigned_psbt__method__Wallet(
    that: JsValue,
    psbt: JsValue,
    signatures: JsValue,
) -> support::WireSyncReturn {
    wire_complete_unsigned_psbt__method__Wallet_impl(that, psbt, signatures)
}

#[wasm_bindgen]
pub fn wire_complete_unsigned_tx__method__Wallet(
    that: JsValue,
    unsigned_tx: JsValue,
    signatures: JsValue,
) -> support::WireSyncReturn {
    wire_complete_unsigned_tx__method__Wallet_impl(that, unsigned_tx, signatures)
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
pub fn wire_effect_of_tx__method__Wallet(
    that: JsValue,
    key_id: JsValue,
    tx: JsValue,
) -> support::WireSyncReturn {
    wire_effect_of_tx__method__Wallet_impl(that, key_id, tx)
}

#[wasm_bindgen]
pub fn wire_effect_of_psbt_tx__method__Wallet(
    that: JsValue,
    key_id: JsValue,
    psbt: JsValue,
) -> support::WireSyncReturn {
    wire_effect_of_psbt_tx__method__Wallet_impl(that, key_id, psbt)
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
pub fn wire_descriptor_for_key__method__Wallet(
    that: JsValue,
    key_id: JsValue,
) -> support::WireSyncReturn {
    wire_descriptor_for_key__method__Wallet_impl(that, key_id)
}

#[wasm_bindgen]
pub fn wire_tx__method__SignedTx(that: JsValue) -> support::WireSyncReturn {
    wire_tx__method__SignedTx_impl(that)
}

#[wasm_bindgen]
pub fn wire_tx__method__UnsignedTx(that: JsValue) -> support::WireSyncReturn {
    wire_tx__method__UnsignedTx_impl(that)
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

// Section: allocate functions

// Section: related functions

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
pub fn drop_opaque_ChainSync(ptr: *const c_void) {
    unsafe {
        Arc::<ChainSync>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_ChainSync(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<ChainSync>::increment_strong_count(ptr as _);
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
pub fn drop_opaque_FrostsnapCoreCoordinatorFrostKey(ptr: *const c_void) {
    unsafe {
        Arc::<frostsnap_core::CoordinatorFrostKey>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_FrostsnapCoreCoordinatorFrostKey(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::CoordinatorFrostKey>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(ptr: *const c_void) {
    unsafe {
        Arc::<frostsnap_core::message::BitcoinTransactionSignTask>::decrement_strong_count(
            ptr as _,
        );
    }
}

#[wasm_bindgen]
pub fn share_opaque_FrostsnapCoreMessageBitcoinTransactionSignTask(
    ptr: *const c_void,
) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::message::BitcoinTransactionSignTask>::increment_strong_count(
            ptr as _,
        );
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr: *const c_void) {
    unsafe {
        Arc::<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_MutexBTreeMapKeyIdStreamSinkTxState(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>::increment_strong_count(ptr as _);
        ptr
    }
}

#[wasm_bindgen]
pub fn drop_opaque_MutexCrateWalletWallet(ptr: *const c_void) {
    unsafe {
        Arc::<Mutex<crate::wallet::_Wallet>>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_MutexCrateWalletWallet(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<Mutex<crate::wallet::_Wallet>>::increment_strong_count(ptr as _);
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
impl Wire2Api<Device> for JsValue {
    fn wire2api(self) -> Device {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            4,
            "Expected 4 elements, got {}",
            self_.length()
        );
        Device {
            name: self_.get(0).wire2api(),
            firmware_digest: self_.get(1).wire2api(),
            latest_digest: self_.get(2).wire2api(),
            id: self_.get(3).wire2api(),
        }
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

impl Wire2Api<Vec<Device>> for JsValue {
    fn wire2api(self) -> Vec<Device> {
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
impl Wire2Api<SignedTx> for JsValue {
    fn wire2api(self) -> SignedTx {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        SignedTx {
            inner: self_.get(0).wire2api(),
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

impl Wire2Api<[u8; 32]> for Box<[u8]> {
    fn wire2api(self) -> [u8; 32] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
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
            task: self_.get(0).wire2api(),
        }
    }
}

impl Wire2Api<Wallet> for JsValue {
    fn wire2api(self) -> Wallet {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            3,
            "Expected 3 elements, got {}",
            self_.length()
        );
        Wallet {
            inner: self_.get(0).wire2api(),
            wallet_streams: self_.get(1).wire2api(),
            chain_sync: self_.get(2).wire2api(),
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
impl Wire2Api<RustOpaque<Arc<Mutex<Vec<PortDesc>>>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Arc<Mutex<Vec<PortDesc>>>> {
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
impl Wire2Api<RustOpaque<ChainSync>> for JsValue {
    fn wire2api(self) -> RustOpaque<ChainSync> {
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
impl Wire2Api<RustOpaque<FfiQrReader>> for JsValue {
    fn wire2api(self) -> RustOpaque<FfiQrReader> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<frostsnap_core::CoordinatorFrostKey>> for JsValue {
    fn wire2api(self) -> RustOpaque<frostsnap_core::CoordinatorFrostKey> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<frostsnap_core::message::BitcoinTransactionSignTask>> for JsValue {
    fn wire2api(self) -> RustOpaque<frostsnap_core::message::BitcoinTransactionSignTask> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Mutex<BTreeMap<KeyId, StreamSink<TxState>>>> {
        #[cfg(target_pointer_width = "64")]
        {
            compile_error!("64-bit pointers are not supported.");
        }

        unsafe { support::opaque_from_dart((self.as_f64().unwrap() as usize) as _) }
    }
}
impl Wire2Api<RustOpaque<Mutex<crate::wallet::_Wallet>>> for JsValue {
    fn wire2api(self) -> RustOpaque<Mutex<crate::wallet::_Wallet>> {
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
impl Wire2Api<Level> for JsValue {
    fn wire2api(self) -> Level {
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
impl Wire2Api<[u8; 32]> for JsValue {
    fn wire2api(self) -> [u8; 32] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
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
