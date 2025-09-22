use std::collections::{BTreeSet, HashSet};

use super::keys;
use super::*;
use crate::{fail, message::HeldShare, EnterPhysicalId, RestorationId};

#[derive(Clone, Debug, PartialEq)]
pub struct RestorationState {
    pub restoration_id: RestorationId,
    pub key_name: String,
    pub access_structure: RecoveringAccessStructure,
    pub need_to_consolidate: HashSet<DeviceId>,
    pub key_purpose: KeyPurpose,
    pub fingerprint: schnorr_fun::frost::Fingerprint,
}

impl RestorationState {
    pub fn is_restorable(&self) -> bool {
        self.status().shared_key.is_some()
    }

    pub fn status(&self) -> RestorationStatus {
        // Use fuzzy recovery to find compatible shares and shared key
        let recovery_result = self.access_structure.try_fuzzy_recovery(self.fingerprint);

        let compatible_indices = recovery_result
            .as_ref()
            .map(|(_, indices)| indices.clone())
            .unwrap_or_default();

        let shares = self.access_structure
            .held_shares
            .iter()
            .enumerate()
            .map(|(i, recover_share)| {
                let is_compatible = compatible_indices.contains(&i);

                RestorationShare {
                    device_id: recover_share.held_by,
                    index: recover_share.held_share.share_image.index.try_into().expect("share index is small"),
                    is_compatible,
                }
            })
            .collect();

        RestorationStatus {
            threshold: self.access_structure.threshold,
            shares,
            shared_key: recovery_result.map(|(key, _)| key),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestorationStatus {
    pub threshold: Option<u16>,
    pub shares: Vec<RestorationShare>,
    pub shared_key: Option<SharedKey>,
}

impl RestorationStatus {
    pub fn problem(&self) -> Option<RestorationProblem> {
        // Count compatible unique shares
        let compatible_unique: u16 = self
            .shares
            .iter()
            .filter(|share| share.is_compatible)
            .map(|share| share.index)
            .collect::<BTreeSet<_>>()
            .len()
            .try_into()
            .expect("must be small");

        // Check if we have any shares but none are compatible
        if !self.shares.is_empty() && compatible_unique == 0 {
            return Some(RestorationProblem::InvalidShares);
        }

        // If we know the threshold, check if we have enough compatible shares
        if let Some(threshold) = self.threshold {
            if compatible_unique < threshold {
                return Some(RestorationProblem::NotEnoughShares {
                    need_more: threshold - compatible_unique,
                });
            }
        }

        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestorationProblem {
    NotEnoughShares { need_more: u16 },
    InvalidShares,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestorationShare {
    pub device_id: DeviceId,
    pub index: u16,
    pub is_compatible: bool,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RecoverShare {
    pub held_by: DeviceId,
    pub held_share: HeldShare,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub(super) restorations: BTreeMap<RestorationId, restoration::RestorationState>,
    /// This is where we remember consolidations we need to do from restorations we've finished.
    pub(super) pending_physical_consolidations: BTreeSet<PendingConsolidation>,
    /// For when we ask a device to enter a backup and we plan to immediately consolidate it because
    /// we already know the access structure.
    tmp_waiting_consolidate: BTreeSet<PendingConsolidation>,

    tmp_waiting_save: BTreeMap<(DeviceId, ShareImage), RestorationId>,
}

impl State {
    pub fn apply_mutation_restoration(
        &mut self,
        mutation: RestorationMutation,
        fingerprint: schnorr_fun::frost::Fingerprint,
    ) -> Option<RestorationMutation> {
        use RestorationMutation::*;
        match mutation {
            LegacyNewRestoration {
                restoration_id,
                ref key_name,
                threshold,
                key_purpose,
            } => {
                // Convert legacy to new and recurse
                return self.apply_mutation_restoration(NewRestoration {
                    restoration_id,
                    key_name: key_name.clone(),
                    threshold: Some(threshold),
                    key_purpose,
                }, fingerprint);
            }
            NewRestoration {
                restoration_id,
                ref key_name,
                threshold,
                key_purpose,
            } => {
                self.restorations.insert(
                    restoration_id,
                    RestorationState {
                        restoration_id,
                        key_name: key_name.clone(),
                        access_structure: RecoveringAccessStructure {
                            threshold,
                            held_shares: Default::default(),
                        },
                        need_to_consolidate: Default::default(),
                        key_purpose,
                        fingerprint,
                    },
                );
            }
            LegacyRestorationProgress {
                restoration_id,
                device_id,
                access_structure_ref,
                share_image,
            } => {
                // Convert legacy to new format
                let held_share = HeldShare {
                    access_structure_ref,
                    share_image,
                    threshold: None, // Legacy didn't have threshold
                    key_name: self.restorations.get(&restoration_id)
                        .map(|s| s.key_name.clone())
                        .unwrap_or_default(),
                    purpose: self.restorations.get(&restoration_id)
                        .map(|s| s.key_purpose)
                        .unwrap_or(KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin)),
                };
                return self.apply_mutation_restoration(RestorationProgress {
                    restoration_id,
                    device_id,
                    held_share,
                }, fingerprint);
            }
            RestorationProgress {
                restoration_id,
                device_id,
                ref held_share,
            } => {
                if let Some(state) = self.restorations.get_mut(&restoration_id) {
                    if state
                        .access_structure
                        .has_got_share(device_id, held_share.share_image)
                    {
                        return None;
                    }

                    // Check for AccessStructureRef conflicts
                    if let Some(new_ref) = held_share.access_structure_ref {
                        if let Some(existing_ref) = state.access_structure.get_access_structure_ref() {
                            if existing_ref != new_ref {
                                fail!("access_structure_ref didn't match");
                            }
                        }
                    }

                    // If no access_structure_ref, mark for consolidation
                    if held_share.access_structure_ref.is_none() {
                        state.need_to_consolidate.insert(device_id);
                    }

                    let recover_share = RecoverShare {
                        held_by: device_id,
                        held_share: held_share.clone(),
                    };
                    state.access_structure.held_shares.push(recover_share);
                } else {
                    fail!("restoration id didn't exist")
                }
            }
            DeleteRestoration { restoration_id } => {
                let existed = self.restorations.remove(&restoration_id).is_some();
                if !existed {
                    return None;
                }
            }
            DeviceNeedsConsolidation(consolidation) => {
                let changed = self.pending_physical_consolidations.insert(consolidation);
                if !changed {
                    return None;
                }
            }
            DeviceFinishedConsolidation(consolidation) => {
                if !self.pending_physical_consolidations.remove(&consolidation) {
                    fail!("pending physical restoration did not exist");
                }
            }
            DeleteRestorationShare {
                restoration_id,
                device_id,
                share_image,
            } => {
                if let Some(restoration) = self.restorations.get_mut(&restoration_id) {
                    let pos = restoration
                        .access_structure
                        .held_shares
                        .iter()
                        .position(|recover_share| recover_share.held_by == device_id && recover_share.held_share.share_image == share_image)?;
                    restoration.access_structure.held_shares.remove(pos);
                    restoration.need_to_consolidate.remove(&device_id);
                } else {
                    fail!("restoration id didn't exist");
                }
            }
        }

        Some(mutation.clone())
    }

    pub fn clear_up_key_deletion(&mut self, key_id: KeyId) {
        self.pending_physical_consolidations
            .retain(|consolidation| consolidation.access_structure_ref.key_id != key_id);

        self.tmp_waiting_consolidate
            .retain(|consolidation| consolidation.access_structure_ref.key_id != key_id);
    }

    pub fn clear_tmp_data(&mut self) {
        self.tmp_waiting_consolidate.clear();
        self.tmp_waiting_save.clear();
    }
}

impl FrostCoordinator {
    pub fn start_restoring_key(
        &mut self,
        key_name: String,
        threshold: Option<u16>,
        key_purpose: KeyPurpose,
        restoration_id: RestorationId,
    ) {
        assert!(!self.restoration.restorations.contains_key(&restoration_id));
        self.mutate(Mutation::Restoration(RestorationMutation::NewRestoration {
            restoration_id,
            key_name,
            threshold,
            key_purpose,
        }));
    }

    pub fn request_held_shares(&self, id: DeviceId) -> impl Iterator<Item = CoordinatorSend> {
        core::iter::once(CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(
                CoordinatorRestoration::RequestHeldShares,
            ),
            destinations: [id].into(),
        })
    }

    pub fn tell_device_to_load_physical_backup(
        &self,
        enter_physical_id: EnterPhysicalId,
        device_id: DeviceId,
    ) -> impl IntoIterator<Item = CoordinatorSend> {
        vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(
                CoordinatorRestoration::EnterPhysicalBackup { enter_physical_id },
            ),
            destinations: [device_id].into(),
        }]
    }

    /// Check a physical backup loaded by a device that you know belongs to a certain access structure.
    pub fn check_physical_backup(
        &self,
        access_structure_ref: AccessStructureRef,
        phase: PhysicalBackupPhase,
        encryption_key: SymmetricKey,
    ) -> Result<ShareIndex, CheckBackupError> {
        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;

        let share_index = phase.backup.share_image.index;
        let CoordFrostKey { complete_key, .. } = self
            .keys
            .get(&key_id)
            .ok_or(CheckBackupError::NoSuchAccessStructure)?;

        let root_shared_key = complete_key
            .root_shared_key(access_structure_id, encryption_key)
            .ok_or(CheckBackupError::DecryptionError)?;

        let expected_image = root_shared_key.share_image(share_index);
        if phase.backup.share_image != expected_image {
            return Err(CheckBackupError::ShareImageIsWrong);
        }

        Ok(share_index)
    }

    pub fn tell_device_to_save_physical_backup(
        &mut self,
        phase: PhysicalBackupPhase,
        restoration_id: RestorationId,
    ) -> impl IntoIterator<Item = CoordinatorSend> {
        self.restoration
            .tmp_waiting_save
            .insert((phase.from, phase.backup.share_image), restoration_id);
        let state = match self.get_restoration_state(restoration_id) {
            Some(state) => state,
            None => return vec![],
        };
        let PhysicalBackupPhase {
            backup: EnteredPhysicalBackup { share_image, .. },
            from,
        } = phase;
        vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(
                CoordinatorRestoration::SavePhysicalBackup {
                    share_image,
                    key_name: state.key_name,
                    threshold: state.access_structure.threshold,
                    purpose: state.key_purpose,
                },
            ),
            destinations: [from].into(),
        }]
    }

    /// This is for telling the device to consolidate a backup when we have recovered the key already.
    /// If the key is recovering you have to consolidate it after recovery has finished.
    pub fn tell_device_to_consolidate_physical_backup(
        &mut self,
        phase: PhysicalBackupPhase,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
    ) -> Result<TellDeviceConsolidateBackup, CheckBackupError> {
        self.check_physical_backup(access_structure_ref, phase, encryption_key)?;

        let PhysicalBackupPhase {
            backup:
                EnteredPhysicalBackup {
                    enter_physical_id: _,
                    share_image,
                },
            from,
        } = phase;

        let root_shared_key = self
            .root_shared_key(access_structure_ref, encryption_key)
            .expect("invariant");
        let frost_key = self
            .get_frost_key(access_structure_ref.key_id)
            .expect("invariant");

        let key_name = frost_key.key_name.clone();
        let purpose = frost_key.purpose;

        self.restoration
            .tmp_waiting_consolidate
            .insert(PendingConsolidation {
                device_id: from,
                access_structure_ref,
                share_index: share_image.index,
            });

        Ok(TellDeviceConsolidateBackup {
            device_id: from,
            share_index: share_image.index,
            root_shared_key,
            key_name,
            purpose,
        })
    }

    pub fn add_recovery_share_to_restoration(
        &mut self,
        restoration_id: RestorationId,
        recover_share: &RecoverShare,
    ) -> Result<(), RestoreRecoverShareError> {
        self.check_recover_share_compatible_with_restoration(restoration_id, recover_share)?;
        self.mutate(Mutation::Restoration(
            RestorationMutation::RestorationProgress {
                restoration_id,
                device_id: recover_share.held_by,
                held_share: recover_share.held_share.clone(),
            },
        ));

        Ok(())
    }

    pub fn check_recover_share_compatible_with_restoration(
        &self,
        restoration_id: RestorationId,
        recover_share: &RecoverShare,
    ) -> Result<(), RestoreRecoverShareError> {
        match self.restoration.restorations.get(&restoration_id) {
            Some(restoration) => {
                // Check if we already have this exact share
                if restoration.access_structure.has_got_share_image(
                    recover_share.held_by,
                    recover_share.held_share.share_image,
                ) {
                    return Err(RestoreRecoverShareError::AlreadyGotThisShare);
                }

                // Only reject if AccessStructureRef conflicts
                let new_ref = recover_share.held_share.access_structure_ref;
                let existing_ref = restoration.access_structure.get_access_structure_ref();

                if let (Some(new), Some(existing)) = (new_ref, existing_ref) {
                    if new != existing {
                        return Err(RestoreRecoverShareError::AcccessStructureMismatch);
                    }
                }
            }
            None => return Err(RestoreRecoverShareError::UnknownRestorationId),
        }

        Ok(())
    }

    pub fn check_physical_backup_compatible_with_restoration(
        &self,
        restoration_id: RestorationId,
        phase: PhysicalBackupPhase,
    ) -> Result<(), RestorePhysicalBackupError> {
        match self.restoration.restorations.get(&restoration_id) {
            Some(restoration) => {
                // Only check if we already have this exact share
                if restoration
                    .access_structure
                    .has_got_share_image(phase.from, phase.backup.share_image)
                {
                    return Err(RestorePhysicalBackupError::AlreadyGotThisShare);
                }
                // Accept all other shares - fuzzy recovery will determine validity later
            }
            None => return Err(RestorePhysicalBackupError::UnknownRestorationId),
        }

        Ok(())
    }

    pub fn finish_restoring(
        &mut self,
        restoration_id: RestorationId,
        encryption_key: SymmetricKey,
        rng: &mut impl rand_core::RngCore,
    ) -> Result<AccessStructureRef, RestorationError> {
        let state = self
            .restoration
            .restorations
            .get(&restoration_id)
            .cloned()
            .ok_or(RestorationError::UnknownRestorationId)?;

        // Use fuzzy recovery to find the valid shared key
        let (root_shared_key, compatible_indices) = state
            .access_structure
            .try_fuzzy_recovery(state.fingerprint)
            .ok_or(RestorationError::NotEnoughShares)?;

        let got_threshold = root_shared_key.threshold();

        // If we have an expected threshold, verify it matches
        if let Some(expected_threshold) = state.access_structure.threshold {
            if expected_threshold as usize != got_threshold {
                return Err(RestorationError::ThresholdDoesntMatch {
                    expected: expected_threshold,
                    got: got_threshold as u16,
                });
            }
        }

        let access_structure_ref = AccessStructureRef::from_root_shared_key(&root_shared_key);

        // if we already know about this access structure, check it matches
        if let Some(expected_ref) = state.access_structure.get_access_structure_ref() {
            if access_structure_ref != expected_ref {
                return Err(RestorationError::InterpolationDoesntMatch);
            }
        }

        // Build device_to_share_index from compatible shares only
        let device_to_share_index: BTreeMap<DeviceId, ShareIndex> = compatible_indices
            .iter()
            .filter_map(|&i| {
                state.access_structure.held_shares.get(i).map(|recover_share| {
                    (recover_share.held_by, recover_share.held_share.share_image.index)
                })
            })
            .collect();

        self.mutate_new_key(
            state.key_name.clone(),
            root_shared_key,
            device_to_share_index,
            encryption_key,
            state.key_purpose,
            rng,
        );

        for device_id in state.need_to_consolidate {
            self.mutate(Mutation::Restoration(
                RestorationMutation::DeviceNeedsConsolidation(PendingConsolidation {
                    device_id,
                    access_structure_ref,
                    share_index: state
                        .access_structure
                        .get_device_contribution(device_id)
                        .expect("invariant")
                        .index,
                }),
            ))
        }

        self.mutate(Mutation::Restoration(
            RestorationMutation::DeleteRestoration { restoration_id },
        ));

        Ok(access_structure_ref)
    }

    pub fn get_restoration_state(&self, restoration_id: RestorationId) -> Option<RestorationState> {
        self.restoration.restorations.get(&restoration_id).cloned()
    }

    /// Recovers a share to an existing access structure
    pub fn recover_share(
        &mut self,
        access_structure_ref: AccessStructureRef,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<(), RecoverShareError> {
        self.check_recover_share_compatible_with_key(
            access_structure_ref,
            recover_share,
            encryption_key,
        )?;

        let share_index = recover_share.held_share.share_image.index;

        self.mutate(Mutation::Keygen(keys::KeyMutation::NewShare {
            access_structure_ref,
            device_id: recover_share.held_by,
            share_index,
        }));

        let was_a_physical_backup = recover_share.held_share.access_structure_ref.is_none();

        if was_a_physical_backup {
            self.mutate(Mutation::Restoration(
                RestorationMutation::DeviceNeedsConsolidation(PendingConsolidation {
                    device_id: recover_share.held_by,
                    access_structure_ref,
                    share_index,
                }),
            ))
        }

        Ok(())
    }

    pub fn check_recover_share_compatible_with_key(
        &self,
        access_structure_ref: AccessStructureRef,
        recover_share: &RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<(), RecoverShareError> {
        let access_structure = self
            .get_access_structure(access_structure_ref)
            .ok_or(RecoverShareError::NoSuchAccessStructure)?;

        if let Some(got) = recover_share.held_share.access_structure_ref {
            if got != access_structure_ref {
                return Err(RecoverShareError::AccessStructureMismatch);
            }
        }
        let frost_key = self
            .get_frost_key(access_structure_ref.key_id)
            .ok_or(RecoverShareError::NoSuchAccessStructure)?;

        if access_structure
            .device_to_share_index
            .contains_key(&recover_share.held_by)
        {
            return Err(RecoverShareError::AlreadyGotThisShare);
        }

        let root_shared_key = frost_key
            .complete_key
            .root_shared_key(access_structure_ref.access_structure_id, encryption_key)
            .ok_or(RecoverShareError::DecryptionError)?;

        let share_image = recover_share.held_share.share_image;

        let expected_image = root_shared_key.share_image(share_image.index);

        if expected_image != share_image {
            return Err(RecoverShareError::ShareImageIsWrong);
        }
        Ok(())
    }

    pub fn cancel_restoration(&mut self, restoration_id: RestorationId) {
        self.mutate(Mutation::Restoration(
            RestorationMutation::DeleteRestoration { restoration_id },
        ))
    }

    pub fn start_restoring_key_from_recover_share(
        &mut self,
        recover_share: &RecoverShare,
        restoration_id: RestorationId,
    ) {
        let held_share = &recover_share.held_share;
        assert!(!self.restoration.restorations.contains_key(&restoration_id));
        if let Some(access_structure_ref) = held_share.access_structure_ref {
            assert!(self.get_access_structure(access_structure_ref).is_none());
        }

        self.mutate(Mutation::Restoration(RestorationMutation::NewRestoration {
            restoration_id,
            key_name: held_share.key_name.clone(),
            threshold: held_share.threshold,
            key_purpose: held_share.purpose,
        }));

        self.mutate(Mutation::Restoration(
            RestorationMutation::RestorationProgress {
                restoration_id,
                device_id: recover_share.held_by,
                held_share: held_share.clone(),
            },
        ));
    }

    pub fn recv_restoration_message(
        &mut self,
        from: DeviceId,
        message: DeviceRestoration,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        match message {
            DeviceRestoration::PhysicalEntered(entered_physical_backup) => {
                //XXX: We could check if a restoration id exists before sending out the message but
                // it's not a good idea becuase atm it's valid to ask a device to enter a backup
                // when you're not keeping track of the restoration id for the purpose of doing a
                // backup check.
                Ok(vec![CoordinatorSend::ToUser(
                    CoordinatorToUserMessage::Restoration(
                        ToUserRestoration::PhysicalBackupEntered(Box::new(PhysicalBackupPhase {
                            backup: entered_physical_backup,
                            from,
                        })),
                    ),
                )])
            }
            DeviceRestoration::PhysicalSaved(share_image) => {
                if let Some(restoration_id) = self
                    .restoration
                    .tmp_waiting_save
                    .remove(&(from, share_image))
                {
                    // Get restoration state to construct HeldShare
                    let restoration_state = self.restoration.restorations.get(&restoration_id);
                    if let Some(state) = restoration_state {
                        let held_share = HeldShare {
                            access_structure_ref: None,
                            share_image,
                            threshold: state.access_structure.threshold,
                            key_name: state.key_name.clone(),
                            purpose: state.key_purpose,
                        };

                        self.mutate(Mutation::Restoration(
                            RestorationMutation::RestorationProgress {
                                restoration_id,
                                device_id: from,
                                held_share,
                            },
                        ));

                        Ok(vec![CoordinatorSend::ToUser(
                            CoordinatorToUserMessage::Restoration(
                                ToUserRestoration::PhysicalBackupSaved {
                                    device_id: from,
                                    restoration_id,
                                    share_index: share_image.index,
                                },
                            ),
                        )])
                    } else {
                        Err(Error::coordinator_invalid_message(
                            message.kind(),
                            "restoration not found",
                        ))
                    }
                } else {
                    Err(Error::coordinator_invalid_message(
                        message.kind(),
                        "coordinator not waiting for that share to be saved",
                    ))
                }
            }
            DeviceRestoration::FinishedConsolidation {
                access_structure_ref,
                share_index,
            } => {
                let consolidation = PendingConsolidation {
                    device_id: from,
                    access_structure_ref,
                    share_index,
                };
                // we have to distinguish between two types of finished consolidations:
                //
                // 1. We've just asked the device to enter a backup for a access structure we knew about when we asked them
                // 2. We asked them earlier to enter it before we have restored the access structure
                // and now we've connected the device again we need to consolidate before we use it.
                if self
                    .restoration
                    .tmp_waiting_consolidate
                    .remove(&consolidation)
                {
                    self.mutate(Mutation::Keygen(keys::KeyMutation::NewShare {
                        access_structure_ref,
                        device_id: from,
                        share_index,
                    }));
                } else if self
                    .restoration
                    .pending_physical_consolidations
                    .contains(&consolidation)
                {
                    self.mutate(Mutation::Restoration(
                        RestorationMutation::DeviceFinishedConsolidation(consolidation),
                    ));
                } else {
                    return Err(Error::coordinator_invalid_message(
                        message.kind(),
                        "not waiting for device to consolidate",
                    ));
                }

                Ok(vec![CoordinatorSend::ToUser(
                    CoordinatorToUserMessage::Restoration(
                        ToUserRestoration::FinishedConsolidation {
                            device_id: from,
                            access_structure_ref,
                            share_index,
                        },
                    ),
                )])
            }
            DeviceRestoration::LegacyHeldShares(legacy_held_shares) => {
                // Convert legacy shares to new format
                let held_shares: Vec<HeldShare> = legacy_held_shares
                    .into_iter()
                    .map(|legacy| legacy.into())
                    .collect();
                Ok(vec![CoordinatorSend::ToUser(
                    ToUserRestoration::GotHeldShares {
                        held_by: from,
                        shares: held_shares,
                    }
                    .into(),
                )])
            }
            DeviceRestoration::HeldShares(held_shares) => Ok(vec![CoordinatorSend::ToUser(
                ToUserRestoration::GotHeldShares {
                    held_by: from,
                    shares: held_shares,
                }
                .into(),
            )]),
        }
    }

    pub fn has_backups_that_need_to_be_consolidated(&self, device_id: DeviceId) -> bool {
        self.restoration
            .pending_physical_consolidations
            .iter()
            .any(|consolidation| consolidation.device_id == device_id)
    }

    pub fn consolidate_pending_physical_backups(
        &self,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> impl IntoIterator<Item = CoordinatorSend> {
        let consolidations = self
            .restoration
            .pending_physical_consolidations
            .iter()
            .filter(|pending| pending.device_id == device_id);

        let mut messages = vec![];

        for consolidation in consolidations {
            let root_shared_key = self
                .root_shared_key(consolidation.access_structure_ref, encryption_key)
                .expect("invariant");
            let frost_key = self
                .get_frost_key(consolidation.access_structure_ref.key_id)
                .expect("invariant");

            messages.push(CoordinatorSend::ToDevice {
                message: CoordinatorToDeviceMessage::Restoration(
                    CoordinatorRestoration::Consolidate(Box::new(ConsolidateBackup {
                        share_index: consolidation.share_index,
                        root_shared_key,
                        key_name: frost_key.key_name.clone(),
                        purpose: frost_key.purpose,
                    })),
                ),
                destinations: [device_id].into(),
            });
        }

        messages
    }

    pub fn request_device_display_backup(
        &mut self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        encryption_key: SymmetricKey,
    ) -> Result<Vec<CoordinatorSend>, ActionError> {
        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;
        let complete_key = &self
            .keys
            .get(&key_id)
            .ok_or(ActionError::StateInconsistent("no such key".into()))?
            .complete_key;

        let access_structure = complete_key
            .access_structures
            .get(&access_structure_id)
            .ok_or(ActionError::StateInconsistent(
                "no such access structure".into(),
            ))?;
        let party_index = *access_structure
            .device_to_share_index
            .get(&device_id)
            .ok_or(ActionError::StateInconsistent(
                "device does not have share in key".into(),
            ))?;
        let root_shared_key = complete_key
            .root_shared_key(access_structure_id, encryption_key)
            .ok_or(ActionError::StateInconsistent(
                "couldn't decrypt root key".into(),
            ))?;
        let (_, coord_share_decryption_contrib) = complete_key
            .coord_share_decryption_contrib(access_structure_id, device_id, encryption_key)
            .ok_or(ActionError::StateInconsistent(
                "couldn't decrypt root key".into(),
            ))?;
        Ok(vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(
                CoordinatorRestoration::DisplayBackup {
                    access_structure_ref,
                    coord_share_decryption_contrib,
                    party_index,
                    root_shared_key,
                },
            ),
            destinations: BTreeSet::from_iter([device_id]),
        }])
    }

    /// Delete a restoration share. For now we refer to it by `device_id` but it would be better to
    /// refer to the explicit share in the future (which is what the mutation does).
    pub fn delete_restoration_share(&mut self, restoration_id: RestorationId, device_id: DeviceId) {
        if let Some(restoration) = self.restoration.restorations.get(&restoration_id) {
            if let Some(share_image) = restoration
                .access_structure
                .held_shares
                .iter()
                .find(|recover_share| recover_share.held_by == device_id)
                .map(|recover_share| recover_share.held_share.share_image)
            {
                self.mutate(Mutation::Restoration(
                    RestorationMutation::DeleteRestorationShare {
                        device_id,
                        restoration_id,
                        share_image,
                    },
                ));
            }
        }
    }

    pub fn restoring(&self) -> impl Iterator<Item = RestorationState> + '_ {
        self.restoration.restorations.values().cloned()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq, Kind)]
pub enum RestorationMutation {
    LegacyNewRestoration {
        restoration_id: RestorationId,
        key_name: String,
        threshold: u16,
        key_purpose: KeyPurpose,
    },
    LegacyRestorationProgress {
        restoration_id: RestorationId,
        device_id: DeviceId,
        share_image: ShareImage,
        access_structure_ref: Option<AccessStructureRef>,
    },
    DeleteRestorationShare {
        restoration_id: RestorationId,
        device_id: DeviceId,
        share_image: ShareImage,
    },
    /// Can be used to cancel a restoration or indicate its data can be purged after a restoration
    /// is finished.
    DeleteRestoration {
        restoration_id: RestorationId,
    },
    /// A device was restored with a physical backup -- the next time it connects we need to
    /// consolidate the physical backup.
    DeviceNeedsConsolidation(PendingConsolidation),
    DeviceFinishedConsolidation(PendingConsolidation),
    NewRestoration {
        restoration_id: RestorationId,
        key_name: String,
        threshold: Option<u16>,
        key_purpose: KeyPurpose,
    },
    RestorationProgress {
        restoration_id: RestorationId,
        device_id: DeviceId,
        held_share: HeldShare,
    },
}

impl RestorationMutation {
    pub fn tied_to_key(&self) -> Option<KeyId> {
        use RestorationMutation::*;
        Some(match self {
            DeviceFinishedConsolidation(pending) | DeviceNeedsConsolidation(pending) => {
                pending.access_structure_ref.key_id
            }
            _ => {
                return None;
            }
        })
    }

    pub fn tied_to_restoration(&self) -> Option<RestorationId> {
        use RestorationMutation::*;
        match self {
            &LegacyNewRestoration { restoration_id, .. }
            | &NewRestoration { restoration_id, .. }
            | &LegacyRestorationProgress { restoration_id, .. }
            | &RestorationProgress { restoration_id, .. }
            | &DeleteRestoration { restoration_id }
            | &DeleteRestorationShare { restoration_id, .. } => Some(restoration_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ToUserRestoration {
    GotHeldShares {
        held_by: DeviceId,
        shares: Vec<HeldShare>,
    },
    PhysicalBackupEntered(Box<PhysicalBackupPhase>),
    PhysicalBackupSaved {
        device_id: DeviceId,
        restoration_id: RestorationId,
        share_index: ShareIndex,
    },
    FinishedConsolidation {
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        share_index: ShareIndex,
    },
}

impl From<ToUserRestoration> for CoordinatorToUserMessage {
    fn from(value: ToUserRestoration) -> Self {
        CoordinatorToUserMessage::Restoration(value)
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, bincode::Encode, bincode::Decode,
)]
pub struct PendingConsolidation {
    pub device_id: DeviceId,
    pub access_structure_ref: AccessStructureRef,
    pub share_index: ShareIndex,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalBackupPhase {
    pub backup: EnteredPhysicalBackup,
    pub from: DeviceId,
}

#[derive(Debug, Clone)]
pub struct TellDeviceConsolidateBackup {
    pub device_id: DeviceId,
    pub share_index: ShareIndex,
    pub root_shared_key: SharedKey,
    pub key_name: String,
    pub purpose: KeyPurpose,
}

impl IntoIterator for TellDeviceConsolidateBackup {
    type Item = CoordinatorSend;
    type IntoIter = core::iter::Once<CoordinatorSend>;

    fn into_iter(self) -> Self::IntoIter {
        core::iter::once(CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(CoordinatorRestoration::Consolidate(
                Box::new(ConsolidateBackup {
                    share_index: self.share_index,
                    root_shared_key: self.root_shared_key,
                    key_name: self.key_name,
                    purpose: self.purpose,
                }),
            )),
            destinations: [self.device_id].into(),
        })
    }
}

#[derive(Debug, Clone)]
pub enum CheckBackupError {
    /// The access structure for the share isn't known to the coordinator
    NoSuchAccessStructure,
    /// Share image is wrong
    ShareImageIsWrong,
    /// The application provided the wrong decryption key so we couldn't verify the new key share.
    DecryptionError,
}

impl fmt::Display for CheckBackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckBackupError::NoSuchAccessStructure => write!(
                f,
                "The access structure for the share isn't known to the coordinator"
            ),
            CheckBackupError::ShareImageIsWrong => {
                write!(f, "The share image was not what was expected")
            }
            CheckBackupError::DecryptionError => {
                write!(f, "The application provided the wrong decryption key so we couldn't verify the share.")
            }
        }
    }
}

impl std::error::Error for CheckBackupError {}

#[derive(Debug, Clone)]
pub enum RestorationError {
    /// The restoration session no longer exists
    UnknownRestorationId,
    /// You can't restore yet since you don't have enough shares
    NotEnoughShares,
    /// The shares interpolated but didn't match the access structure id expected
    InterpolationDoesntMatch,
    /// Threshold doesn't match. The threshold is wrong a backup was entered wrongly.
    ThresholdDoesntMatch { expected: u16, got: u16 },
}

impl fmt::Display for RestorationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestorationError::UnknownRestorationId => {
                write!(f, "The restoration session no longer exists")
            }
            RestorationError::NotEnoughShares => write!(f, "Not enough shares to restore"),
            RestorationError::InterpolationDoesntMatch => write!(
                f,
                "Interpolated shares did not match the expected access structure ID"
            ),
            RestorationError::ThresholdDoesntMatch { expected, got } => write!(
                f,
                "The threshold was entered wrongly or one of the shares is wrong. Expected a threshold of {expected}, got {got}",
            ),
        }
    }
}

impl std::error::Error for RestorationError {}

/// An error occuring when you try and an a "recover share" to a restoration session
#[derive(Debug, Clone)]
pub enum RestoreRecoverShareError {
    /// The name of the key doesn't match
    NameMismatch,
    /// The restoration session no longer exists
    UnknownRestorationId,
    /// The key share is use by the device for a different purpose than the restoration session
    PurposeNotCompatible,
    /// Access structure doesn't match one of the other shares
    AcccessStructureMismatch,
    /// Already know this device has this share
    AlreadyGotThisShare,
    /// The share image that this device claims exists at this index contradicts another device in the restoration.
    ConflictingShareImage { conflicts_with: DeviceId },
}

#[derive(Debug, Clone)]
pub enum RestorePhysicalBackupError {
    UnknownRestorationId,
    AlreadyGotThisShare,
    ConflictingShareImage { conflicts_with: DeviceId },
}

impl fmt::Display for RestorePhysicalBackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestorePhysicalBackupError::UnknownRestorationId => {
                write!(f, "Coordinator didn't have the restoration id")
            }
            RestorePhysicalBackupError::AlreadyGotThisShare => {
                write!(f, "Already know this device has this share")
            }
            RestorePhysicalBackupError::ConflictingShareImage { conflicts_with } => {
                write!(f, "The device {conflicts_with} has already submitted a backup with that index but with a different share image")
            }
        }
    }
}

impl std::error::Error for RestorePhysicalBackupError {}

impl fmt::Display for RestoreRecoverShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestoreRecoverShareError::UnknownRestorationId => {
                write!(f, "Coordinator didn't have the restoration id")
            }
            RestoreRecoverShareError::PurposeNotCompatible => {
                write!(f, "The key share is use by the device for a different purpose than the restoration session")
            }
            RestoreRecoverShareError::AcccessStructureMismatch => {
                write!(f, "Access structure doesn't match one of the other shares")
            }
            RestoreRecoverShareError::AlreadyGotThisShare => {
                write!(f, "Already know this device has this share")
            }
            RestoreRecoverShareError::NameMismatch => {
                write!(
                    f,
                    "The name of the key being restored and the one in the share is not the same"
                )
            }
            RestoreRecoverShareError::ConflictingShareImage { conflicts_with } => {
                write!(f, "The device {conflicts_with} has already submitted a backup with that index but with a different share image")
            }
        }
    }
}

impl std::error::Error for RestoreRecoverShareError {}

/// An error when you try to recover a share to a known access structure
#[derive(Debug, Clone)]
pub enum RecoverShareError {
    /// The coordinator already knows about this share
    AlreadyGotThisShare,
    /// The access structure for the share isn't known to the coordinator
    NoSuchAccessStructure,
    /// Access structure for this share wasn't the same as the one you were trying to recover to
    AccessStructureMismatch,
    /// Share image is wrong
    ShareImageIsWrong,
    /// The application provided the wrong decryption key so we couldn't verify the new key share.
    DecryptionError,
}

impl fmt::Display for RecoverShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoverShareError::AlreadyGotThisShare => {
                write!(f, "The coordinator already knows about this share")
            }
            RecoverShareError::NoSuchAccessStructure => write!(
                f,
                "The access structure for the share isn't known to the coordinator"
            ),
            RecoverShareError::ShareImageIsWrong => {
                write!(f, "The share image was not what was expected")
            }
            RecoverShareError::DecryptionError => {
                write!(f, "The application provided the wrong decryption key so we couldn't verify the new key share.")
            }
            RecoverShareError::AccessStructureMismatch => {
                write!(
                    f,
                    "The recoverable share is for a different access structure"
                )
            }
        }
    }
}

impl std::error::Error for RecoverShareError {}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RecoveringAccessStructure {
    pub threshold: Option<u16>,
    pub held_shares: Vec<RecoverShare>,
}

impl RecoveringAccessStructure {
    pub fn get_access_structure_ref(&self) -> Option<AccessStructureRef> {
        self.held_shares.iter()
            .find_map(|recover_share| recover_share.held_share.access_structure_ref)
    }

    pub fn get_effective_threshold(&self) -> Option<u16> {
        self.threshold.or_else(|| {
            self.held_shares.iter()
                .find_map(|recover_share| recover_share.held_share.threshold)
        })
    }

    pub fn has_got_share_image(&self, device_id: DeviceId, share_image: ShareImage) -> bool {
        self.held_shares.iter()
            .any(|recover_share| recover_share.held_by == device_id && recover_share.held_share.share_image == share_image)
    }

    pub fn contradicts(&self, share_image: ShareImage) -> Option<DeviceId> {
        self.held_shares.iter()
            .find(|recover_share| recover_share.held_share.share_image.index == share_image.index && recover_share.held_share.share_image != share_image)
            .map(|recover_share| recover_share.held_by)
    }

    /// Try to recover using frost_backup's find_valid_subset
    /// Returns the SharedKey and indices of compatible shares
    pub fn try_fuzzy_recovery(&self, fingerprint: schnorr_fun::frost::Fingerprint) -> Option<(SharedKey, Vec<usize>)> {
        // We need at least 2 shares for recovery
        if self.held_shares.len() < 2 {
            return None;
        }

        let share_images: Vec<ShareImage> = self.held_shares.iter()
            .map(|recover_share| recover_share.held_share.share_image)
            .collect();

        let threshold = self.get_effective_threshold().map(|t| t as usize);

        // Use frost_backup's find_valid_subset to find compatible share images
        // This will try different combinations and thresholds to find a valid set
        use frost_backup::recovery::find_valid_subset;

        let (compatible_images, shared_key) = find_valid_subset(&share_images, fingerprint, threshold)?;

        // Convert the compatible ShareImages back to indices in our held_shares array
        let compatible_indices: Vec<usize> = self.held_shares
            .iter()
            .enumerate()
            .filter_map(|(i, recover_share)| {
                if compatible_images.contains(&recover_share.held_share.share_image) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        // Convert to non-zero SharedKey
        let shared_key = shared_key.non_zero()?;

        Some((shared_key, compatible_indices))
    }

    pub fn progress(&self) -> u16 {
        self.held_shares
            .iter()
            .map(|recover_share| recover_share.held_share.share_image.index)
            .collect::<BTreeSet<_>>()
            .len()
            .try_into()
            .unwrap()
    }



    pub fn has_got_share_index(&self, share_index: ShareIndex) -> bool {
        self.held_shares
            .iter()
            .any(|recover_share| recover_share.held_share.share_image.index == share_index)
    }

    pub fn has_got_share(&self, device_id: DeviceId, share_image: ShareImage) -> bool {
        self.held_shares
            .iter()
            .any(|recover_share| recover_share.held_by == device_id && recover_share.held_share.share_image == share_image)
    }

    pub fn has_got_from(&self, device_id: DeviceId) -> bool {
        self.get_device_contribution(device_id).is_some()
    }

    pub fn get_device_contribution(&self, device_id: DeviceId) -> Option<ShareImage> {
        self.held_shares
            .iter()
            .find(|recover_share| recover_share.held_by == device_id)
            .map(|recover_share| recover_share.held_share.share_image)
    }
}
