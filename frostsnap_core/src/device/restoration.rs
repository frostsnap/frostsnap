use super::card::{self as device_card, CardOutput};
use super::card_restore::CardRestorePhase;
use crate::message::card::{CardRefusal, DeviceCard};
use crate::message::HeldShare2;
use crate::EnterPhysicalId;
use frost_backup::ShareBackup;
use schnorr_fun::frost::SharedKey;

use super::*;
use alloc::fmt::Debug;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct State {
    tmp_loaded_backups: BTreeMap<ShareImage, PhysicalBackup>,
    saved_backups: BTreeMap<ShareImage, SavedBackup3>,
}

/// A backup the user entered: today's 25 words, or a 64-word verifiable card. Its `Debug`
/// shows only the share image.
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub enum PhysicalBackup {
    Words25(ShareBackup),
    /// Variant 1, as the 32-word card's was: `Save3` is unchanged on flash.
    Card {
        share: WipedShare,
        /// The threshold the card claims, unproven: None when the card records 0 (not
        /// recorded), and [`UNREADABLE_THRESHOLD`](super::card_restore::UNREADABLE_THRESHOLD)
        /// when its threshold field doesn't read.
        threshold: Option<u16>,
    },
}

/// A share decoded from a card, wiped when it's dropped wherever a copy goes: the loaded
/// backups, a saved backup and its staged mutation (design §9 R13). It's encoded as the share
/// itself, so `Save3` is unchanged on flash.
#[derive(Clone, PartialEq, bincode::Encode, bincode::Decode)]
pub struct WipedShare(pub SecretShare);

impl core::fmt::Debug for WipedShare {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WipedShare")
            .field("index", &self.0.index)
            .finish_non_exhaustive()
    }
}

impl Drop for WipedShare {
    fn drop(&mut self) {
        device_card::wipe(&mut self.0.share, Scalar::zero());
    }
}

impl core::fmt::Debug for PhysicalBackup {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let kind = match self {
            PhysicalBackup::Words25(_) => "Words25",
            PhysicalBackup::Card { .. } => "Card",
        };
        f.debug_struct(kind)
            .field("share_image", &self.share_image())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtractError {
    /// The 25-word backup's polynomial checksum doesn't match.
    PolyChecksum,
    /// The card's share isn't the polynomial's share at its index.
    ImageMismatch,
}

impl PhysicalBackup {
    pub fn share_image(&self) -> ShareImage {
        match self {
            PhysicalBackup::Words25(share_backup) => share_backup.share_image(),
            PhysicalBackup::Card { share, .. } => share.0.share_image(),
        }
    }

    /// The share, once it's checked against the root polynomial. A card is checked by its image
    /// alone: its threshold isn't bound by its proofs, so it only warns (spec PK.3,
    /// [`threshold_warning`](Self::threshold_warning)).
    pub fn extract_secret(&self, root_shared_key: &SharedKey) -> Result<SecretShare, ExtractError> {
        match self {
            PhysicalBackup::Words25(share_backup) => share_backup
                .clone()
                .extract_secret(root_shared_key)
                .map_err(|_| ExtractError::PolyChecksum),
            PhysicalBackup::Card {
                share: WipedShare(share),
                ..
            } => {
                if root_shared_key.share_image(share.index) == share.share_image() {
                    Ok(*share)
                } else {
                    Err(ExtractError::ImageMismatch)
                }
            }
        }
    }

    /// A card whose threshold isn't the wallet's (`threshold`), or doesn't read: consolidation
    /// holds on a warning the user confirms, and never refuses (build plan A11).
    pub fn threshold_warning(&self, threshold: u16) -> Option<ThresholdWarning> {
        let PhysicalBackup::Card {
            threshold: Some(card),
            ..
        } = self
        else {
            return None;
        };
        let card = u8::try_from(*card).ok();
        (card != Some(0) && card.map(u16::from) != Some(threshold)).then_some(ThresholdWarning {
            card,
            wallet: threshold,
        })
    }
}

/// A card's threshold that isn't its wallet's, at consolidation (build plan A11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThresholdWarning {
    /// The threshold the card claims, or None if it doesn't read.
    pub card: Option<u8>,
    /// The wallet's threshold.
    pub wallet: u16,
}

impl State {
    pub fn apply_mutation_restoration(
        &mut self,
        mutation: RestorationMutation,
    ) -> Option<RestorationMutation> {
        use RestorationMutation::*;
        match &mutation {
            Save(legacy_saved_backup) => {
                // Convert legacy to new and recurse
                let saved_backup: SavedBackup2 = legacy_saved_backup.clone().into();
                return self.apply_mutation_restoration(Save2(saved_backup));
            }
            _UnSave(_) => {
                // No-op: cleanup happens automatically when SaveShare mutation is applied
            }
            Save2(saved_backup) => {
                let backup_share_image = saved_backup.share_backup.share_image();
                self.saved_backups
                    .insert(backup_share_image, saved_backup.clone().into());
            }
            Save3(saved_backup) => {
                self.saved_backups
                    .insert(saved_backup.backup.share_image(), saved_backup.clone());
            }
        }
        Some(mutation)
    }

    pub fn clear_tmp_data(&mut self) {
        self.tmp_loaded_backups.clear();
    }

    pub fn remove_backups_with_share_image(&mut self, share_image: ShareImage) {
        self.tmp_loaded_backups.remove(&share_image);
        self.saved_backups.remove(&share_image);
    }
}

impl<S: Debug + NonceStreamSlot> FrostSigner<S> {
    pub fn recv_restoration_message(
        &mut self,
        message: CoordinatorRestoration,
        _rng: &mut impl rand_core::RngCore,
    ) -> MessageResult<Vec<DeviceSend>> {
        use ToUserRestoration::*;
        match &message {
            &CoordinatorRestoration::EnterPhysicalBackup { enter_physical_id } => {
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::Restoration(Box::new(EnterBackup {
                        phase: EnterBackupPhase { enter_physical_id },
                    })),
                ))])
            }
            &CoordinatorRestoration::SavePhysicalBackup {
                share_image,
                threshold,
                purpose,
                ref key_name,
            } => {
                // Convert to new SavePhysicalBackup2 and recurse
                self.recv_restoration_message(
                    CoordinatorRestoration::SavePhysicalBackup2(Box::new(HeldShare2 {
                        access_structure_ref: None,
                        share_image,
                        threshold: Some(threshold),
                        purpose: Some(purpose),
                        key_name: Some(key_name.clone()),
                        needs_consolidation: true,
                    })),
                    _rng,
                )
            }
            CoordinatorRestoration::SavePhysicalBackup2(held_share) => {
                let share_image = held_share.share_image;
                let threshold = held_share.threshold;
                let purpose = held_share.purpose;
                let key_name = held_share.key_name.clone();
                if let Some(backup) = self.restoration.tmp_loaded_backups.remove(&share_image) {
                    // A 25-word backup is saved as before, byte for byte.
                    self.mutate(Mutation::Restoration(match backup {
                        PhysicalBackup::Words25(share_backup) => {
                            RestorationMutation::Save2(self::SavedBackup2 {
                                share_backup,
                                threshold,
                                purpose,
                                key_name: key_name.clone(),
                            })
                        }
                        backup @ PhysicalBackup::Card { .. } => {
                            RestorationMutation::Save3(self::SavedBackup3 {
                                backup,
                                threshold,
                                purpose,
                                key_name: key_name.clone(),
                            })
                        }
                    }));

                    Ok(vec![
                        DeviceSend::ToUser(Box::new(DeviceToUserMessage::Restoration(Box::new(
                            ToUserRestoration::BackupSaved {
                                share_image,
                                key_name,
                                purpose,
                                threshold,
                            },
                        )))),
                        DeviceSend::ToCoordinator(Box::new(
                            DeviceToCoordinatorMessage::Restoration(
                                DeviceRestoration::PhysicalSaved(share_image),
                            ),
                        )),
                    ])
                } else {
                    Err(Error::signer_invalid_message(
                        &message,
                        "couldn't find secret share saved for that restoration",
                    ))
                }
            }
            CoordinatorRestoration::Consolidate(consolidate) => {
                let root_shared_key = &consolidate.root_shared_key;
                let access_structure_ref =
                    AccessStructureRef::from_root_shared_key(root_shared_key);
                let share_image = root_shared_key.share_image(consolidate.share_index);

                // Try to get the share from saved backups or tmp loaded backups, and check it
                // against the polynomial: its checksum, or for a card its image
                let maybe_backup = self
                    .restoration
                    .saved_backups
                    .get(&share_image)
                    .map(|saved_backup| &saved_backup.backup)
                    .or_else(|| self.restoration.tmp_loaded_backups.get(&share_image))
                    .cloned();

                if let Some(physical_backup) = maybe_backup {
                    let card = matches!(physical_backup, PhysicalBackup::Card { .. });
                    let threshold_warning =
                        physical_backup.threshold_warning(root_shared_key.threshold() as u16);
                    let secret_share = match physical_backup.extract_secret(root_shared_key) {
                        Ok(secret_share) => secret_share,
                        Err(_) => {
                            // TODO: This needs to be a catastrophic error that
                            // halts the whole device.
                            return Err(Error::signer_invalid_message(
                                &message,
                                "polynomial checksum validation failed",
                            ));
                        }
                    };

                    let expected_image = root_shared_key.share_image(secret_share.index);
                    let actual_image = secret_share.share_image();

                    if expected_image != actual_image
                        || secret_share.index != consolidate.share_index
                    {
                        Err(Error::signer_invalid_message(
                            &message,
                            "the image didn't match for consolidation",
                        ))
                    } else {
                        let complete_share = CompleteSecretShare {
                            access_structure_ref,
                            key_name: consolidate.key_name.clone(),
                            purpose: consolidate.purpose,
                            threshold: root_shared_key.threshold() as u16,
                            secret_share,
                        };

                        let coord_contrib = CoordShareDecryptionContrib::for_master_share(
                            self.device_id(),
                            consolidate.share_index,
                            root_shared_key,
                        );

                        Ok(vec![DeviceSend::ToUser(Box::new(
                            DeviceToUserMessage::Restoration(Box::new(
                                ToUserRestoration::ConsolidateBackup(ConsolidatePhase {
                                    complete_share,
                                    coord_contrib,
                                    card,
                                    threshold_warning,
                                }),
                            )),
                        ))])
                    }
                } else if self
                    .get_encrypted_share(access_structure_ref, consolidate.share_index)
                    .is_none()
                {
                    Err(Error::signer_invalid_message(
                        &message,
                        "we can't consolidate a share we don't know about",
                    ))
                } else {
                    // we've already consolidated it so just answer afirmatively
                    Ok(vec![DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::Restoration(
                            DeviceRestoration::FinishedConsolidation {
                                access_structure_ref,
                                share_index: consolidate.share_index,
                            },
                        ),
                    ))])
                }
            }
            &CoordinatorRestoration::DisplayBackup {
                // Redundant with root_shared_key — ignored, derived below.
                access_structure_ref: _,
                coord_share_decryption_contrib,
                share_index,
                ref root_shared_key,
            } => {
                let access_structure_ref =
                    AccessStructureRef::from_root_shared_key(root_shared_key);
                let AccessStructureRef {
                    key_id,
                    access_structure_id,
                } = access_structure_ref;
                let key_data = self.keys.get(&key_id).ok_or(Error::signer_invalid_message(
                    &message,
                    format!(
                        "signer doesn't have a share for this key: {}",
                        self.keys
                            .keys()
                            .map(|key| key.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                ))?;

                let access_structure_data = key_data
                    .access_structures
                    .get(&access_structure_id)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "no such access structure on this device",
                        )
                    })?;

                let encrypted_share =
                    access_structure_data
                        .shares
                        .get(&share_index)
                        .ok_or_else(|| {
                            Error::signer_invalid_message(
                                &message,
                                "access structure exists but this device doesn't have that share",
                            )
                        })?;
                // A card share is never shown again as 25 words (spec L1.6.2).
                if let Some(refusal) = self.legacy_display_refusal(encrypted_share.share_image) {
                    return Ok(refusal);
                }
                let encrypted_secret_share = encrypted_share.ciphertext;
                let phase = BackupDisplayPhase {
                    access_structure_ref,
                    share_index,
                    encrypted_secret_share,
                    coord_share_decryption_contrib,
                    key_name: key_data.key_name.clone(),
                    root_shared_key: root_shared_key.clone(),
                };
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::Restoration(Box::new(DisplayBackup {
                        key_name: phase.key_name.clone(),
                        access_structure_ref: phase.access_structure_ref,
                        phase,
                    })),
                ))])
            }

            &CoordinatorRestoration::CheckBackup {
                coord_share_decryption_contrib,
                share_index,
                ref root_shared_key,
            } => {
                let access_structure_ref =
                    AccessStructureRef::from_root_shared_key(root_shared_key);
                let AccessStructureRef {
                    key_id,
                    access_structure_id,
                } = access_structure_ref;
                let key_data = self.keys.get(&key_id).ok_or(Error::signer_invalid_message(
                    &message,
                    format!(
                        "signer doesn't have a share for this key: {}",
                        self.keys
                            .keys()
                            .map(|key| key.to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                ))?;

                let access_structure_data = key_data
                    .access_structures
                    .get(&access_structure_id)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "no such access structure on this device",
                        )
                    })?;

                let encrypted_share =
                    access_structure_data
                        .shares
                        .get(&share_index)
                        .ok_or_else(|| {
                            Error::signer_invalid_message(
                                &message,
                                "access structure exists but this device doesn't have that share",
                            )
                        })?;
                // A card share is never shown again as 25 words (spec L1.6.2).
                if let Some(refusal) = self.legacy_display_refusal(encrypted_share.share_image) {
                    return Ok(refusal);
                }
                let encrypted_secret_share = encrypted_share.ciphertext;
                let phase = BackupDisplayPhase {
                    access_structure_ref,
                    share_index,
                    encrypted_secret_share,
                    coord_share_decryption_contrib,
                    key_name: key_data.key_name.clone(),
                    root_shared_key: root_shared_key.clone(),
                };
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::Restoration(Box::new(CheckBackup {
                        key_name: phase.key_name.clone(),
                        access_structure_ref: phase.access_structure_ref,
                        phase,
                    })),
                ))])
            }

            CoordinatorRestoration::EnterCard {
                enter_physical_id,
                target,
                recorded_format,
                share_indices,
            } => Ok(self.card_restore_enter(CardRestorePhase {
                enter_physical_id: *enter_physical_id,
                target: target.clone(),
                recorded_format: *recorded_format,
                share_indices: share_indices.clone(),
            })),
            CoordinatorRestoration::CardPhoneHalf {
                enter_physical_id,
                words,
            } => Ok(self.card_restore_phone_half(*enter_physical_id, words.clone())),
            CoordinatorRestoration::RequestHeldShares => {
                let held_shares = self.held_shares().collect();
                let send = Some(DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::Restoration(DeviceRestoration::HeldShares2(
                        held_shares,
                    )),
                )));
                Ok(send.into_iter().collect())
            }
        }
    }

    /// The refusal of today's 25-word display for a card share, if this share is one.
    fn legacy_display_refusal(&self, share_image: ShareImage) -> Option<Vec<DeviceSend>> {
        let mut ops = frost_backup::verifiable_card::proof::Ops::default();
        device_card::card_gate(
            self.card.records.get(&share_image),
            None,
            &CardOutput::LegacyDisplay,
            &mut ops,
        )
        .err()
        .map(|_| {
            vec![
                DeviceSend::ToCoordinator(Box::new(DeviceToCoordinatorMessage::Card(
                    DeviceCard::Refused {
                        reason: CardRefusal::WrongState,
                    },
                ))),
                device_card::to_user(device_card::ToUserCard::Screen(
                    device_card::CardScreen::RedisplayRefused,
                )),
            ]
        })
    }

    pub fn tell_coordinator_about_backup_load_result(
        &mut self,
        phase: EnterBackupPhase,
        share_backup: ShareBackup,
    ) -> impl IntoIterator<Item = DeviceSend> {
        self.load_physical_backup(phase, PhysicalBackup::Words25(share_backup))
    }

    pub(super) fn load_physical_backup(
        &mut self,
        phase: EnterBackupPhase,
        backup: PhysicalBackup,
    ) -> Vec<DeviceSend> {
        let mut ret = vec![];
        let enter_physical_id = phase.enter_physical_id;

        let share_image = backup.share_image();
        self.restoration
            .tmp_loaded_backups
            .insert(share_image, backup);

        ret.push(DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::Restoration(DeviceRestoration::PhysicalEntered(
                EnteredPhysicalBackup {
                    enter_physical_id,
                    share_image,
                },
            )),
        )));

        ret
    }

    pub fn held_shares(&self) -> impl Iterator<Item = HeldShare2> + '_ {
        // Iterator over shares from keys with master access structures
        let keys_iter = self.keys.iter().flat_map(move |(key_id, key_data)| {
            key_data.access_structures.iter().flat_map(
                move |(access_structure_id, access_structure)| {
                    access_structure.shares.values().filter_map(move |share| {
                        if access_structure.kind == AccessStructureKind::Master {
                            Some(HeldShare2 {
                                key_name: Some(key_data.key_name.clone()),
                                share_image: share.share_image,
                                access_structure_ref: Some(AccessStructureRef {
                                    access_structure_id: *access_structure_id,
                                    key_id: *key_id,
                                }),
                                threshold: Some(access_structure.threshold),
                                purpose: Some(key_data.purpose),
                                needs_consolidation: false,
                            })
                        } else {
                            None
                        }
                    })
                },
            )
        });

        // Iterator over shares from saved backups
        let backups_iter =
            self.restoration
                .saved_backups
                .iter()
                .map(|(&share_image, saved_backup)| HeldShare2 {
                    key_name: saved_backup.key_name.clone(),
                    access_structure_ref: None,
                    share_image,
                    threshold: saved_backup.threshold,
                    purpose: saved_backup.purpose,
                    needs_consolidation: true,
                });

        // Chain both iterators together
        keys_iter.chain(backups_iter)
    }

    pub fn saved_backups(&self) -> &BTreeMap<ShareImage, SavedBackup3> {
        &self.restoration.saved_backups
    }

    pub fn finish_consolidation(
        &mut self,
        symm_keygen: &mut impl DeviceSecretDerivation,
        phase: ConsolidatePhase,
        rng: &mut impl rand_core::RngCore,
    ) -> impl IntoIterator<Item = DeviceSend> {
        let share_image = phase.complete_share.secret_share.share_image();
        let access_structure_ref = phase.complete_share.access_structure_ref;
        // Insert-only: a share with a record (from keygen) keeps it.
        self.mutate(Mutation::Card(device_card::CardMutation::Origin {
            share_image,
            origin: device_card::ShareOrigin::Restored { card: phase.card },
        }));
        // No need to explicitly UnSave - cleanup happens automatically in SaveShare mutation

        let encrypted_secret_share = EncryptedSecretShare::encrypt(
            phase.complete_share.secret_share,
            phase.complete_share.access_structure_ref,
            phase.coord_contrib,
            symm_keygen,
            rng,
        );
        self.save_complete_share(KeyGenPhase4 {
            key_name: phase.complete_share.key_name,
            key_purpose: phase.complete_share.purpose,
            access_structure_ref: phase.complete_share.access_structure_ref,
            access_structure_kind: AccessStructureKind::Master,
            threshold: phase.complete_share.threshold,
            encrypted_secret_share,
            card: false,
        });

        vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::Restoration(DeviceRestoration::FinishedConsolidation {
                share_index: share_image.index,
                access_structure_ref,
            }),
        ))]
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq, frostsnap_macros::Kind)]
pub enum RestorationMutation {
    Save(SavedBackup),
    /// DO NOT USE: This is a no-op. Cleanup of restoration backups happens automatically
    /// when SaveShare mutation is applied. If you need to delete backups, create a new
    /// mutation for that specific purpose.
    _UnSave(ShareImage),
    Save2(SavedBackup2),
    /// A backup of any kind. 25-word backups are still saved as `Save2`.
    Save3(SavedBackup3),
}

#[derive(Debug, Clone)]
pub enum ToUserRestoration {
    EnterBackup {
        phase: EnterBackupPhase,
    },
    BackupSaved {
        share_image: ShareImage,
        key_name: Option<String>,
        purpose: Option<KeyPurpose>,
        threshold: Option<u16>,
    },
    ConsolidateBackup(ConsolidatePhase),
    DisplayBackup {
        key_name: String,
        access_structure_ref: AccessStructureRef,
        phase: BackupDisplayPhase,
    },
    CheckBackup {
        key_name: String,
        access_structure_ref: AccessStructureRef,
        phase: BackupDisplayPhase,
    },
}

#[derive(Debug, Clone)]
pub struct ConsolidatePhase {
    pub complete_share: CompleteSecretShare,
    pub coord_contrib: CoordShareDecryptionContrib,
    /// The share came from a verifiable card.
    pub card: bool,
    /// The card's threshold isn't the wallet's, or doesn't read: the firmware shows a warning
    /// the user holds to confirm before it finishes the consolidation (build plan A11).
    pub threshold_warning: Option<ThresholdWarning>,
}

#[derive(Clone, Debug)]
pub struct BackupDisplayPhase {
    pub access_structure_ref: AccessStructureRef,
    pub share_index: ShareIndex,
    pub encrypted_secret_share: Ciphertext<32, Scalar<Secret, Zero>>,
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
    pub key_name: String,
    pub root_shared_key: SharedKey,
}

impl BackupDisplayPhase {
    pub fn decrypt_to_backup(
        &self,
        symm_keygen: &mut impl DeviceSecretDerivation,
    ) -> Result<ShareBackup, ActionError> {
        let encryption_key = symm_keygen.get_share_encryption_key(
            self.access_structure_ref,
            self.share_index,
            self.coord_share_decryption_contrib,
        );
        let secret_share = self.encrypted_secret_share.decrypt(encryption_key).ok_or(
            ActionError::StateInconsistent("could not decrypt secret share".into()),
        )?;
        let secret = SecretShare {
            index: self.share_index,
            share: secret_share,
        };
        Ok(ShareBackup::from_secret_share_and_shared_key(
            secret,
            &self.root_shared_key,
        ))
    }
}

#[derive(Clone, Debug)]
pub struct EnterBackupPhase {
    pub enter_physical_id: EnterPhysicalId,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SavedBackup {
    pub share_backup: ShareBackup,
    pub threshold: u16,
    pub purpose: KeyPurpose,
    pub key_name: String,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SavedBackup2 {
    pub share_backup: ShareBackup,
    pub threshold: Option<u16>,
    pub purpose: Option<KeyPurpose>,
    pub key_name: Option<String>,
}

impl From<SavedBackup> for SavedBackup2 {
    fn from(legacy: SavedBackup) -> Self {
        SavedBackup2 {
            share_backup: legacy.share_backup,
            threshold: Some(legacy.threshold),
            purpose: Some(legacy.purpose),
            key_name: Some(legacy.key_name),
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SavedBackup3 {
    pub backup: PhysicalBackup,
    pub threshold: Option<u16>,
    pub purpose: Option<KeyPurpose>,
    pub key_name: Option<String>,
}

impl From<SavedBackup2> for SavedBackup3 {
    fn from(saved_backup: SavedBackup2) -> Self {
        SavedBackup3 {
            backup: PhysicalBackup::Words25(saved_backup.share_backup),
            threshold: saved_backup.threshold,
            purpose: saved_backup.purpose,
            key_name: saved_backup.key_name,
        }
    }
}
