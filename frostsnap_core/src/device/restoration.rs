use crate::EnterPhysicalId;

use super::*;
use alloc::fmt::Debug;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct State {
    tmp_loaded_backups: BTreeMap<ShareImage, SecretShare>,
    saved_backups: BTreeMap<ShareImage, SavedBackup>,
}

impl State {
    pub fn apply_mutation_restoration(
        &mut self,
        mutation: RestorationMutation,
    ) -> Option<RestorationMutation> {
        use RestorationMutation::*;
        match &mutation {
            Save(saved_backup) => {
                self.saved_backups.insert(
                    ShareImage::from_secret(saved_backup.secret_share),
                    saved_backup.clone(),
                );
            }
            UnSave(share_image) => {
                self.tmp_loaded_backups.remove(share_image);
                self.saved_backups.remove(share_image)?;
            }
        }
        Some(mutation)
    }

    pub fn clear_tmp_data(&mut self) {
        self.tmp_loaded_backups.clear();
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
                if let Some(secret_share) = self.restoration.tmp_loaded_backups.remove(&share_image)
                {
                    self.mutate(Mutation::Restoration(RestorationMutation::Save(
                        self::SavedBackup {
                            secret_share,
                            threshold,
                            purpose,
                            key_name: key_name.clone(),
                        },
                    )));
                    let share_image = ShareImage::from_secret(secret_share);

                    Ok(vec![
                        DeviceSend::ToUser(Box::new(DeviceToUserMessage::Restoration(
                            ToUserRestoration::BackupSaved {
                                share_image,
                                key_name: key_name.clone(),
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
                let share_image = ShareImage {
                    share_index: consolidate.share_index,
                    point: root_shared_key
                        .share_image(consolidate.share_index)
                        .normalize(),
                };

                let secret_share = self
                    .restoration
                    .saved_backups
                    .get(&share_image)
                    // XXX: We drop all the extra metadata and just extract the secret share.
                    // This data was always more of a hint about what was on the physical backup.
                    .map(|saved_backup| &saved_backup.secret_share)
                    .or_else(|| self.restoration.tmp_loaded_backups.get(&share_image))
                    .copied();

                if let Some(secret_share) = secret_share {
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
            } => {
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
                };
                Ok(vec![DeviceSend::ToUser(Box::new(
                    DeviceToUserMessage::Restoration(DisplayBackupRequest {
                        phase: Box::new(phase),
                    }),
                ))])
            }

            CoordinatorRestoration::RequestHeldShares => {
                let held_shares = self.held_shares();
                let send = Some(DeviceSend::ToCoordinator(Box::new(
                    DeviceToCoordinatorMessage::Restoration(DeviceRestoration::HeldShares(
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
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
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
        let backup = SecretShare {
            index: phase.party_index,
            share: secret_share,
        }
        .to_bech32_backup();
        Ok(vec![DeviceSend::ToUser(Box::new(
            DeviceToUserMessage::Restoration(ToUserRestoration::DisplayBackup {
                key_name: key_data.key_name.clone(),
                backup,
            }),
        ))])
    }

    pub fn tell_coordinator_about_backup_load_result(
        &mut self,
        phase: EnterBackupPhase,
        secret_share: SecretShare,
    ) -> impl IntoIterator<Item = DeviceSend> {
        let mut ret = vec![];
        let enter_physical_id = phase.enter_physical_id;

        let share_image = ShareImage::from_secret(secret_share);
        self.restoration
            .tmp_loaded_backups
            .insert(share_image, secret_share);

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

    pub fn held_shares(&self) -> Vec<HeldShare> {
        let mut held_shares = vec![];

        for (key_id, key_data) in &self.keys {
            for (access_structure_id, access_structure) in &key_data.access_structures {
                for share in access_structure.shares.values() {
                    if access_structure.kind == AccessStructureKind::Master {
                        held_shares.push(HeldShare {
                            key_name: key_data.key_name.clone(),
                            share_image: share.share_image,
                            access_structure_ref: Some(AccessStructureRef {
                                access_structure_id: *access_structure_id,
                                key_id: *key_id,
                            }),
                            threshold: access_structure.threshold,
                            purpose: key_data.purpose,
                        });
                    }
                }
            }
        }

        for (&share_image, saved_backup) in &self.restoration.saved_backups {
            held_shares.push(HeldShare {
                key_name: saved_backup.key_name.clone(),
                access_structure_ref: None,
                share_image,
                threshold: saved_backup.threshold,
                purpose: saved_backup.purpose,
            });
        }

        held_shares
    }

    pub fn saved_backups(&self) -> &BTreeMap<ShareImage, SavedBackup> {
        &self.restoration.saved_backups
    }

    pub fn finish_consolidation(
        &mut self,
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
        phase: ConsolidatePhase,
        rng: &mut impl rand_core::RngCore,
    ) -> impl IntoIterator<Item = DeviceSend> {
        let share_image = ShareImage::from_secret(phase.complete_share.secret_share);
        let access_structure_ref = phase.complete_share.access_structure_ref;
        self.mutate(Mutation::Restoration(RestorationMutation::UnSave(
            share_image,
        )));

        let encrypted_secret_share = EncryptedSecretShare::encrypt(
            phase.complete_share.secret_share,
            phase.complete_share.access_structure_ref,
            phase.coord_contrib,
            symm_keygen,
            rng,
        );
        self.save_complete_share(KeyGenPhase3 {
            key_name: phase.complete_share.key_name,
            key_purpose: phase.complete_share.purpose,
            access_structure_ref: phase.complete_share.access_structure_ref,
            access_structure_kind: AccessStructureKind::Master,
            threshold: phase.complete_share.threshold,
            encrypted_secret_share,
        });

        vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::Restoration(DeviceRestoration::FinishedConsolidation {
                share_index: share_image.share_index,
                access_structure_ref,
            }),
        ))]
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub enum RestorationMutation {
    Save(SavedBackup),
    UnSave(ShareImage),
}

#[derive(Debug, Clone)]
pub enum ToUserRestoration {
    EnterBackup {
        phase: EnterBackupPhase,
    },
    BackupSaved {
        share_image: ShareImage,
        key_name: String,
        purpose: KeyPurpose,
        threshold: u16,
    },
    ConsolidateBackup(ConsolidatePhase),
    DisplayBackupRequest {
        phase: Box<BackupDisplayPhase>,
    },
    DisplayBackup {
        key_name: String,
        backup: String,
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
    pub party_index: PartyIndex,
    pub encrypted_secret_share: Ciphertext<32, Scalar<Secret, Zero>>,
    pub coord_share_decryption_contrib: CoordShareDecryptionContrib,
    pub key_name: String,
}

#[derive(Clone, Debug)]
pub struct EnterBackupPhase {
    pub enter_physical_id: EnterPhysicalId,
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SavedBackup {
    pub secret_share: SecretShare,
    pub threshold: u16,
    pub purpose: KeyPurpose,
    pub key_name: String,
}
