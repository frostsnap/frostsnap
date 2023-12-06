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
pub fn wire_sub_key_events(port_: MessagePort) {
    wire_sub_key_events_impl(port_)
}

#[wasm_bindgen]
pub fn wire_emit_key_event(port_: MessagePort, event: JsValue) {
    wire_emit_key_event_impl(port_, event)
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
pub fn wire_announce_available_ports(port_: MessagePort, ports: JsValue) {
    wire_announce_available_ports_impl(port_, ports)
}

#[wasm_bindgen]
pub fn wire_switch_to_host_handles_serial(port_: MessagePort) {
    wire_switch_to_host_handles_serial_impl(port_)
}

#[wasm_bindgen]
pub fn wire_update_name_preview(port_: MessagePort, id: JsValue, name: String) {
    wire_update_name_preview_impl(port_, id, name)
}

#[wasm_bindgen]
pub fn wire_finish_naming(port_: MessagePort, id: JsValue, name: String) {
    wire_finish_naming_impl(port_, id, name)
}

#[wasm_bindgen]
pub fn wire_send_cancel(port_: MessagePort, id: JsValue) {
    wire_send_cancel_impl(port_, id)
}

#[wasm_bindgen]
pub fn wire_cancel_all(port_: MessagePort) {
    wire_cancel_all_impl(port_)
}

#[wasm_bindgen]
pub fn wire_registered_devices(port_: MessagePort) {
    wire_registered_devices_impl(port_)
}

#[wasm_bindgen]
pub fn wire_start_coordinator_thread(port_: MessagePort) {
    wire_start_coordinator_thread_impl(port_)
}

#[wasm_bindgen]
pub fn wire_key_state() -> support::WireSyncReturn {
    wire_key_state_impl()
}

#[wasm_bindgen]
pub fn wire_get_key(key_id: JsValue) -> support::WireSyncReturn {
    wire_get_key_impl(key_id)
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
pub fn wire_start_signing(port_: MessagePort, key_id: JsValue, devices: JsValue, message: String) {
    wire_start_signing_impl(port_, key_id, devices, message)
}

#[wasm_bindgen]
pub fn wire_generate_new_key(port_: MessagePort, threshold: usize, devices: JsValue) {
    wire_generate_new_key_impl(port_, threshold, devices)
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
pub fn wire_named_devices__method__DeviceListState(that: JsValue) -> support::WireSyncReturn {
    wire_named_devices__method__DeviceListState_impl(that)
}

// Section: allocate functions

// Section: related functions

#[wasm_bindgen]
pub fn drop_opaque_FrostsnapCoreCoordinatorFrostKeyState(ptr: *const c_void) {
    unsafe {
        Arc::<frostsnap_core::CoordinatorFrostKeyState>::decrement_strong_count(ptr as _);
    }
}

#[wasm_bindgen]
pub fn share_opaque_FrostsnapCoreCoordinatorFrostKeyState(ptr: *const c_void) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::CoordinatorFrostKeyState>::increment_strong_count(ptr as _);
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

// Section: impl Wire2Api

impl Wire2Api<String> for String {
    fn wire2api(self) -> String {
        self
    }
}

impl Wire2Api<Device> for JsValue {
    fn wire2api(self) -> Device {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            2,
            "Expected 2 elements, got {}",
            self_.length()
        );
        Device {
            name: self_.get(0).wire2api(),
            id: self_.get(1).wire2api(),
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
impl Wire2Api<KeyState> for JsValue {
    fn wire2api(self) -> KeyState {
        let self_ = self.dyn_into::<JsArray>().unwrap();
        assert_eq!(
            self_.length(),
            1,
            "Expected 1 elements, got {}",
            self_.length()
        );
        KeyState {
            keys: self_.get(0).wire2api(),
        }
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
impl Wire2Api<Vec<FrostKey>> for JsValue {
    fn wire2api(self) -> Vec<FrostKey> {
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
impl Wire2Api<Vec<u8>> for Box<[u8]> {
    fn wire2api(self) -> Vec<u8> {
        self.into_vec()
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
impl Wire2Api<RustOpaque<frostsnap_core::CoordinatorFrostKeyState>> for JsValue {
    fn wire2api(self) -> RustOpaque<frostsnap_core::CoordinatorFrostKeyState> {
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
impl Wire2Api<String> for JsValue {
    fn wire2api(self) -> String {
        self.as_string().expect("non-UTF-8 string, or not a string")
    }
}
impl Wire2Api<i32> for JsValue {
    fn wire2api(self) -> i32 {
        self.unchecked_into_f64() as _
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
