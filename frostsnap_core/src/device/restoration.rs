use crate::message::HeldShare2;
use crate::EnterPhysicalId;
use frost_backup::ShareBackup;
use schnorr_fun::frost::SharedKey;

use super::*;
use alloc::fmt::Debug;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct State {
    tmp_loaded_backups: BTreeMap<ShareImage, ShareBackup>,
    saved_backups: BTreeMap<ShareImage, SavedBackup2>,
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
                    .insert(backup_share_image, saved_backup.clone());
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
                    DeviceToUserMessage::Restoration(EnterBackup {
                        phase: EnterBackupPhase { enter_physical_id },
                    }),
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
                if let Some(share_backup) = self.restoration.tmp_loaded_backups.remove(&share_image)
                {
                    self.mutate(Mutation::Restoration(RestorationMutation::Save2(
                        self::SavedBackup2 {
                            share_backup: share_backup.clone(),
                            threshold,
                            purpose,
                            key_name: key_name.clone(),
                        },
                    )));

                    Ok(vec![
                        DeviceSend::ToUser(Box::new(DeviceToUserMessage::Restoration(
                            ToUserRestoration::BackupSaved {
                                share_image,
                                key_name,
                                purpose,
                                threshold,
                            },
                        ))),
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

                // Try to get the share from saved backups or tmp loaded backups
                // Both contain ShareBackup now, so we validate the polynomial checksum
                let maybe_backup = self
                    .restoration
                    .saved_backups
                    .get(&share_image)
                    .map(|saved_backup| &saved_backup.share_backup)
                    .or_else(|| self.restoration.tmp_loaded_backups.get(&share_image))
                    .cloned();

                if let Some(secret_share_backup) = maybe_backup {
                    let secret_share = secret_share_backup
                        .extract_secret(root_shared_key)
                        .map_err(|_| {
                            // TODO: This needs to be a catastrophic error that
                            // halts the whole device.
                            Error::signer_invalid_message(
                                &message,
                                "polynomial checksum validation failed",
                            )
                        })?;

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
                            DeviceToUserMessage::Restoration(ToUserRestoration::ConsolidateBackup(
                                ConsolidatePhase {
                                    complete_share,
                                    coord_contrib,
                                },
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
                access_structure_ref,
                coord_share_decryption_contrib,
                party_index,
                ref root_shared_key,
            } => {
                // Verify that the root_shared_key corresponds to the access_structure_ref
                let expected_ref = AccessStructureRef::from_root_shared_key(root_shared_key);
                if expected_ref != access_structure_ref {
                    return Err(Error::signer_invalid_message(
                        &message,
                        "root_shared_key doesn't match access_structure_ref",
                    ));
                }

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

                let encrypted_secret_share = access_structure_data
                    .shares
                    .get(&party_index)
                    .ok_or_else(|| {
                        Error::signer_invalid_message(
                            &message,
                            "access structure exists but this device doesn't have that share",
                        )
                    })?
                    .ciphertext;
                let phase = BackupDisplayPhase {
                    access_structure_ref,
                    party_index,
                    encrypted_secret_share,
                    coord_share_decryption_contrib,
                    key_name: key_data.key_name.clone(),
                    root_shared_key: root_shared_key.clone(),
                };
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::Restoration(DisplayBackupRequest {
                        phase: Box::new(phase),
                    }),
                ))])
            }

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

    pub fn display_backup_ack(
        &mut self,
        phase: BackupDisplayPhase,
        symm_keygen: &mut impl DeviceSecretDerivation,
    ) -> Result<Vec<DeviceSend>, ActionError> {
        let key_data = self
            .keys
            .get(&phase.access_structure_ref.key_id)
            .expect("key must exist");
        let encryption_key = symm_keygen.get_share_encryption_key(
            phase.access_structure_ref,
            phase.party_index,
            phase.coord_share_decryption_contrib,
        );
        let secret_share = phase.encrypted_secret_share.decrypt(encryption_key).ok_or(
            ActionError::StateInconsistent("could not decrypt secret share".into()),
        )?;
        let secret = SecretShare {
            index: phase.party_index,
            share: secret_share,
        };
        // Create BIP39 backup with polynomial checksum
        let share_backup =
            ShareBackup::from_secret_share_and_shared_key(secret, &phase.root_shared_key);
        Ok(vec![DeviceSend::ToUser(Box::new(
            DeviceToUserMessage::Restoration(ToUserRestoration::DisplayBackup {
                key_name: key_data.key_name.clone(),
                backup: share_backup,
            }),
        ))])
    }

    pub fn tell_coordinator_about_backup_load_result(
        &mut self,
        phase: EnterBackupPhase,
        share_backup: ShareBackup,
    ) -> impl IntoIterator<Item = DeviceSend> {
        let mut ret = vec![];
        let enter_physical_id = phase.enter_physical_id;

        let share_image = share_backup.share_image();
        self.restoration
            .tmp_loaded_backups
            .insert(share_image, share_backup);

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

    pub fn saved_backups(&self) -> &BTreeMap<ShareImage, SavedBackup2> {
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
    DisplayBackupRequest {
        phase: Box<BackupDisplayPhase>,
    },
    DisplayBackup {
        key_name: String,
        backup: ShareBackup,
    },
}

#[derive(Debug, Clone)]
pub struct ConsolidatePhase {
    pub complete_share: CompleteSecretShare,
    pub coord_contrib: CoordShareDecryptionContrib,
}

#[derive(Clone, Debug)]
pub struct BackupDisplayPhase {
    pub access_structure_ref: AccessStructureRef,
    pub party_index: ShareIndex,
    pub encrypted_secret_share: Ciphertext<32, Scalar<Secret, Zero>>,
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
    pub key_name: String,
    pub root_shared_key: SharedKey,
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
