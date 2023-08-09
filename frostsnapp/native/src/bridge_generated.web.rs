use super::*;
// Section: wire functions

#[wasm_bindgen]
pub fn wire_init_events(port_: MessagePort) {
    wire_init_events_impl(port_)
}

#[wasm_bindgen]
pub fn wire_sub_device_events(port_: MessagePort) {
    wire_sub_device_events_impl(port_)
}

#[wasm_bindgen]
pub fn wire_new_ffi_coordinator(port_: MessagePort, host_handles_serial: bool) {
    wire_new_ffi_coordinator_impl(port_, host_handles_serial)
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
pub fn wire_announce_available_ports(port_: MessagePort, coordinator: JsValue, ports: JsValue) {
    wire_announce_available_ports_impl(port_, coordinator, ports)
}

#[wasm_bindgen]
pub fn wire_set_device_label(
    port_: MessagePort,
    coordinator: JsValue,
    device_id: String,
    label: String,
) {
    wire_set_device_label_impl(port_, coordinator, device_id, label)
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

// Section: allocate functions

// Section: related functions

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

impl Wire2Api<Vec<u8>> for Box<[u8]> {
    fn wire2api(self) -> Vec<u8> {
        self.into_vec()
    }
}

// Section: impl Wire2Api for JsValue

impl Wire2Api<RustOpaque<FfiCoordinator>> for JsValue {
    fn wire2api(self) -> RustOpaque<FfiCoordinator> {
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
impl Wire2Api<bool> for JsValue {
    fn wire2api(self) -> bool {
        self.is_truthy()
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
impl Wire2Api<Option<String>> for JsValue {
    fn wire2api(self) -> Option<String> {
        (!self.is_undefined() && !self.is_null()).then(|| self.wire2api())
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