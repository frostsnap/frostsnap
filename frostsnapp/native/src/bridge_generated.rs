#![allow(
    non_camel_case_types,
    unused,
    clippy::redundant_closure,
    clippy::useless_conversion,
    clippy::unit_arg,
    clippy::double_parens,
    non_snake_case,
    clippy::too_many_arguments
)]
// AUTO GENERATED FILE, DO NOT EDIT.
// Generated by `flutter_rust_bridge`@ 1.82.1.

use crate::api::*;
use core::panic::UnwindSafe;
use flutter_rust_bridge::rust2dart::IntoIntoDart;
use flutter_rust_bridge::*;
use std::ffi::c_void;
use std::sync::Arc;

// Section: imports

// Section: wire functions

fn wire_sub_port_events_impl(port_: MessagePort) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "sub_port_events",
            port: Some(port_),
            mode: FfiCallMode::Stream,
        },
        move || {
            move |task_callback| {
                Result::<_, ()>::Ok(sub_port_events(task_callback.stream_sink::<_, PortEvent>()))
            }
        },
    )
}
fn wire_sub_device_events_impl(port_: MessagePort) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "sub_device_events",
            port: Some(port_),
            mode: FfiCallMode::Stream,
        },
        move || {
            move |task_callback| {
                Result::<_, ()>::Ok(sub_device_events(
                    task_callback.stream_sink::<_, DeviceListUpdate>(),
                ))
            }
        },
    )
}
fn wire_sub_key_events_impl(port_: MessagePort) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "sub_key_events",
            port: Some(port_),
            mode: FfiCallMode::Stream,
        },
        move || {
            move |task_callback| {
                Result::<_, ()>::Ok(sub_key_events(task_callback.stream_sink::<_, KeyState>()))
            }
        },
    )
}
fn wire_turn_stderr_logging_on_impl(port_: MessagePort, level: impl Wire2Api<Level> + UnwindSafe) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "turn_stderr_logging_on",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_level = level.wire2api();
            move |task_callback| Result::<_, ()>::Ok(turn_stderr_logging_on(api_level))
        },
    )
}
fn wire_turn_logcat_logging_on_impl(port_: MessagePort, _level: impl Wire2Api<Level> + UnwindSafe) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "turn_logcat_logging_on",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api__level = _level.wire2api();
            move |task_callback| Result::<_, ()>::Ok(turn_logcat_logging_on(api__level))
        },
    )
}
fn wire_announce_available_ports_impl(
    port_: MessagePort,
    ports: impl Wire2Api<Vec<PortDesc>> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "announce_available_ports",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_ports = ports.wire2api();
            move |task_callback| Result::<_, ()>::Ok(announce_available_ports(api_ports))
        },
    )
}
fn wire_switch_to_host_handles_serial_impl(port_: MessagePort) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "switch_to_host_handles_serial",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || move |task_callback| Result::<_, ()>::Ok(switch_to_host_handles_serial()),
    )
}
fn wire_update_name_preview_impl(
    port_: MessagePort,
    id: impl Wire2Api<DeviceId> + UnwindSafe,
    name: impl Wire2Api<String> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "update_name_preview",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_id = id.wire2api();
            let api_name = name.wire2api();
            move |task_callback| Result::<_, ()>::Ok(update_name_preview(api_id, api_name))
        },
    )
}
fn wire_finish_naming_impl(
    port_: MessagePort,
    id: impl Wire2Api<DeviceId> + UnwindSafe,
    name: impl Wire2Api<String> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "finish_naming",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_id = id.wire2api();
            let api_name = name.wire2api();
            move |task_callback| Result::<_, ()>::Ok(finish_naming(api_id, api_name))
        },
    )
}
fn wire_send_cancel_impl(port_: MessagePort, id: impl Wire2Api<DeviceId> + UnwindSafe) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "send_cancel",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_id = id.wire2api();
            move |task_callback| Result::<_, ()>::Ok(send_cancel(api_id))
        },
    )
}
fn wire_cancel_all_impl(port_: MessagePort) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "cancel_all",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || move |task_callback| Result::<_, ()>::Ok(cancel_all()),
    )
}
fn wire_registered_devices_impl(port_: MessagePort) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, Vec<mirror_DeviceId>, _>(
        WrapInfo {
            debug_name: "registered_devices",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || move |task_callback| Result::<_, ()>::Ok(registered_devices()),
    )
}
fn wire_start_coordinator_thread_impl(port_: MessagePort) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "start_coordinator_thread",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || move |task_callback| Result::<_, ()>::Ok(start_coordinator_thread()),
    )
}
fn wire_key_state_impl() -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "key_state",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || Result::<_, ()>::Ok(key_state()),
    )
}
fn wire_get_key_impl(key_id: impl Wire2Api<KeyId> + UnwindSafe) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "get_key",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_key_id = key_id.wire2api();
            Result::<_, ()>::Ok(get_key(api_key_id))
        },
    )
}
fn wire_device_at_index_impl(index: impl Wire2Api<usize> + UnwindSafe) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "device_at_index",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_index = index.wire2api();
            Result::<_, ()>::Ok(device_at_index(api_index))
        },
    )
}
fn wire_device_list_state_impl() -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "device_list_state",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || Result::<_, ()>::Ok(device_list_state()),
    )
}
fn wire_generate_new_key_impl(
    port_: MessagePort,
    threshold: impl Wire2Api<usize> + UnwindSafe,
    devices: impl Wire2Api<Vec<DeviceId>> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "generate_new_key",
            port: Some(port_),
            mode: FfiCallMode::Stream,
        },
        move || {
            let api_threshold = threshold.wire2api();
            let api_devices = devices.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(generate_new_key(
                    api_threshold,
                    api_devices,
                    task_callback.stream_sink::<_, mirror_CoordinatorToUserKeyGenMessage>(),
                ))
            }
        },
    )
}
fn wire_threshold__method__FrostKey_impl(
    that: impl Wire2Api<FrostKey> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "threshold__method__FrostKey",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(FrostKey::threshold(&api_that))
        },
    )
}
fn wire_id__method__FrostKey_impl(
    that: impl Wire2Api<FrostKey> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "id__method__FrostKey",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(FrostKey::id(&api_that))
        },
    )
}
fn wire_name__method__FrostKey_impl(
    that: impl Wire2Api<FrostKey> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "name__method__FrostKey",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(FrostKey::name(&api_that))
        },
    )
}
fn wire_devices__method__FrostKey_impl(
    that: impl Wire2Api<FrostKey> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "devices__method__FrostKey",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(FrostKey::devices(&api_that))
        },
    )
}
fn wire_satisfy__method__PortOpen_impl(
    port_: MessagePort,
    that: impl Wire2Api<PortOpen> + UnwindSafe,
    err: impl Wire2Api<Option<String>> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "satisfy__method__PortOpen",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_err = err.wire2api();
            move |task_callback| Result::<_, ()>::Ok(PortOpen::satisfy(&api_that, api_err))
        },
    )
}
fn wire_satisfy__method__PortRead_impl(
    port_: MessagePort,
    that: impl Wire2Api<PortRead> + UnwindSafe,
    bytes: impl Wire2Api<Vec<u8>> + UnwindSafe,
    err: impl Wire2Api<Option<String>> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "satisfy__method__PortRead",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_bytes = bytes.wire2api();
            let api_err = err.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(PortRead::satisfy(&api_that, api_bytes, api_err))
            }
        },
    )
}
fn wire_satisfy__method__PortWrite_impl(
    port_: MessagePort,
    that: impl Wire2Api<PortWrite> + UnwindSafe,
    err: impl Wire2Api<Option<String>> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "satisfy__method__PortWrite",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_err = err.wire2api();
            move |task_callback| Result::<_, ()>::Ok(PortWrite::satisfy(&api_that, api_err))
        },
    )
}
fn wire_satisfy__method__PortBytesToRead_impl(
    port_: MessagePort,
    that: impl Wire2Api<PortBytesToRead> + UnwindSafe,
    bytes_to_read: impl Wire2Api<u32> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "satisfy__method__PortBytesToRead",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_bytes_to_read = bytes_to_read.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(PortBytesToRead::satisfy(&api_that, api_bytes_to_read))
            }
        },
    )
}
fn wire_named_devices__method__DeviceListState_impl(
    that: impl Wire2Api<DeviceListState> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "named_devices__method__DeviceListState",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(DeviceListState::named_devices(&api_that))
        },
    )
}
// Section: wrapper structs

#[derive(Clone)]
pub struct mirror_CoordinatorToUserKeyGenMessage(CoordinatorToUserKeyGenMessage);

#[derive(Clone)]
pub struct mirror_DeviceId(DeviceId);

#[derive(Clone)]
pub struct mirror_KeyId(KeyId);

// Section: static checks

const _: fn() = || {
    match None::<CoordinatorToUserKeyGenMessage>.unwrap() {
        CoordinatorToUserKeyGenMessage::ReceivedShares { id } => {
            let _: DeviceId = id;
        }
        CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
            let _: [u8; 32] = session_hash;
        }
        CoordinatorToUserKeyGenMessage::KeyGenAck { id } => {
            let _: DeviceId = id;
        }
        CoordinatorToUserKeyGenMessage::FinishedKey { key_id } => {
            let _: KeyId = key_id;
        }
    }
    {
        let DeviceId_ = None::<DeviceId>.unwrap();
        let _: [u8; 33] = DeviceId_.0;
    }
    {
        let KeyId_ = None::<KeyId>.unwrap();
        let _: [u8; 32] = KeyId_.0;
    }
};
// Section: allocate functions

// Section: related functions

// Section: impl Wire2Api

pub trait Wire2Api<T> {
    fn wire2api(self) -> T;
}

impl<T, S> Wire2Api<Option<T>> for *mut S
where
    *mut S: Wire2Api<T>,
{
    fn wire2api(self) -> Option<T> {
        (!self.is_null()).then(|| self.wire2api())
    }
}

impl Wire2Api<i32> for i32 {
    fn wire2api(self) -> i32 {
        self
    }
}

impl Wire2Api<Level> for i32 {
    fn wire2api(self) -> Level {
        match self {
            0 => Level::Debug,
            1 => Level::Info,
            _ => unreachable!("Invalid variant for Level: {}", self),
        }
    }
}

impl Wire2Api<u16> for u16 {
    fn wire2api(self) -> u16 {
        self
    }
}
impl Wire2Api<u32> for u32 {
    fn wire2api(self) -> u32 {
        self
    }
}
impl Wire2Api<u8> for u8 {
    fn wire2api(self) -> u8 {
        self
    }
}

impl Wire2Api<usize> for usize {
    fn wire2api(self) -> usize {
        self
    }
}
// Section: impl IntoDart

impl support::IntoDart for mirror_CoordinatorToUserKeyGenMessage {
    fn into_dart(self) -> support::DartAbi {
        match self.0 {
            CoordinatorToUserKeyGenMessage::ReceivedShares { id } => {
                vec![0.into_dart(), id.into_into_dart().into_dart()]
            }
            CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
                vec![1.into_dart(), session_hash.into_into_dart().into_dart()]
            }
            CoordinatorToUserKeyGenMessage::KeyGenAck { id } => {
                vec![2.into_dart(), id.into_into_dart().into_dart()]
            }
            CoordinatorToUserKeyGenMessage::FinishedKey { key_id } => {
                vec![3.into_dart(), key_id.into_into_dart().into_dart()]
            }
        }
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for mirror_CoordinatorToUserKeyGenMessage {}
impl rust2dart::IntoIntoDart<mirror_CoordinatorToUserKeyGenMessage>
    for CoordinatorToUserKeyGenMessage
{
    fn into_into_dart(self) -> mirror_CoordinatorToUserKeyGenMessage {
        mirror_CoordinatorToUserKeyGenMessage(self)
    }
}

impl support::IntoDart for Device {
    fn into_dart(self) -> support::DartAbi {
        vec![self.name.into_dart(), self.id.into_into_dart().into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for Device {}
impl rust2dart::IntoIntoDart<Device> for Device {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for mirror_DeviceId {
    fn into_dart(self) -> support::DartAbi {
        vec![self.0 .0.into_into_dart().into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for mirror_DeviceId {}
impl rust2dart::IntoIntoDart<mirror_DeviceId> for DeviceId {
    fn into_into_dart(self) -> mirror_DeviceId {
        mirror_DeviceId(self)
    }
}

impl support::IntoDart for DeviceListChange {
    fn into_dart(self) -> support::DartAbi {
        vec![
            self.kind.into_into_dart().into_dart(),
            self.index.into_into_dart().into_dart(),
            self.device.into_into_dart().into_dart(),
        ]
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for DeviceListChange {}
impl rust2dart::IntoIntoDart<DeviceListChange> for DeviceListChange {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for DeviceListChangeKind {
    fn into_dart(self) -> support::DartAbi {
        match self {
            Self::Added => 0,
            Self::Removed => 1,
            Self::Named => 2,
        }
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for DeviceListChangeKind {}
impl rust2dart::IntoIntoDart<DeviceListChangeKind> for DeviceListChangeKind {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for DeviceListState {
    fn into_dart(self) -> support::DartAbi {
        vec![self.devices.into_into_dart().into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for DeviceListState {}
impl rust2dart::IntoIntoDart<DeviceListState> for DeviceListState {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for DeviceListUpdate {
    fn into_dart(self) -> support::DartAbi {
        vec![
            self.changes.into_into_dart().into_dart(),
            self.state.into_into_dart().into_dart(),
        ]
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for DeviceListUpdate {}
impl rust2dart::IntoIntoDart<DeviceListUpdate> for DeviceListUpdate {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for FrostKey {
    fn into_dart(self) -> support::DartAbi {
        vec![self.0.into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for FrostKey {}
impl rust2dart::IntoIntoDart<FrostKey> for FrostKey {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for mirror_KeyId {
    fn into_dart(self) -> support::DartAbi {
        vec![self.0 .0.into_into_dart().into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for mirror_KeyId {}
impl rust2dart::IntoIntoDart<mirror_KeyId> for KeyId {
    fn into_into_dart(self) -> mirror_KeyId {
        mirror_KeyId(self)
    }
}

impl support::IntoDart for KeyState {
    fn into_dart(self) -> support::DartAbi {
        vec![self.keys.into_into_dart().into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for KeyState {}
impl rust2dart::IntoIntoDart<KeyState> for KeyState {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for PortBytesToRead {
    fn into_dart(self) -> support::DartAbi {
        vec![self.id.into_into_dart().into_dart(), self.ready.into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for PortBytesToRead {}
impl rust2dart::IntoIntoDart<PortBytesToRead> for PortBytesToRead {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for PortEvent {
    fn into_dart(self) -> support::DartAbi {
        match self {
            Self::Open { request } => vec![0.into_dart(), request.into_into_dart().into_dart()],
            Self::Write { request } => vec![1.into_dart(), request.into_into_dart().into_dart()],
            Self::Read { request } => vec![2.into_dart(), request.into_into_dart().into_dart()],
            Self::BytesToRead { request } => {
                vec![3.into_dart(), request.into_into_dart().into_dart()]
            }
        }
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for PortEvent {}
impl rust2dart::IntoIntoDart<PortEvent> for PortEvent {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for PortOpen {
    fn into_dart(self) -> support::DartAbi {
        vec![
            self.id.into_into_dart().into_dart(),
            self.baud_rate.into_into_dart().into_dart(),
            self.ready.into_dart(),
        ]
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for PortOpen {}
impl rust2dart::IntoIntoDart<PortOpen> for PortOpen {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for PortRead {
    fn into_dart(self) -> support::DartAbi {
        vec![
            self.id.into_into_dart().into_dart(),
            self.len.into_into_dart().into_dart(),
            self.ready.into_dart(),
        ]
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for PortRead {}
impl rust2dart::IntoIntoDart<PortRead> for PortRead {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for PortWrite {
    fn into_dart(self) -> support::DartAbi {
        vec![
            self.id.into_into_dart().into_dart(),
            self.bytes.into_into_dart().into_dart(),
            self.ready.into_dart(),
        ]
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for PortWrite {}
impl rust2dart::IntoIntoDart<PortWrite> for PortWrite {
    fn into_into_dart(self) -> Self {
        self
    }
}

// Section: executor

support::lazy_static! {
    pub static ref FLUTTER_RUST_BRIDGE_HANDLER: support::DefaultHandler = Default::default();
}

/// cbindgen:ignore
#[cfg(target_family = "wasm")]
#[path = "bridge_generated.web.rs"]
mod web;
#[cfg(target_family = "wasm")]
pub use web::*;

#[cfg(not(target_family = "wasm"))]
#[path = "bridge_generated.io.rs"]
mod io;
#[cfg(not(target_family = "wasm"))]
pub use io::*;
