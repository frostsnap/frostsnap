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
pub extern "C" fn wire_sub_key_events(port_: i64) {
    wire_sub_key_events_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_emit_key_event(port_: i64, event: *mut wire_KeyState) {
    wire_emit_key_event_impl(port_, event)
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
pub extern "C" fn wire_announce_available_ports(port_: i64, ports: *mut wire_list_port_desc) {
    wire_announce_available_ports_impl(port_, ports)
}

#[no_mangle]
pub extern "C" fn wire_switch_to_host_handles_serial(port_: i64) {
    wire_switch_to_host_handles_serial_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_update_name_preview(
    port_: i64,
    id: *mut wire_DeviceId,
    name: *mut wire_uint_8_list,
) {
    wire_update_name_preview_impl(port_, id, name)
}

#[no_mangle]
pub extern "C" fn wire_finish_naming(
    port_: i64,
    id: *mut wire_DeviceId,
    name: *mut wire_uint_8_list,
) {
    wire_finish_naming_impl(port_, id, name)
}

#[no_mangle]
pub extern "C" fn wire_send_cancel(port_: i64, id: *mut wire_DeviceId) {
    wire_send_cancel_impl(port_, id)
}

#[no_mangle]
pub extern "C" fn wire_cancel_all(port_: i64) {
    wire_cancel_all_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_registered_devices(port_: i64) {
    wire_registered_devices_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_start_coordinator_thread(port_: i64) {
    wire_start_coordinator_thread_impl(port_)
}

#[no_mangle]
pub extern "C" fn wire_key_state() -> support::WireSyncReturn {
    wire_key_state_impl()
}

#[no_mangle]
pub extern "C" fn wire_get_key(key_id: *mut wire_KeyId) -> support::WireSyncReturn {
    wire_get_key_impl(key_id)
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
pub extern "C" fn wire_start_signing(
    port_: i64,
    key_id: *mut wire_KeyId,
    devices: *mut wire_list_device_id,
    message: *mut wire_uint_8_list,
) {
    wire_start_signing_impl(port_, key_id, devices, message)
}

#[no_mangle]
pub extern "C" fn wire_generate_new_key(
    port_: i64,
    threshold: usize,
    devices: *mut wire_list_device_id,
) {
    wire_generate_new_key_impl(port_, threshold, devices)
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
pub extern "C" fn wire_named_devices__method__DeviceListState(
    that: *mut wire_DeviceListState,
) -> support::WireSyncReturn {
    wire_named_devices__method__DeviceListState_impl(that)
}

// Section: allocate functions

#[no_mangle]
pub extern "C" fn new_FrostsnapCoreCoordinatorFrostKeyState(
) -> wire_FrostsnapCoreCoordinatorFrostKeyState {
    wire_FrostsnapCoreCoordinatorFrostKeyState::new_with_null_ptr()
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
pub extern "C" fn new_box_autoadd_device_id_0() -> *mut wire_DeviceId {
    support::new_leak_box_ptr(wire_DeviceId::new_with_null_ptr())
}

#[no_mangle]
pub extern "C" fn new_box_autoadd_device_list_state_0() -> *mut wire_DeviceListState {
    support::new_leak_box_ptr(wire_DeviceListState::new_with_null_ptr())
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
pub extern "C" fn new_box_autoadd_key_state_0() -> *mut wire_KeyState {
    support::new_leak_box_ptr(wire_KeyState::new_with_null_ptr())
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
pub extern "C" fn new_list_device_0(len: i32) -> *mut wire_list_device {
    let wrap = wire_list_device {
        ptr: support::new_leak_vec_ptr(<wire_Device>::new_with_null_ptr(), len),
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
pub extern "C" fn new_list_frost_key_0(len: i32) -> *mut wire_list_frost_key {
    let wrap = wire_list_frost_key {
        ptr: support::new_leak_vec_ptr(<wire_FrostKey>::new_with_null_ptr(), len),
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
pub extern "C" fn drop_opaque_FrostsnapCoreCoordinatorFrostKeyState(ptr: *const c_void) {
    unsafe {
        Arc::<frostsnap_core::CoordinatorFrostKeyState>::decrement_strong_count(ptr as _);
    }
}

#[no_mangle]
pub extern "C" fn share_opaque_FrostsnapCoreCoordinatorFrostKeyState(
    ptr: *const c_void,
) -> *const c_void {
    unsafe {
        Arc::<frostsnap_core::CoordinatorFrostKeyState>::increment_strong_count(ptr as _);
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

// Section: impl Wire2Api

impl Wire2Api<RustOpaque<frostsnap_core::CoordinatorFrostKeyState>>
    for wire_FrostsnapCoreCoordinatorFrostKeyState
{
    fn wire2api(self) -> RustOpaque<frostsnap_core::CoordinatorFrostKeyState> {
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
impl Wire2Api<String> for *mut wire_uint_8_list {
    fn wire2api(self) -> String {
        let vec: Vec<u8> = self.wire2api();
        String::from_utf8_lossy(&vec).into_owned()
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
impl Wire2Api<KeyState> for *mut wire_KeyState {
    fn wire2api(self) -> KeyState {
        let wrap = unsafe { support::box_from_leak_ptr(self) };
        Wire2Api::<KeyState>::wire2api(*wrap).into()
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
impl Wire2Api<Device> for wire_Device {
    fn wire2api(self) -> Device {
        Device {
            name: self.name.wire2api(),
            id: self.id.wire2api(),
        }
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
impl Wire2Api<KeyState> for wire_KeyState {
    fn wire2api(self) -> KeyState {
        KeyState {
            keys: self.keys.wire2api(),
        }
    }
}

impl Wire2Api<Vec<Device>> for *mut wire_list_device {
    fn wire2api(self) -> Vec<Device> {
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
impl Wire2Api<Vec<FrostKey>> for *mut wire_list_frost_key {
    fn wire2api(self) -> Vec<FrostKey> {
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

impl Wire2Api<[u8; 32]> for *mut wire_uint_8_list {
    fn wire2api(self) -> [u8; 32] {
        let vec: Vec<u8> = self.wire2api();
        support::from_vec_to_array(vec)
    }
}
impl Wire2Api<[u8; 33]> for *mut wire_uint_8_list {
    fn wire2api(self) -> [u8; 33] {
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

// Section: wire structs

#[repr(C)]
#[derive(Clone)]
pub struct wire_FrostsnapCoreCoordinatorFrostKeyState {
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
pub struct wire_Device {
    name: *mut wire_uint_8_list,
    id: wire_DeviceId,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_DeviceId {
    field0: *mut wire_uint_8_list,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_DeviceListState {
    devices: *mut wire_list_device,
    state_id: usize,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_FrostKey {
    field0: wire_FrostsnapCoreCoordinatorFrostKeyState,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_KeyId {
    field0: *mut wire_uint_8_list,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_KeyState {
    keys: *mut wire_list_frost_key,
}

#[repr(C)]
#[derive(Clone)]
pub struct wire_list_device {
    ptr: *mut wire_Device,
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
pub struct wire_list_frost_key {
    ptr: *mut wire_FrostKey,
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
pub struct wire_uint_8_list {
    ptr: *mut u8,
    len: i32,
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

impl NewWithNullPtr for wire_FrostsnapCoreCoordinatorFrostKeyState {
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

impl NewWithNullPtr for wire_Device {
    fn new_with_null_ptr() -> Self {
        Self {
            name: core::ptr::null_mut(),
            id: Default::default(),
        }
    }
}

impl Default for wire_Device {
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

impl NewWithNullPtr for wire_FrostKey {
    fn new_with_null_ptr() -> Self {
        Self {
            field0: wire_FrostsnapCoreCoordinatorFrostKeyState::new_with_null_ptr(),
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

impl NewWithNullPtr for wire_KeyState {
    fn new_with_null_ptr() -> Self {
        Self {
            keys: core::ptr::null_mut(),
        }
    }
}

impl Default for wire_KeyState {
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

// Section: sync execution mode utility

#[no_mangle]
pub extern "C" fn free_WireSyncReturn(ptr: support::WireSyncReturn) {
    unsafe {
        let _ = support::box_from_leak_ptr(ptr);
    };
}
