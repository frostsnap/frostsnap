use std::collections::{BTreeSet, HashSet};

use flutter_rust_bridge::frb;
use frostsnap_core::schnorr_fun::frost::ShareIndex;
use frostsnap_core::DeviceId;

use super::broadcast::{Broadcast, UnitBroadcastSubscription};
use crate::coordinator::{SignerDevice, Signers};

/// A device's nonces change only through what this app does: starting a signing session reserves
/// them, and keygen, recovery and a device's signing replies replenish them. Each of those runs on
/// a screen that can't be open over the one that made the selection, and names and share indices
/// don't change for that screen's lifetime either, so the signers are read once, when the
/// selection is made.
pub struct SignerSelection {
    signers: Signers,
    selected: HashSet<DeviceId>,
    nonces_required: u32,
    changed: Broadcast<()>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignerRefusal {
    NotMember,
    TooFewNonces { available: u32, needed: u32 },
    ShareAlreadySelected { share_index: u32 },
    ThresholdReached { threshold: u16 },
}

pub struct SignerRow {
    pub id: DeviceId,
    pub name: Option<String>,
    pub share_index: u32,
    pub selected: bool,
    pub refusal: Option<SignerRefusal>,
}

fn short(share_index: ShareIndex) -> u32 {
    u32::try_from(share_index).expect("share index should fit in u32")
}

impl SignerSelection {
    pub(crate) fn new(signers: Signers) -> Self {
        Self {
            signers,
            selected: HashSet::new(),
            nonces_required: 1,
            changed: Broadcast::default(),
        }
    }

    fn selected_shares(&self) -> BTreeSet<ShareIndex> {
        self.signers
            .devices
            .iter()
            .filter(|device| self.selected.contains(&device.id))
            .map(|device| device.share_index)
            .collect()
    }

    fn refusal(&self, device: &SignerDevice) -> Option<SignerRefusal> {
        if device.nonces < self.nonces_required {
            return Some(SignerRefusal::TooFewNonces {
                available: device.nonces,
                needed: self.nonces_required,
            });
        }
        if self.selected.contains(&device.id) {
            return None;
        }
        let shares = self.selected_shares();
        if shares.contains(&device.share_index) {
            return Some(SignerRefusal::ShareAlreadySelected {
                share_index: short(device.share_index),
            });
        }
        (shares.len() >= usize::from(self.signers.threshold)).then_some(
            SignerRefusal::ThresholdReached {
                threshold: self.signers.threshold,
            },
        )
    }

    #[frb(sync)]
    pub fn subscribe(&self) -> UnitBroadcastSubscription {
        UnitBroadcastSubscription(self.changed.subscribe())
    }

    #[frb(sync)]
    pub fn rows(&self) -> Vec<SignerRow> {
        self.signers
            .devices
            .iter()
            .map(|device| SignerRow {
                id: device.id,
                name: device.name.clone(),
                share_index: short(device.share_index),
                selected: self.selected.contains(&device.id),
                refusal: self.refusal(device),
            })
            .collect()
    }

    #[frb(sync)]
    pub fn threshold(&self) -> u16 {
        self.signers.threshold
    }

    #[frb(sync)]
    pub fn remaining(&self) -> u32 {
        usize::from(self.signers.threshold).saturating_sub(self.selected_shares().len()) as u32
    }

    #[frb(sync)]
    pub fn selected(&self) -> Vec<DeviceId> {
        self.selected.iter().copied().collect()
    }

    #[frb(sync)]
    pub fn require_nonces(&mut self, nonces: u32) {
        self.nonces_required = nonces.max(1);
        let signers = &self.signers;
        let required = self.nonces_required;
        self.selected.retain(|id| {
            signers
                .devices
                .iter()
                .any(|device| device.id == *id && device.nonces >= required)
        });
        self.changed.add(&());
    }

    #[frb(sync)]
    pub fn select(&mut self, d_id: DeviceId) -> Result<(), SignerRefusal> {
        let device = self
            .signers
            .devices
            .iter()
            .find(|device| device.id == d_id)
            .ok_or(SignerRefusal::NotMember)?;
        if let Some(refusal) = self.refusal(device) {
            return Err(refusal);
        }
        if self.selected.insert(d_id) {
            self.changed.add(&());
        }
        Ok(())
    }

    #[frb(sync)]
    pub fn deselect(&mut self, d_id: DeviceId) {
        if self.selected.remove(&d_id) {
            self.changed.add(&());
        }
    }
}

#[cfg(test)]
mod tests {
    use frostsnap_core::schnorr_fun::fun::prelude::*;

    use super::*;

    const A: DeviceId = DeviceId([1; 33]);
    const B: DeviceId = DeviceId([2; 33]);
    const C: DeviceId = DeviceId([3; 33]);

    fn share(index: u32) -> ShareIndex {
        Scalar::<Public, Zero>::from(index).non_zero().unwrap()
    }

    fn two_of(devices: [(DeviceId, u32, u32); 3]) -> SignerSelection {
        SignerSelection::new(Signers {
            threshold: 2,
            devices: devices
                .into_iter()
                .map(|(id, share_index, nonces)| SignerDevice {
                    id,
                    name: None,
                    share_index: share(share_index),
                    nonces,
                })
                .collect(),
        })
    }

    fn row(selection: &SignerSelection, id: DeviceId) -> SignerRow {
        let rows = selection.rows();
        rows.into_iter().find(|row| row.id == id).unwrap()
    }

    #[test]
    fn only_one_device_per_share_can_be_selected() {
        let mut selection = two_of([(A, 1, 1), (B, 1, 1), (C, 2, 1)]);
        selection.select(A).unwrap();
        assert_eq!(
            selection.select(B),
            Err(SignerRefusal::ShareAlreadySelected { share_index: 1 })
        );
        assert_eq!(
            row(&selection, B).refusal,
            Some(SignerRefusal::ShareAlreadySelected { share_index: 1 })
        );
        assert_eq!(selection.remaining(), 1);
        selection.select(C).unwrap();
        assert_eq!(selection.remaining(), 0);
    }
}
