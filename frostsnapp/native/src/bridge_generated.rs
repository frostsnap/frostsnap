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
// Generated by `flutter_rust_bridge`@ 1.82.6.

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
fn wire_emit_key_event_impl(port_: MessagePort, event: impl Wire2Api<KeyState> + UnwindSafe) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "emit_key_event",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_event = event.wire2api();
            move |task_callback| Result::<_, ()>::Ok(emit_key_event(api_event))
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
fn wire_new_coordinator_impl(port_: MessagePort, db_file: impl Wire2Api<String> + UnwindSafe) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, Coordinator, _>(
        WrapInfo {
            debug_name: "new_coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_db_file = db_file.wire2api();
            move |task_callback| new_coordinator(api_db_file)
        },
    )
}
fn wire_echo_key_id_impl(port_: MessagePort, key_id: impl Wire2Api<KeyId> + UnwindSafe) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, mirror_KeyId, _>(
        WrapInfo {
            debug_name: "echo_key_id",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_key_id = key_id.wire2api();
            move |task_callback| Result::<_, ()>::Ok(echo_key_id(api_key_id))
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
fn wire_is_finished__method__SigningState_impl(
    that: impl Wire2Api<SigningState> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "is_finished__method__SigningState",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(SigningState::is_finished(&api_that))
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
fn wire_start_thread__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "start_thread__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            move |task_callback| Coordinator::start_thread(&api_that)
        },
    )
}
fn wire_announce_available_ports__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    ports: impl Wire2Api<Vec<PortDesc>> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "announce_available_ports__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_ports = ports.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(Coordinator::announce_available_ports(&api_that, api_ports))
            }
        },
    )
}
fn wire_switch_to_host_handles_serial__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "switch_to_host_handles_serial__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(Coordinator::switch_to_host_handles_serial(&api_that))
            }
        },
    )
}
fn wire_update_name_preview__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    id: impl Wire2Api<DeviceId> + UnwindSafe,
    name: impl Wire2Api<String> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "update_name_preview__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_id = id.wire2api();
            let api_name = name.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(Coordinator::update_name_preview(
                    &api_that, api_id, api_name,
                ))
            }
        },
    )
}
fn wire_finish_naming__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    id: impl Wire2Api<DeviceId> + UnwindSafe,
    name: impl Wire2Api<String> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "finish_naming__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_id = id.wire2api();
            let api_name = name.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(Coordinator::finish_naming(&api_that, api_id, api_name))
            }
        },
    )
}
fn wire_send_cancel__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    id: impl Wire2Api<DeviceId> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "send_cancel__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            let api_id = id.wire2api();
            move |task_callback| Result::<_, ()>::Ok(Coordinator::send_cancel(&api_that, api_id))
        },
    )
}
fn wire_cancel_all__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "cancel_all__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            move |task_callback| Result::<_, ()>::Ok(Coordinator::cancel_all(&api_that))
        },
    )
}
fn wire_registered_devices__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, Vec<mirror_DeviceId>, _>(
        WrapInfo {
            debug_name: "registered_devices__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Normal,
        },
        move || {
            let api_that = that.wire2api();
            move |task_callback| Result::<_, ()>::Ok(Coordinator::registered_devices(&api_that))
        },
    )
}
fn wire_key_state__method__Coordinator_impl(
    that: impl Wire2Api<Coordinator> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "key_state__method__Coordinator",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(Coordinator::key_state(&api_that))
        },
    )
}
fn wire_get_key__method__Coordinator_impl(
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    key_id: impl Wire2Api<KeyId> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "get_key__method__Coordinator",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            let api_key_id = key_id.wire2api();
            Result::<_, ()>::Ok(Coordinator::get_key(&api_that, api_key_id))
        },
    )
}
fn wire_start_signing__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    key_id: impl Wire2Api<KeyId> + UnwindSafe,
    devices: impl Wire2Api<Vec<DeviceId>> + UnwindSafe,
    message: impl Wire2Api<String> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "start_signing__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Stream,
        },
        move || {
            let api_that = that.wire2api();
            let api_key_id = key_id.wire2api();
            let api_devices = devices.wire2api();
            let api_message = message.wire2api();
            move |task_callback| {
                Coordinator::start_signing(
                    &api_that,
                    api_key_id,
                    api_devices,
                    api_message,
                    task_callback.stream_sink::<_, SigningState>(),
                )
            }
        },
    )
}
fn wire_get_signing_state__method__Coordinator_impl(
    that: impl Wire2Api<Coordinator> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "get_signing_state__method__Coordinator",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            Result::<_, ()>::Ok(Coordinator::get_signing_state(&api_that))
        },
    )
}
fn wire_devices_for_frost_key__method__Coordinator_impl(
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    frost_key: impl Wire2Api<FrostKey> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "devices_for_frost_key__method__Coordinator",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            let api_frost_key = frost_key.wire2api();
            Result::<_, ()>::Ok(Coordinator::devices_for_frost_key(&api_that, api_frost_key))
        },
    )
}
fn wire_get_device__method__Coordinator_impl(
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    id: impl Wire2Api<DeviceId> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "get_device__method__Coordinator",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            let api_id = id.wire2api();
            Result::<_, ()>::Ok(Coordinator::get_device(&api_that, api_id))
        },
    )
}
fn wire_nonces_available__method__Coordinator_impl(
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    id: impl Wire2Api<DeviceId> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "nonces_available__method__Coordinator",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            let api_id = id.wire2api();
            Result::<_, ()>::Ok(Coordinator::nonces_available(&api_that, api_id))
        },
    )
}
fn wire_generate_new_key__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    threshold: impl Wire2Api<usize> + UnwindSafe,
    devices: impl Wire2Api<Vec<DeviceId>> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "generate_new_key__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Stream,
        },
        move || {
            let api_that = that.wire2api();
            let api_threshold = threshold.wire2api();
            let api_devices = devices.wire2api();
            move |task_callback| {
                Result::<_, ()>::Ok(Coordinator::generate_new_key(
                    &api_that,
                    api_threshold,
                    api_devices,
                    task_callback.stream_sink::<_, mirror_CoordinatorToUserKeyGenMessage>(),
                ))
            }
        },
    )
}
fn wire_can_restore_signing_session__method__Coordinator_impl(
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    key_id: impl Wire2Api<KeyId> + UnwindSafe,
) -> support::WireSyncReturn {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap_sync(
        WrapInfo {
            debug_name: "can_restore_signing_session__method__Coordinator",
            port: None,
            mode: FfiCallMode::Sync,
        },
        move || {
            let api_that = that.wire2api();
            let api_key_id = key_id.wire2api();
            Result::<_, ()>::Ok(Coordinator::can_restore_signing_session(
                &api_that, api_key_id,
            ))
        },
    )
}
fn wire_try_restore_signing_session__method__Coordinator_impl(
    port_: MessagePort,
    that: impl Wire2Api<Coordinator> + UnwindSafe,
    key_id: impl Wire2Api<KeyId> + UnwindSafe,
) {
    FLUTTER_RUST_BRIDGE_HANDLER.wrap::<_, _, _, (), _>(
        WrapInfo {
            debug_name: "try_restore_signing_session__method__Coordinator",
            port: Some(port_),
            mode: FfiCallMode::Stream,
        },
        move || {
            let api_that = that.wire2api();
            let api_key_id = key_id.wire2api();
            move |task_callback| {
                Coordinator::try_restore_signing_session(
                    &api_that,
                    api_key_id,
                    task_callback.stream_sink::<_, SigningState>(),
                )
            }
        },
    )
}
// Section: wrapper structs

#[derive(Clone)]
pub struct mirror_CoordinatorToUserKeyGenMessage(CoordinatorToUserKeyGenMessage);

#[derive(Clone)]
pub struct mirror_DeviceId(DeviceId);

#[derive(Clone)]
pub struct mirror_EncodedSignature(EncodedSignature);

#[derive(Clone)]
pub struct mirror_KeyId(KeyId);

// Section: static checks

const _: fn() = || {
    match None::<CoordinatorToUserKeyGenMessage>.unwrap() {
        CoordinatorToUserKeyGenMessage::ReceivedShares { from } => {
            let _: DeviceId = from;
        }
        CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
            let _: [u8; 32] = session_hash;
        }
        CoordinatorToUserKeyGenMessage::KeyGenAck { from } => {
            let _: DeviceId = from;
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
        let EncodedSignature_ = None::<EncodedSignature>.unwrap();
        let _: [u8; 64] = EncodedSignature_.0;
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

impl support::IntoDart for Coordinator {
    fn into_dart(self) -> support::DartAbi {
        vec![self.0.into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for Coordinator {}
impl rust2dart::IntoIntoDart<Coordinator> for Coordinator {
    fn into_into_dart(self) -> Self {
        self
    }
}

impl support::IntoDart for mirror_CoordinatorToUserKeyGenMessage {
    fn into_dart(self) -> support::DartAbi {
        match self.0 {
            CoordinatorToUserKeyGenMessage::ReceivedShares { from } => {
                vec![0.into_dart(), from.into_into_dart().into_dart()]
            }
            CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
                vec![1.into_dart(), session_hash.into_into_dart().into_dart()]
            }
            CoordinatorToUserKeyGenMessage::KeyGenAck { from } => {
                vec![2.into_dart(), from.into_into_dart().into_dart()]
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
        vec![
            self.devices.into_into_dart().into_dart(),
            self.state_id.into_into_dart().into_dart(),
        ]
        .into_dart()
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

impl support::IntoDart for mirror_EncodedSignature {
    fn into_dart(self) -> support::DartAbi {
        vec![self.0 .0.into_into_dart().into_dart()].into_dart()
    }
}
impl support::IntoDartExceptPrimitive for mirror_EncodedSignature {}
impl rust2dart::IntoIntoDart<mirror_EncodedSignature> for EncodedSignature {
    fn into_into_dart(self) -> mirror_EncodedSignature {
        mirror_EncodedSignature(self)
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

impl support::IntoDart for SigningState {
    fn into_dart(self) -> support::DartAbi {
        vec![
            self.got_shares.into_into_dart().into_dart(),
            self.needed_from.into_into_dart().into_dart(),
            self.finished_signatures.into_into_dart().into_dart(),
        ]
        .into_dart()
    }
}
impl support::IntoDartExceptPrimitive for SigningState {}
impl rust2dart::IntoIntoDart<SigningState> for SigningState {
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
pub use self::web::*;

#[cfg(not(target_family = "wasm"))]
#[path = "bridge_generated.io.rs"]
mod io;
#[cfg(not(target_family = "wasm"))]
pub use self::io::*;
