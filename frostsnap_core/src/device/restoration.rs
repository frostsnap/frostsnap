use super::*;
use alloc::fmt::Debug;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct State {
    tmp_loaded_backups: BTreeMap<RestorationId, SecretShare>,
    saved_shares_incomplete: BTreeMap<RestorationId, SecretShare>,
}

impl State {
    pub fn apply_mutation_restoration(
        &mut self,
        mutation: RestorationMutation,
    ) -> Option<RestorationMutation> {
        use RestorationMutation::*;
        match &mutation {
            Save(incomplete_secret_share) => {
                self.saved_shares_incomplete.insert(
                    incomplete_secret_share.restoration_id,
                    incomplete_secret_share.secret_share,
                );
            }
            &Clear(restoration_id) => {
                self.saved_shares_incomplete.remove(&restoration_id)?;
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
            &CoordinatorRestoration::Load { restoration_id } => Ok(vec![DeviceSend::ToUser(
                Box::new(DeviceToUserMessage::Restoration(EnterBackup {
                    phase: LoadBackupPhase { restoration_id },
                })),
            )]),
            &CoordinatorRestoration::Save { restoration_id } => {
                if let Some(secret_share) =
                    self.restoration.tmp_loaded_backups.remove(&restoration_id)
                {
                    self.mutate(Mutation::Restoration(RestorationMutation::Save(
                        IncompleteSecretShare {
                            secret_share,
                            restoration_id,
                        },
                    )));

                    Ok(vec![DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::Restoration(DeviceRestoration::PhysicalSaved(
                            EnteredPhysicalBackup {
                                restoration_id,
                                share_image: ShareImage::from_secret(secret_share),
                            },
                        )),
                    ))])
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

                if let Some(secret_share) = self
                    .restoration
                    .saved_shares_incomplete
                    .get(&consolidate.restoration_id)
                    .cloned()
                {
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
                                    restoration_id: consolidate.restoration_id,
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
                        "unrecognized restoration",
                    ))
                } else {
                    // we've already consolidated it so just ignore
                    Ok(vec![DeviceSend::ToCoordinator(Box::new(
                        DeviceToCoordinatorMessage::Restoration(
                            DeviceRestoration::ExitedRecoveryMode {
                                restoration_id: consolidate.restoration_id,
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
        phase: LoadBackupPhase,
        secret_share: SecretShare,
    ) -> impl IntoIterator<Item = DeviceSend> {
        let mut ret = vec![];
        let restoration_id = phase.restoration_id;

        self.restoration
            .tmp_loaded_backups
            .insert(restoration_id, secret_share);

        let share_image = ShareImage::from_secret(secret_share);
        ret.push(DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::Restoration(DeviceRestoration::PhysicalLoaded(
                EnteredPhysicalBackup {
                    restoration_id,
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
                            access_structure_ref: AccessStructureRef {
                                access_structure_id: *access_structure_id,
                                key_id: *key_id,
                            },
                            threshold: access_structure.threshold,
                            purpose: key_data.purpose,
                        });
                    }
                }
            }
        }
        held_shares
    }

    pub fn exit_recovery_mode(
        &mut self,
        symm_keygen: &mut impl DeviceSymmetricKeyGen,
        phase: ConsolidatePhase,
        rng: &mut impl rand_core::RngCore,
    ) -> impl IntoIterator<Item = DeviceSend> {
        self.mutate(Mutation::Restoration(RestorationMutation::Clear(
            phase.restoration_id,
        )));

        self.save_complete_share(
            phase.complete_share,
            symm_keygen,
            phase.coord_contrib,
            false,
            rng,
        );

        vec![DeviceSend::ToCoordinator(Box::new(
            DeviceToCoordinatorMessage::Restoration(DeviceRestoration::ExitedRecoveryMode {
                restoration_id: phase.restoration_id,
            }),
        ))]
    }
}

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq)]
pub enum RestorationMutation {
    Save(IncompleteSecretShare),
    Clear(RestorationId),
}

#[derive(Debug, Clone)]
pub enum ToUserRestoration {
    EnterBackup { phase: LoadBackupPhase },
    ConsolidateBackup(ConsolidatePhase),
    DisplayBackupRequest { phase: Box<BackupDisplayPhase> },
    DisplayBackup { key_name: String, backup: String },
}

#[derive(Debug, Clone)]
pub struct ConsolidatePhase {
    pub complete_share: CompleteSecretShare,
    pub restoration_id: RestorationId,
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
pub struct LoadBackupPhase {
    pub restoration_id: RestorationId,
}
