#![no_std]

use alloc::{collections::VecDeque, string::ToString};
use esp_hal::sha;
use frostsnap_comms::{DeviceSendBody, DeviceSendMessage};
use frostsnap_core::DeviceId;
use rand_core::SeedableRng;
use ui::UserInteraction;

#[macro_use]
extern crate alloc;

pub mod device_config;
pub mod efuse;
pub mod esp32_run;
mod flash_nonce_slot;
#[cfg(feature = "v2")]
pub mod graphics;
pub mod io;
pub mod ota;
pub mod panic;
pub mod storage;
pub mod ui;

#[derive(Debug, Clone)]
pub struct UpstreamConnection {
    state: UpstreamConnectionState,
    messages: VecDeque<DeviceSendMessage<DeviceSendBody>>,
    announcement: Option<DeviceSendMessage<DeviceSendBody>>,
    my_device_id: DeviceId,
}

impl UpstreamConnection {
    pub fn new(my_device_id: DeviceId) -> Self {
        Self {
            state: UpstreamConnectionState::PowerOn,
            messages: Default::default(),
            announcement: None,
            my_device_id,
        }
    }

    pub fn set_state(&mut self, state: UpstreamConnectionState, ui: &mut impl UserInteraction) {
        ui.set_upstream_connection_state(state);
        match state {
            UpstreamConnectionState::PowerOn => {
                self.messages.clear();
            }
            UpstreamConnectionState::Established => {}
            UpstreamConnectionState::EstablishedAndCoordAck => {}
        }
        self.state = state;
    }

    pub fn get_state(&self) -> UpstreamConnectionState {
        self.state
    }

    pub fn dequeue_message(&mut self) -> Option<DeviceSendMessage<DeviceSendBody>> {
        if self.state >= UpstreamConnectionState::Established {
            if let Some(announcement) = self.announcement.take() {
                return Some(announcement);
            }
        }

        if self.state == UpstreamConnectionState::EstablishedAndCoordAck {
            return self.messages.pop_front();
        }

        None
    }

    pub fn send_announcement(&mut self, announcement: DeviceSendBody) {
        self.announcement = Some(DeviceSendMessage {
            from: self.my_device_id,
            body: announcement,
        });
    }

    pub fn send_to_coordinator(
        &mut self,
        iter: impl IntoIterator<Item = impl Into<DeviceSendBody>>,
    ) {
        self.messages
            .extend(iter.into_iter().map(|body| DeviceSendMessage {
                from: self.my_device_id,
                body: body.into(),
            }));
    }

    fn send_debug(&mut self, message: impl ToString) {
        if self.state == UpstreamConnectionState::EstablishedAndCoordAck {
            self.send_to_coordinator([DeviceSendBody::Debug {
                message: message.to_string(),
            }]);
        }
    }

    pub fn has_messages_to_send(&self) -> bool {
        self.announcement.is_some() || !self.messages.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum UpstreamConnectionState {
    /// We have power from the upstream port
    PowerOn,
    /// Received magic bytes from upstream device
    Established,
    /// The coordinator has Ack'd us
    EstablishedAndCoordAck,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DownstreamConnectionState {
    Disconnected,
    Connected,
    Established,
}

pub type Instant = fugit::Instant<u64, 1, 1_000_000>;
pub type Duration = fugit::Duration<u64, 1, 1_000_000>;

pub fn extract_entropy(
    rng: &mut impl rand_core::RngCore,
    sha256: &mut esp_hal::sha::Sha<'_>,
    bytes: usize,
) -> impl rand_core::RngCore {
    pub use frostsnap_core::sha2::digest::FixedOutput;
    let mut digest = sha256.start::<sha::Sha256>();
    for _ in 0..(bytes.div_ceil(64)) {
        let mut entropy = [0u8; 64];
        rng.fill_bytes(&mut entropy);
        digest.update(&entropy).expect("infallible");
    }

    let result = digest.finalize_fixed();
    rand_chacha::ChaCha20Rng::from_seed(result.into())
}
