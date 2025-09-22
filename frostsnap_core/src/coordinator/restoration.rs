use std::collections::BTreeSet;

use super::keys;
use super::*;
use crate::{fail, message::HeldShare2, EnterPhysicalId, RestorationId};

#[derive(Clone, Debug, PartialEq)]
pub struct RestorationState {
    pub restoration_id: RestorationId,
    pub key_name: String,
    pub access_structure: RecoveringAccessStructure,
    pub key_purpose: KeyPurpose,
    pub fingerprint: schnorr_fun::frost::Fingerprint,
}

impl RestorationState {
    pub fn is_restorable(&self) -> bool {
        self.status().shared_key.is_some()
    }

    pub fn needs_to_consolidate(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.access_structure
            .held_shares
            .iter()
            .filter(|share| share.held_share.needs_consolidation)
            .map(|share| share.held_by)
    }

    /// Prepare for saving a physical backup - returns the HeldShare2 to store
    pub fn prepare_save_physical_backup(
        &self,
        device_id: DeviceId,
        share_image: ShareImage,
    ) -> HeldShare2 {
        // Clone access structure and do a trial run to see what threshold we would get after adding this share
        let mut trial_access_structure = self.access_structure.clone();
        // this models what the device's "HeldShare2" would look like after it saves the backup
        let mut held_share = HeldShare2 {
            access_structure_ref: None,
            share_image,
            threshold: None,
            key_name: Some(self.key_name.clone()),
            purpose: Some(self.key_purpose),
            needs_consolidation: true,
        };
        let trial_recover_share = RecoverShare {
            held_by: device_id,
            held_share: held_share.clone(),
        };
        trial_access_structure.add_share(trial_recover_share, self.fingerprint);

        if let Some(_shared_key) = &trial_access_structure.shared_key {
            // NOTE: If the restoration has succeeded with this new share we
            // populate the access structure metadata. Note that we *could*
            // consolidate at this point if we wanted to by sending the
            // shared_key over but to keep things simple and predictable we
            // consolidate only after the user has confirmed the restoration.
            held_share.threshold = trial_access_structure.effective_threshold();
            held_share.access_structure_ref = trial_access_structure.access_structure_ref();
        }

        held_share
    }

    pub fn status(&self) -> RestorationStatus {
        let shared_key = self.access_structure.shared_key.as_ref();
        let restoration_access_ref = self.access_structure.access_structure_ref();

        let shares = self
            .access_structure
            .held_shares
            .iter()
            .map(|recover_share| {
                let compatibility = if let Some(key) = shared_key {
                    let expected_image =
                        key.share_image(recover_share.held_share.share_image.index);
                    if expected_image == recover_share.held_share.share_image {
                        ShareCompatibility::Compatible
                    } else {
                        ShareCompatibility::Incompatible
                    }
                } else if let (Some(restoration_ref), Some(share_ref)) = (
                    restoration_access_ref,
                    recover_share.held_share.access_structure_ref,
                ) {
                    if restoration_ref == share_ref {
                        ShareCompatibility::Compatible
                    } else {
                        ShareCompatibility::Incompatible
                    }
                } else {
                    ShareCompatibility::Uncertain
                };

                RestorationShare {
                    device_id: recover_share.held_by,
                    index: recover_share
                        .held_share
                        .share_image
                        .index
                        .try_into()
                        .expect("share index is small"),
                    compatibility,
                }
            })
            .collect();

        RestorationStatus {
            threshold: self.access_structure.effective_threshold(),
            shares,
            shared_key: shared_key.cloned(),
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
    pub fn share_count(&self) -> ShareCount {
        let incompatible = self
            .shares
            .iter()
            .filter(|s| s.compatibility == ShareCompatibility::Incompatible)
            .count() as u16;

        // When threshold is unknown, count all unique indices (compatibility not determined yet)
        // When threshold is known, count only compatible unique indices
        let got = if self.threshold.is_some() {
            self.shares
                .iter()
                .filter(|s| s.compatibility == ShareCompatibility::Compatible)
                .map(|s| s.index)
                .collect::<std::collections::BTreeSet<_>>()
                .len() as u16
        } else {
            self.shares
                .iter()
                .map(|s| s.index)
                .collect::<std::collections::BTreeSet<_>>()
                .len() as u16
        };

        let needed = self.threshold;

        ShareCount {
            got,
            needed,
            incompatible,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShareCount {
    pub got: u16,
    pub needed: Option<u16>,
    pub incompatible: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub enum ShareCompatibility {
    Compatible,
    Incompatible,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestorationShare {
    pub device_id: DeviceId,
    pub index: u16,
    pub compatibility: ShareCompatibility,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RecoverShare {
    pub held_by: DeviceId,
    pub held_share: HeldShare2,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub(super) restorations: BTreeMap<RestorationId, restoration::RestorationState>,
    /// This is where we remember consolidations we need to do from restorations we've finished.
    pub(super) pending_physical_consolidations: BTreeSet<PendingConsolidation>,
    /// For when we ask a device to enter a backup and we plan to immediately consolidate it because
    /// we already know the access structure.
    tmp_waiting_consolidate: BTreeSet<PendingConsolidation>,

    tmp_waiting_save: BTreeMap<(DeviceId, ShareImage), (RestorationId, HeldShare2)>,
}

impl State {
    pub fn apply_mutation_restoration(
        &mut self,
        mutation: RestorationMutation,
        fingerprint: schnorr_fun::frost::Fingerprint,
    ) -> Option<RestorationMutation> {
        use RestorationMutation::*;
        match mutation {
            NewRestoration {
                restoration_id,
                ref key_name,
                threshold,
                key_purpose,
            } => {
                // Convert legacy to new and recurse
                return self.apply_mutation_restoration(
                    NewRestoration2 {
                        restoration_id,
                        key_name: key_name.clone(),
                        starting_threshold: Some(threshold),
                        key_purpose,
                    },
                    fingerprint,
                );
            }
            NewRestoration2 {
                restoration_id,
                ref key_name,
                starting_threshold: threshold,
                key_purpose,
            } => {
                self.restorations.insert(
                    restoration_id,
                    RestorationState {
                        restoration_id,
                        key_name: key_name.clone(),
                        access_structure: RecoveringAccessStructure {
                            starting_threshold: threshold,
                            held_shares: Default::default(),
                            shared_key: None,
                        },
                        key_purpose,
                        fingerprint,
                    },
                );
            }
            RestorationProgress {
                restoration_id,
                device_id,
                access_structure_ref,
                share_image,
            } => {
                // Convert legacy to new format
                let held_share = HeldShare2 {
                    access_structure_ref,
                    share_image,
                    threshold: None,
                    key_name: self
                        .restorations
                        .get(&restoration_id)
                        .map(|s| s.key_name.clone()),
                    purpose: self
                        .restorations
                        .get(&restoration_id)
                        .map(|s| s.key_purpose),
                    needs_consolidation: access_structure_ref.is_none(),
                };
                return self.apply_mutation_restoration(
                    RestorationProgress2 {
                        restoration_id,
                        device_id,
                        held_share,
                    },
                    fingerprint,
                );
            }
            RestorationProgress2 {
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
                        if let Some(existing_ref) = state.access_structure.access_structure_ref() {
                            if existing_ref != new_ref {
                                fail!("access_structure_ref didn't match");
                            }
                        }
                    }

                    let recover_share = RecoverShare {
                        held_by: device_id,
                        held_share: held_share.clone(),
                    };
                    state.access_structure.add_share(recover_share, fingerprint);
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
                    let pos = restoration.access_structure.held_shares.iter().position(
                        |recover_share| {
                            recover_share.held_by == device_id
                                && recover_share.held_share.share_image == share_image
                        },
                    )?;
                    restoration.access_structure.held_shares.remove(pos);
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
        self.mutate(Mutation::Restoration(
            RestorationMutation::NewRestoration2 {
                restoration_id,
                key_name,
                starting_threshold: threshold,
                key_purpose,
            },
        ));
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
        let state = match self.get_restoration_state(restoration_id) {
            Some(state) => state,
            None => return vec![],
        };

        let PhysicalBackupPhase {
            backup: EnteredPhysicalBackup { share_image, .. },
            from,
        } = phase;

        // Prepare the HeldShare
        let held_share = state.prepare_save_physical_backup(from, share_image);

        // Save the restoration_id and held_share for when the device confirms
        self.restoration
            .tmp_waiting_save
            .insert((from, share_image), (restoration_id, held_share.clone()));

        vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(
                CoordinatorRestoration::SavePhysicalBackup2(Box::new(held_share)),
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
        encryption_key: SymmetricKey,
    ) -> Result<(), RestoreRecoverShareError> {
        self.check_recover_share_compatible_with_restoration(
            restoration_id,
            recover_share,
            encryption_key,
        )?;
        self.mutate(Mutation::Restoration(
            RestorationMutation::RestorationProgress2 {
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
        encryption_key: SymmetricKey,
    ) -> Result<(), RestoreRecoverShareError> {
        let restoration = self
            .restoration
            .restorations
            .get(&restoration_id)
            .ok_or(RestoreRecoverShareError::UnknownRestorationId)?;

        // ❗ Don't check purpose or key_name - they could be wrong or missing
        // in physical backups. They are informational but shouldn't prevent it
        // being added to the restoration. If they are not compatible the access
        // structure ref check will catch it immediately or the fingerprint check will
        // exclude it from the restoration later.

        // Use find_share to check if share exists elsewhere
        if let Some(location) =
            self.find_share(recover_share.held_share.share_image, encryption_key)
        {
            match location.key_state {
                KeyLocationState::Restoring {
                    restoration_id: found_id,
                } if found_id == restoration_id => {
                    // Found in same restoration
                    if location.device_ids.contains(&recover_share.held_by) {
                        // Same device in same restoration
                        return Err(RestoreRecoverShareError::AlreadyGotThisShare);
                    }
                    // Different device in same restoration is OK (adding redundancy)
                }
                _ => {
                    // Found in different restoration or complete wallet
                    return Err(RestoreRecoverShareError::ShareBelongsElsewhere {
                        location: Box::new(location),
                    });
                }
            }
        }

        // Check AccessStructureRef compatibility
        let new_ref = recover_share.held_share.access_structure_ref;
        let existing_ref = restoration.access_structure.access_structure_ref();

        if let (Some(new), Some(existing)) = (new_ref, existing_ref) {
            if new != existing {
                return Err(RestoreRecoverShareError::AcccessStructureMismatch);
            }
        }

        Ok(())
    }

    pub fn check_physical_backup_compatible_with_restoration(
        &self,
        restoration_id: RestorationId,
        phase: PhysicalBackupPhase,
        encryption_key: SymmetricKey,
    ) -> Result<(), RestorePhysicalBackupError> {
        self.restoration
            .restorations
            .get(&restoration_id)
            .ok_or(RestorePhysicalBackupError::UnknownRestorationId)?;

        // Use find_share to check if share exists elsewhere
        if let Some(location) = self.find_share(phase.backup.share_image, encryption_key) {
            match location.key_state {
                KeyLocationState::Restoring {
                    restoration_id: found_id,
                } if found_id == restoration_id => {
                    // Found in same restoration
                    if location.device_ids.contains(&phase.from) {
                        // Same device in same restoration
                        return Err(RestorePhysicalBackupError::AlreadyGotThisShare);
                    }
                    // Different device in same restoration is OK (adding redundancy)
                }
                _ => {
                    // Found in different restoration or complete wallet
                    return Err(RestorePhysicalBackupError::ShareBelongsElsewhere {
                        location: Box::new(location),
                    });
                }
            }
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

        // Get cached shared key
        let root_shared_key = state
            .access_structure
            .shared_key
            .as_ref()
            .ok_or(RestorationError::NotEnoughShares)?
            .clone();

        debug_assert!(
            state
                .access_structure
                .starting_threshold
                .map(|t| t as usize == root_shared_key.threshold())
                .unwrap_or(true),
            "shared_key threshold must match starting_threshold if one was specified"
        );

        let access_structure_ref = AccessStructureRef::from_root_shared_key(&root_shared_key);

        let device_to_share_index = state
            .access_structure
            .compatible_device_to_share_index(&root_shared_key);

        self.mutate_new_key(
            state.key_name.clone(),
            root_shared_key,
            device_to_share_index.clone(),
            encryption_key,
            state.key_purpose,
            rng,
        );

        for device_id in state
            .needs_to_consolidate()
            .filter(|device_id| device_to_share_index.contains_key(device_id))
        {
            self.mutate(Mutation::Restoration(
                RestorationMutation::DeviceNeedsConsolidation(PendingConsolidation {
                    device_id,
                    access_structure_ref,
                    share_index: device_to_share_index[&device_id],
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
        let frost_key =
            self.get_frost_key(access_structure_ref.key_id)
                .ok_or(RecoverShareError {
                    key_purpose: self
                        .keys
                        .get(&access_structure_ref.key_id)
                        .map(|k| k.purpose)
                        .unwrap_or(KeyPurpose::Test),
                    kind: RecoverShareErrorKind::NoSuchAccessStructure,
                })?;

        let key_purpose = frost_key.purpose;

        let access_structure =
            self.get_access_structure(access_structure_ref)
                .ok_or(RecoverShareError {
                    key_purpose,
                    kind: RecoverShareErrorKind::NoSuchAccessStructure,
                })?;

        if let Some(got) = recover_share.held_share.access_structure_ref {
            if got != access_structure_ref {
                return Err(RecoverShareError {
                    key_purpose,
                    kind: RecoverShareErrorKind::AccessStructureMismatch,
                });
            }
        }

        if access_structure
            .device_to_share_index
            .contains_key(&recover_share.held_by)
        {
            return Err(RecoverShareError {
                key_purpose,
                kind: RecoverShareErrorKind::AlreadyGotThisShare,
            });
        }

        let root_shared_key = frost_key
            .complete_key
            .root_shared_key(access_structure_ref.access_structure_id, encryption_key)
            .ok_or(RecoverShareError {
                key_purpose,
                kind: RecoverShareErrorKind::DecryptionError,
            })?;

        let share_image = recover_share.held_share.share_image;

        let expected_image = root_shared_key.share_image(share_image.index);

        if expected_image != share_image {
            return Err(RecoverShareError {
                key_purpose,
                kind: RecoverShareErrorKind::ShareImageIsWrong,
            });
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
    ) -> Result<(), StartRestorationFromShareError> {
        let held_share = &recover_share.held_share;

        // Check if key_name and purpose are present
        let key_name = held_share
            .key_name
            .clone()
            .ok_or(StartRestorationFromShareError::MissingMetadata)?;
        let key_purpose = held_share
            .purpose
            .ok_or(StartRestorationFromShareError::MissingMetadata)?;

        assert!(!self.restoration.restorations.contains_key(&restoration_id));
        if let Some(access_structure_ref) = held_share.access_structure_ref {
            assert!(self.get_access_structure(access_structure_ref).is_none());
        }

        self.mutate(Mutation::Restoration(
            RestorationMutation::NewRestoration2 {
                restoration_id,
                key_name,
                starting_threshold: held_share.threshold,
                key_purpose,
            },
        ));

        self.mutate(Mutation::Restoration(
            RestorationMutation::RestorationProgress2 {
                restoration_id,
                device_id: recover_share.held_by,
                held_share: held_share.clone(),
            },
        ));

        Ok(())
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
                if let Some((restoration_id, held_share)) = self
                    .restoration
                    .tmp_waiting_save
                    .remove(&(from, share_image))
                {
                    self.mutate(Mutation::Restoration(
                        RestorationMutation::RestorationProgress2 {
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
            DeviceRestoration::HeldShares(legacy_held_shares) => {
                // Convert legacy shares to new format
                let held_shares: Vec<HeldShare2> = legacy_held_shares
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
            DeviceRestoration::HeldShares2(held_shares) => Ok(vec![CoordinatorSend::ToUser(
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
    NewRestoration {
        restoration_id: RestorationId,
        key_name: String,
        threshold: u16,
        key_purpose: KeyPurpose,
    },
    RestorationProgress {
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
    NewRestoration2 {
        restoration_id: RestorationId,
        key_name: String,
        starting_threshold: Option<u16>,
        key_purpose: KeyPurpose,
    },
    RestorationProgress2 {
        restoration_id: RestorationId,
        device_id: DeviceId,
        held_share: HeldShare2,
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
            &NewRestoration { restoration_id, .. }
            | &NewRestoration2 { restoration_id, .. }
            | &RestorationProgress { restoration_id, .. }
            | &RestorationProgress2 { restoration_id, .. }
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
        shares: Vec<HeldShare2>,
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

impl PhysicalBackupPhase {
    pub fn device_id(&self) -> DeviceId {
        self.from
    }

    pub fn share_image(&self) -> ShareImage {
        self.backup.share_image
    }
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
}

impl fmt::Display for RestorationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestorationError::UnknownRestorationId => {
                write!(f, "The restoration session no longer exists")
            }
            RestorationError::NotEnoughShares => write!(f, "Not enough shares to restore"),
        }
    }
}

impl std::error::Error for RestorationError {}

/// An error occuring when you try and an a "recover share" to a restoration session
#[derive(Debug, Clone)]
pub enum RestoreRecoverShareError {
    /// The restoration session no longer exists
    UnknownRestorationId,
    /// Access structure doesn't match one of the other shares
    AcccessStructureMismatch,
    /// Already know this device has this share
    AlreadyGotThisShare,
    /// The share belongs to a different key or restoration
    ShareBelongsElsewhere { location: Box<ShareLocation> },
}

#[derive(Debug, Clone)]
pub enum RestorePhysicalBackupError {
    UnknownRestorationId,
    AlreadyGotThisShare,
    ShareBelongsElsewhere { location: Box<ShareLocation> },
}

impl fmt::Display for RestorePhysicalBackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestorePhysicalBackupError::UnknownRestorationId => {
                write!(f, "Coordinator didn't have the restoration id")
            }
            RestorePhysicalBackupError::AlreadyGotThisShare => {
                write!(
                    f,
                    "The key on this device has already been added to the restoration"
                )
            }
            RestorePhysicalBackupError::ShareBelongsElsewhere { location } => {
                write!(f, "This key share already belongs to {} '{}' and cannot be added to this restoration", location.key_purpose.key_type_noun(), location.key_name)
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
            RestoreRecoverShareError::AcccessStructureMismatch => {
                write!(f, "Access structure doesn't match one of the other shares")
            }
            RestoreRecoverShareError::AlreadyGotThisShare => {
                write!(
                    f,
                    "The key share on this device has already been added to the restoration"
                )
            }
            RestoreRecoverShareError::ShareBelongsElsewhere { location } => {
                write!(
                    f,
                    "This key share belongs to key '{}' and cannot be added to this restoration",
                    location.key_name
                )
            }
        }
    }
}

impl std::error::Error for RestoreRecoverShareError {}

/// An error when you try to recover a share to a known access structure
#[derive(Debug, Clone)]
pub struct RecoverShareError {
    pub key_purpose: KeyPurpose,
    pub kind: RecoverShareErrorKind,
}

#[derive(Debug, Clone)]
pub enum RecoverShareErrorKind {
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
        let noun = self.key_purpose.key_type_noun();
        match &self.kind {
            RecoverShareErrorKind::AlreadyGotThisShare => {
                write!(f, "This {} already has this key share", noun)
            }
            RecoverShareErrorKind::NoSuchAccessStructure => {
                write!(f, "Could not find this {} to add the key share to", noun)
            }
            RecoverShareErrorKind::ShareImageIsWrong => {
                write!(f, "This key share doesn't belong to this {}", noun)
            }
            RecoverShareErrorKind::DecryptionError => {
                write!(f, "The application provided the wrong decryption key so we couldn't verify the key share for this {}.", noun)
            }
            RecoverShareErrorKind::AccessStructureMismatch => {
                write!(
                    f,
                    "The key share is for a different access structure of this {}",
                    noun
                )
            }
        }
    }
}

impl std::error::Error for RecoverShareError {}

/// Error when starting restoration from a device share
#[derive(Debug, Clone)]
pub enum StartRestorationFromShareError {
    /// The share is missing required metadata (key_name or purpose)
    MissingMetadata,
}

impl fmt::Display for StartRestorationFromShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartRestorationFromShareError::MissingMetadata => {
                write!(f, "This key share doesn't have required metadata. It may have been created from a newer version of the app. Try upgrading the app.")
            }
        }
    }
}

impl std::error::Error for StartRestorationFromShareError {}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RecoveringAccessStructure {
    pub starting_threshold: Option<u16>,
    pub held_shares: Vec<RecoverShare>,
    pub shared_key: Option<SharedKey>,
}

impl RecoveringAccessStructure {
    pub fn access_structure_ref(&self) -> Option<AccessStructureRef> {
        if let Some(ref shared_key) = self.shared_key {
            return Some(AccessStructureRef::from_root_shared_key(shared_key));
        }
        self.held_shares
            .iter()
            .find_map(|recover_share| recover_share.held_share.access_structure_ref)
    }

    pub fn effective_threshold(&self) -> Option<u16> {
        if let Some(ref shared_key) = self.shared_key {
            return Some(shared_key.threshold() as u16);
        }
        self.starting_threshold.or_else(|| {
            self.held_shares
                .iter()
                .find_map(|recover_share| recover_share.held_share.threshold)
        })
    }

    pub fn has_got_share_image(&self, device_id: DeviceId, share_image: ShareImage) -> bool {
        self.held_shares.iter().any(|recover_share| {
            recover_share.held_by == device_id
                && recover_share.held_share.share_image == share_image
        })
    }

    pub fn share_image_to_devices(&self) -> BTreeMap<ShareImage, Vec<DeviceId>> {
        let mut map = BTreeMap::new();
        for recover_share in &self.held_shares {
            map.entry(recover_share.held_share.share_image)
                .or_insert_with(Vec::new)
                .push(recover_share.held_by);
        }
        map
    }

    pub fn compatible_device_to_share_index(
        &self,
        shared_key: &SharedKey,
    ) -> BTreeMap<DeviceId, ShareIndex> {
        self.held_shares
            .iter()
            .filter(|recover_share| {
                let expected_image =
                    shared_key.share_image(recover_share.held_share.share_image.index);
                expected_image == recover_share.held_share.share_image
            })
            .map(|recover_share| {
                (
                    recover_share.held_by,
                    recover_share.held_share.share_image.index,
                )
            })
            .collect()
    }

    pub fn add_share(
        &mut self,
        recover_share: RecoverShare,
        fingerprint: schnorr_fun::frost::Fingerprint,
    ) {
        self.held_shares.push(recover_share);

        // Try fuzzy recovery and cache the shared_key if successful
        if let Some(shared_key) = self.try_fuzzy_recovery(fingerprint) {
            self.shared_key = Some(shared_key);
        }
    }

    /// Try to recover using frost_backup's find_valid_subset
    fn try_fuzzy_recovery(
        &self,
        fingerprint: schnorr_fun::frost::Fingerprint,
    ) -> Option<SharedKey> {
        let share_images: Vec<ShareImage> = self
            .held_shares
            .iter()
            .map(|recover_share| recover_share.held_share.share_image)
            .collect();

        let threshold = self.effective_threshold().map(|t| t as usize);

        // Use frost_backup's find_valid_subset to find compatible share images
        // This will try different combinations and thresholds to find a valid set
        use frost_backup::recovery::find_valid_subset;

        let (_compatible_images, shared_key) =
            find_valid_subset(&share_images, fingerprint, threshold)?;

        shared_key.non_zero()
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
        self.held_shares.iter().any(|recover_share| {
            recover_share.held_by == device_id
                && recover_share.held_share.share_image == share_image
        })
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
