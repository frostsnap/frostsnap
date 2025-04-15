use super::*;
use crate::fail;

#[derive(Clone, Debug, PartialEq)]
pub struct RestorationState {
    pub restoration_id: RestorationId,
    pub key_name: String,
    pub access_structure_ref: Option<AccessStructureRef>,
    pub access_structure: RecoveringAccessStructure,
    pub physical_shares: BTreeSet<DeviceId>,
    pub key_purpose: KeyPurpose,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RecoverShare {
    pub held_by: DeviceId,
    pub held_share: HeldShare,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub(super) restorations: BTreeMap<RestorationId, restoration::RestorationState>,
    pub(super) pending_physical_consolidations:
        BTreeMap<DeviceId, Vec<PendingPhysicalConsolidation>>,
}

impl State {
    pub fn apply_mutation_restoration(
        &mut self,
        mutation: RestorationMutation,
    ) -> Option<RestorationMutation> {
        use RestorationMutation::*;
        match mutation {
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
                        access_structure_ref: Default::default(),
                        access_structure: RecoveringAccessStructure {
                            threshold,
                            share_images: Default::default(),
                        },
                        physical_shares: Default::default(),
                        key_purpose,
                    },
                );
            }
            RestorationProgress {
                restoration_id,
                device_id,
                access_structure_ref,
                share_image,
            } => {
                if let Some(state) = self.restorations.get_mut(&restoration_id) {
                    let already_existing = state
                        .access_structure
                        .share_images
                        .insert(device_id, share_image);

                    if already_existing == Some(share_image) {
                        return None;
                    }

                    if let Some(existing) = state.access_structure_ref {
                        if existing != access_structure_ref {
                            fail!("access_structure_ref didn't match");
                        }
                    }
                    state.access_structure_ref = Some(access_structure_ref);
                } else {
                    fail!("restoration id didn't exist")
                }
            }
            CancelRestoration { restoration_id } => {
                let existed = self.restorations.remove(&restoration_id).is_some();
                if !existed {
                    return None;
                }
            }
            DeviceFinishedPhysicalRestoration {
                restoration_id,
                device_id,
            } => {
                if let Some(pending) = self.pending_physical_consolidations.get_mut(&device_id) {
                    pending.retain(|pending| pending.restoration_id != restoration_id);
                } else {
                    fail!("pending physical restoration did not exist");
                }
            }
            RestorationProgressPhysical {
                restoration_id,
                device_id,
                share_image,
            } => {
                if let Some(state) = self.restorations.get_mut(&restoration_id) {
                    let already_existing = state
                        .access_structure
                        .share_images
                        .insert(device_id, share_image);

                    if already_existing == Some(share_image) {
                        return None;
                    }

                    state.physical_shares.insert(device_id);
                } else {
                    fail!("restoration id didn't exist")
                }
            }
            FinishRestoration {
                restoration_id,
                access_structure_ref,
            } => {
                if let Some(mut restoration) = self.restorations.remove(&restoration_id) {
                    for device_id in restoration.physical_shares {
                        self.pending_physical_consolidations
                            .entry(device_id)
                            .or_default()
                            .push(PendingPhysicalConsolidation {
                                restoration_id,
                                device_id,
                                access_structure_ref,
                                share_index: restoration
                                    .access_structure
                                    .share_images
                                    .remove(&device_id)
                                    .expect("invariant")
                                    .share_index,
                            });
                    }
                }
            }
        }

        Some(mutation.clone())
    }

    pub fn is_restoring(
        &self,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        share_index: PartyIndex,
    ) -> bool {
        self.restorations
            .iter()
            .find(|(_, state)| state.access_structure_ref == Some(access_structure_ref))
            .and_then(|(_, state)| {
                Some(
                    state
                        .access_structure
                        .share_images
                        .get(&device_id)?
                        .share_index
                        == share_index,
                )
            })
            .unwrap_or(false)
    }
}

impl FrostCoordinator {
    pub fn start_restoring_key(
        &mut self,
        key_name: String,
        threshold: u16,
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
        restoration_id: RestorationId,
        device_id: DeviceId,
    ) -> impl IntoIterator<Item = CoordinatorSend> {
        vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(CoordinatorRestoration::Load {
                restoration_id,
            }),
            destinations: [device_id].into(),
        }]
    }

    /// Check a physical backup loaded by a device that you know belongs to a certain access structure.
    pub fn check_physical_backup(
        &self,
        access_structure_ref: AccessStructureRef,
        phase: PhysicalBackupPhase,
        encryption_key: SymmetricKey,
    ) -> Result<PartyIndex, CheckBackupError> {
        let AccessStructureRef {
            key_id,
            access_structure_id,
        } = access_structure_ref;

        let share_index = phase.backup.share_image.share_index;
        let CoordFrostKey { complete_key, .. } = self
            .keys
            .get(&key_id)
            .ok_or(CheckBackupError::NoSuchAccessStructure)?;

        let root_shared_key = complete_key
            .root_shared_key(access_structure_id, encryption_key)
            .ok_or(CheckBackupError::DecryptionError)?;

        let expected_image = ShareImage {
            point: poly::point::eval(root_shared_key.point_polynomial(), share_index).normalize(),
            share_index,
        };

        if phase.backup.share_image != expected_image {
            return Err(CheckBackupError::ShareImageIsWrong);
        }

        Ok(share_index)
    }

    pub fn tell_device_to_save_physical_backup(
        &self,
        phase: PhysicalBackupPhase,
    ) -> impl IntoIterator<Item = CoordinatorSend> {
        let PhysicalBackupPhase {
            backup: EnteredPhysicalBackup { restoration_id, .. },
            from,
        } = phase;
        vec![CoordinatorSend::ToDevice {
            message: CoordinatorToDeviceMessage::Restoration(CoordinatorRestoration::Save {
                restoration_id,
            }),
            destinations: [from].into(),
        }]
    }

    pub fn add_recovery_share_to_restoration(
        &mut self,
        restoration_id: RestorationId,
        recover_share: RecoverShare,
    ) -> Result<(), RestoreRecoverShareError> {
        self.check_recover_share_compatible_with_restoration(restoration_id, &recover_share)?;
        self.mutate(Mutation::Restoration(
            RestorationMutation::RestorationProgress {
                restoration_id,
                device_id: recover_share.held_by,
                access_structure_ref: recover_share.held_share.access_structure_ref,
                share_image: recover_share.held_share.share_image,
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
                let already_existing = restoration
                    .access_structure
                    .share_images
                    .get(&recover_share.held_by);

                if already_existing == Some(&recover_share.held_share.share_image) {
                    return Err(RestoreRecoverShareError::AlreadyGotThisShare);
                }

                if restoration.key_purpose != recover_share.held_share.purpose {
                    return Err(RestoreRecoverShareError::PurposeNotCompatible);
                }

                if let Some(access_structure_ref) = restoration.access_structure_ref {
                    if access_structure_ref != recover_share.held_share.access_structure_ref {
                        return Err(RestoreRecoverShareError::AcccessStructureMismatch);
                    }
                }

                if restoration.key_name != recover_share.held_share.key_name {
                    return Err(RestoreRecoverShareError::NameMismatch);
                }
            }
            None => return Err(RestoreRecoverShareError::UnknownRestorationId),
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
            .ok_or(RestorationError::UnknownRestorationId)?;

        let root_shared_key = state
            .clone()
            .access_structure
            .interpolate()
            .ok_or(RestorationError::NotEnoughShares)?;

        let access_structure_ref = AccessStructureRef::from_root_shared_key(&root_shared_key);

        let expected_threshold = state.access_structure.threshold;
        let got_threshold = root_shared_key.threshold();

        if expected_threshold as usize != got_threshold {
            return Err(RestorationError::ThresholdDoesntMatch {
                expected: expected_threshold,
                got: got_threshold as u16,
            });
        }

        if let Some(expected_access_structure_ref) = state.access_structure_ref {
            if access_structure_ref != expected_access_structure_ref {
                return Err(RestorationError::InterpolationDoesntMatch);
            }
        }

        let device_to_share_index = state
            .access_structure
            .share_images
            .iter()
            .map(|(&device_id, &share_image)| (device_id, share_image.share_index))
            .collect();

        self.mutate_new_key(
            state.key_name.clone(),
            root_shared_key,
            device_to_share_index,
            encryption_key,
            state.key_purpose,
            rng,
        );

        self.mutate(Mutation::Restoration(
            RestorationMutation::FinishRestoration {
                restoration_id,
                access_structure_ref,
            },
        ));

        Ok(access_structure_ref)
    }

    pub fn get_restoration_state(&self, restoration_id: RestorationId) -> Option<RestorationState> {
        self.restoration.restorations.get(&restoration_id).cloned()
    }

    /// Recovers a share to an existing access structure
    pub fn recover_share(
        &mut self,
        recover_share: RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<(), RecoverShareError> {
        self.check_recover_share_compatible_with_key(recover_share.clone(), encryption_key)?;

        self.mutate(Mutation::NewShare {
            access_structure_ref: recover_share.held_share.access_structure_ref,
            device_id: recover_share.held_by,
            share_index: recover_share.held_share.share_image.share_index,
        });

        Ok(())
    }

    pub fn check_recover_share_compatible_with_key(
        &self,
        recover_share: RecoverShare,
        encryption_key: SymmetricKey,
    ) -> Result<(), RecoverShareError> {
        let access_structure_ref = recover_share.held_share.access_structure_ref;
        let frost_key = self
            .get_frost_key(access_structure_ref.key_id)
            .ok_or(RecoverShareError::NoSuchAccessStructure)?;
        let access_structure = self
            .get_access_structure(recover_share.held_share.access_structure_ref)
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

        let expected_image = root_shared_key.share_image(share_image.share_index);

        if expected_image != share_image.point {
            return Err(RecoverShareError::ShareImageIsWrong);
        }
        Ok(())
    }

    pub fn cancel_restoration(&mut self, restoration_id: RestorationId) {
        self.mutate(Mutation::Restoration(
            RestorationMutation::CancelRestoration { restoration_id },
        ))
    }

    pub fn start_restoring_key_from_recover_share(
        &mut self,
        recover_share: RecoverShare,
        restoration_id: RestorationId,
    ) {
        let held_share = recover_share.held_share;
        assert!(!self.restoration.restorations.contains_key(&restoration_id));
        assert!(self
            .get_access_structure(held_share.access_structure_ref)
            .is_none());
        self.mutate(Mutation::Restoration(RestorationMutation::NewRestoration {
            restoration_id,
            key_name: held_share.key_name,
            threshold: held_share.threshold,
            key_purpose: held_share.purpose,
        }));

        self.mutate(Mutation::Restoration(
            RestorationMutation::RestorationProgress {
                restoration_id,
                device_id: recover_share.held_by,
                access_structure_ref: held_share.access_structure_ref,
                share_image: held_share.share_image,
            },
        ));
    }

    pub fn recv_restoration_message(
        &mut self,
        from: DeviceId,
        message: DeviceRestoration,
    ) -> MessageResult<Vec<CoordinatorSend>> {
        match message {
            DeviceRestoration::PhysicalLoaded(entered_physical_backup) => {
                //XXX: We could check if a restoration id exists before sending out the message but
                // it's not a good idea becuase atm it's valid to ask a device to enter a backup
                // when you're not keeping track of the restoration id for the purpose of doing a
                // backup check.
                return Ok(vec![CoordinatorSend::ToUser(
                    CoordinatorToUserMessage::Restoration(
                        ToUserRestoration::PhysicalBackupEntered(Box::new(PhysicalBackupPhase {
                            backup: entered_physical_backup,
                            from,
                        })),
                    ),
                )]);
            }
            DeviceRestoration::PhysicalSaved(entered_physical_backup) => {
                if self
                    .restoration
                    .restorations
                    .contains_key(&entered_physical_backup.restoration_id)
                {
                    let restoration_id = entered_physical_backup.restoration_id;
                    // XXX: If we could this is where we would validate the share.
                    self.mutate(Mutation::Restoration(
                        RestorationMutation::RestorationProgressPhysical {
                            restoration_id,
                            device_id: from,
                            share_image: entered_physical_backup.share_image,
                        },
                    ));
                    return Ok(vec![CoordinatorSend::ToUser(
                        CoordinatorToUserMessage::Restoration(
                            ToUserRestoration::PhysicalBackupSaved {
                                device_id: from,
                                restoration_id,
                                share_index: entered_physical_backup.share_image.share_index,
                            },
                        ),
                    )]);
                }
            }
            DeviceRestoration::ExitedRecoveryMode { restoration_id } => {
                self.mutate(Mutation::Restoration(
                    RestorationMutation::DeviceFinishedPhysicalRestoration {
                        restoration_id,
                        device_id: from,
                    },
                ));
            }
            DeviceRestoration::HeldShares(held_shares) => {
                let mut already_got = vec![];
                let mut recoverable = vec![];
                for held_share in held_shares {
                    let access_structure_ref = held_share.access_structure_ref;

                    if self.knows_about_share(
                        from,
                        access_structure_ref,
                        held_share.share_image.share_index,
                    ) {
                        already_got.push(held_share);
                    } else {
                        recoverable.push(held_share);
                    }
                }
                return Ok(vec![CoordinatorSend::ToUser(
                    ToUserRestoration::GotHeldShares {
                        held_by: from,
                        already_got,
                        recoverable,
                    }
                    .into(),
                )]);
            }
        }

        Ok(vec![])
    }

    pub fn has_backups_that_need_to_be_consolidated(&self, device_id: DeviceId) -> bool {
        self.restoration
            .pending_physical_consolidations
            .get(&device_id)
            .map(|consolidations| !consolidations.is_empty())
            .unwrap_or(false)
    }

    pub fn consolidate_physical_backups(
        &self,
        device_id: DeviceId,
        encryption_key: SymmetricKey,
    ) -> impl IntoIterator<Item = CoordinatorSend> {
        let consolidations = self
            .restoration
            .pending_physical_consolidations
            .get(&device_id)
            .cloned()
            .unwrap_or_default();

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
                        restoration_id: consolidation.restoration_id,
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
                },
            ),
            destinations: BTreeSet::from_iter([device_id]),
        }])
    }

    pub fn restoring(&self) -> impl Iterator<Item = RestorationState> + '_ {
        self.restoration.restorations.values().cloned()
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub enum RestorationMutation {
    NewRestoration {
        restoration_id: RestorationId,
        key_name: String,
        threshold: u16,
        key_purpose: KeyPurpose,
    },
    RestorationProgressPhysical {
        restoration_id: RestorationId,
        device_id: DeviceId,
        share_image: ShareImage,
    },
    RestorationProgress {
        restoration_id: RestorationId,
        device_id: DeviceId,
        access_structure_ref: AccessStructureRef,
        share_image: ShareImage,
    },
    FinishRestoration {
        restoration_id: RestorationId,
        /// the restoration has become this access structure
        access_structure_ref: AccessStructureRef,
    },
    DeviceFinishedPhysicalRestoration {
        restoration_id: RestorationId,
        device_id: DeviceId,
    },
    CancelRestoration {
        restoration_id: RestorationId,
    },
}

#[derive(Clone, Debug)]
pub enum ToUserRestoration {
    GotHeldShares {
        held_by: DeviceId,
        already_got: Vec<HeldShare>,
        recoverable: Vec<HeldShare>,
    },
    PhysicalBackupEntered(Box<PhysicalBackupPhase>),
    PhysicalBackupSaved {
        device_id: DeviceId,
        restoration_id: RestorationId,
        share_index: PartyIndex,
    },
}

impl From<ToUserRestoration> for CoordinatorToUserMessage {
    fn from(value: ToUserRestoration) -> Self {
        CoordinatorToUserMessage::Restoration(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingPhysicalConsolidation {
    pub restoration_id: RestorationId,
    pub device_id: DeviceId,
    pub access_structure_ref: AccessStructureRef,
    pub share_index: PartyIndex,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalBackupPhase {
    pub backup: EnteredPhysicalBackup,
    pub from: DeviceId,
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
                "The threshold was entered wrongly or one of the shares is wrong. Expected a threshold of {}, got {}",
                expected, got
            ),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RestorationError {}

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
    /// Already got this share
    AlreadyGotThisShare,
}

impl fmt::Display for RestoreRecoverShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestoreRecoverShareError::UnknownRestorationId => {
                write!(f, "cooridnator didn't have the restoration id")
            }
            RestoreRecoverShareError::PurposeNotCompatible => {
                write!(f, "The key share is use by the device for a different purpose than the restoration session")
            }
            RestoreRecoverShareError::AcccessStructureMismatch => {
                write!(f, "Access structure doesn't match one of the other shares")
            }
            RestoreRecoverShareError::AlreadyGotThisShare => {
                write!(f, "Already got this share")
            }
            RestoreRecoverShareError::NameMismatch => {
                write!(
                    f,
                    "The name of the key being restored and the one in the share is not the same"
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RestoreRecoverShareError {}

#[derive(Debug, Clone)]
pub enum RecoverShareError {
    /// The coordinator already knows about this share
    AlreadyGotThisShare,
    /// The access structure for the share isn't known to the coordinator
    NoSuchAccessStructure,
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
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for RecoverShareError {}
