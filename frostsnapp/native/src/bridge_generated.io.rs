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
pub extern "C" fn wire_new_ffi_coordinator(port_: i64, host_handles_serial: bool) {
    wire_new_ffi_coordinator_impl(port_, host_handles_serial)
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
pub extern "C" fn wire_announce_available_ports(
    port_: i64,
    coordinator: wire_FfiCoordinator,
    ports: *mut wire_list_port_desc,
) {
    wire_announce_available_ports_impl(port_, coordinator, ports)
}

#[no_mangle]
pub extern "C" fn wire_update_name_preview(
    port_: i64,
    coordinator: wire_FfiCoordinator,
    id: *mut wire_DeviceId,
    name: *mut wire_uint_8_list,
) {
    wire_update_name_preview_impl(port_, coordinator, id, name)
}

#[no_mangle]
pub extern "C" fn wire_finish_naming(
    port_: i64,
    coordinator: wire_FfiCoordinator,
    id: *mut wire_DeviceId,
    name: *mut wire_uint_8_list,
) {
    wire_finish_naming_impl(port_, coordinator, id, name)
}

#[no_mangle]
pub extern "C" fn wire_send_cancel(
    port_: i64,
    coordinator: wire_FfiCoordinator,
    id: *mut wire_DeviceId,
) {
    wire_send_cancel_impl(port_, coordinator, id)
}

#[no_mangle]
pub extern "C" fn wire_registered_devices(port_: i64, coordinator: wire_FfiCoordinator) {
    wire_registered_devices_impl(port_, coordinator)
}

#[no_mangle]
pub extern "C" fn wire_generate_new_key(
    port_: i64,
    coordinator: wire_FfiCoordinator,
    threshold: usize,
) {
    wire_generate_new_key_impl(port_, coordinator, threshold)
}

#[no_mangle]
pub extern "C" fn wire_keygen_ack(port_: i64, coordinator: wire_FfiCoordinator, ack: bool) {
    wire_keygen_ack_impl(port_, coordinator, ack)
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

// Section: allocate functions

#[no_mangle]
pub extern "C" fn new_FfiCoordinator() -> wire_FfiCoordinator {
    wire_FfiCoordinator::new_with_null_ptr()
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

impl Wire2Api<RustOpaque<FfiCoordinator>> for wire_FfiCoordinator {
    fn wire2api(self) -> RustOpaque<FfiCoordinator> {
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
impl Wire2Api<DeviceId> for wire_DeviceId {
    fn wire2api(self) -> DeviceId {
        DeviceId(self.field0.wire2api())
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
pub struct wire_FfiCoordinator {
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
pub struct wire_DeviceId {
    field0: *mut wire_uint_8_list,
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

impl NewWithNullPtr for wire_FfiCoordinator {
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
