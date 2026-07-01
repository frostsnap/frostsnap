//! Persistence entry point for wallets recovered over an external
//! (nostr) channel. This module is stateless: no field on
//! `FrostCoordinator`, no mutation types, no protocol-level state.
//!
//! The fold engine is `RecoveringAccessStructure` in
//! [`super::restoration`]; construct one via
//! `RecoveringAccessStructure::new(shares, threshold_hint)` and, once
//! `.shared_key.is_some()`, hand it to
//! [`super::FrostCoordinator::finalize_remote_recovery`] to persist.
//!
//! Callers must NOT use `RecoveringAccessStructure::access_structure_ref()`
//! for reconstruction checks in the remote context — it falls back to
//! `HeldShare2.access_structure_ref` metadata on the input shares
//! ([`super::restoration`] `access_structure_ref` impl), and that
//! metadata is untrusted from remote peers. Use `.shared_key.is_some()`
//! instead.

use super::restoration::{
    PendingConsolidation, RecoveringAccessStructure, RestorationError, RestorationMutation,
};
use super::{KeyPurpose, Mutation};
use crate::{AccessStructureRef, DeviceId, SymmetricKey};
use alloc::{collections::BTreeSet, string::String};

impl super::FrostCoordinator {
    /// Persist an access structure obtained via remote (nostr) recovery.
    ///
    /// `recovered` must have `shared_key: Some(_)` — otherwise returns
    /// `Err(RestorationError::NotEnoughShares)` without emitting any
    /// mutations (all-or-nothing).
    ///
    /// `my_local_devices` filters BOTH:
    /// - the `device_to_share_index` handed to `mutate_new_key` —
    ///   `mutate_new_key` emits `KeyMutation::NewShare` per entry, and
    ///   recording remote participants' devices as if we hold their
    ///   encrypted shares would lie to downstream signing / recovery /
    ///   backup paths. Same split as `remote_keygen.rs` uses at its
    ///   local_devices filter.
    /// - `needs_to_consolidate()` — so we don't queue consolidation
    ///   for another participant's device.
    ///
    /// `key_name` and `purpose` come from the leader-authored channel
    /// metadata (fetched by the caller from the transport); they aren't
    /// derivable from the shared_key alone.
    pub fn finalize_remote_recovery(
        &mut self,
        recovered: &RecoveringAccessStructure,
        key_name: String,
        purpose: KeyPurpose,
        my_local_devices: &BTreeSet<DeviceId>,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<AccessStructureRef, RestorationError> {
        let root_shared_key = recovered
            .shared_key
            .as_ref()
            .ok_or(RestorationError::NotEnoughShares)?
            .clone();
        let full_map = recovered
            .compatible_device_to_share_index()
            .expect("shared_key is Some ⇒ compatible_device_to_share_index returns Some");

        let local_map = full_map
            .iter()
            .filter(|(d, _)| my_local_devices.contains(d))
            .map(|(d, i)| (*d, *i))
            .collect();

        let access_structure_ref =
            self.mutate_new_key(key_name, root_shared_key, local_map, encryption_key, purpose, rng);

        for device_id in recovered.needs_to_consolidate() {
            if !my_local_devices.contains(&device_id) {
                continue;
            }
            self.mutate(Mutation::Restoration(
                RestorationMutation::DeviceNeedsConsolidation(PendingConsolidation {
                    device_id,
                    access_structure_ref,
                    share_index: full_map[&device_id],
                }),
            ));
        }

        Ok(access_structure_ref)
    }
}
