use crate::common::{Env, Run, TestDeviceKeyGen, TEST_ENCRYPTION_KEY};
use bitcoin::Address;
use frostsnap_core::coordinator::restoration::RecoverShare;
use frostsnap_core::device::{self, DeviceToUserMessage};
use frostsnap_core::message::EncodedSignature;
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_core::{
    coordinator::{
        CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, CoordinatorToUserSigningMessage,
    },
    CheckedSignTask, DeviceId, KeyId, RestorationId, SessionHash, SignSessionId,
};
use rand::RngCore;
use schnorr_fun::Signature;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct TestEnv {
    // keygen
    pub keygen_checks: BTreeMap<DeviceId, SessionHash>,
    pub received_keygen_shares: BTreeSet<DeviceId>,
    pub coordinator_check: Option<SessionHash>,
    pub coordinator_got_keygen_acks: BTreeSet<DeviceId>,
    pub keygen_acks: BTreeSet<KeyId>,

    // backups
    pub backups: BTreeMap<DeviceId, (String, frost_backup::ShareBackup)>,
    pub physical_backups_entered:
        Vec<frostsnap_core::coordinator::restoration::PhysicalBackupPhase>,

    // signing
    pub received_signing_shares: BTreeMap<SignSessionId, BTreeSet<DeviceId>>,
    pub sign_tasks: BTreeMap<DeviceId, CheckedSignTask>,
    pub signatures: BTreeMap<SignSessionId, Vec<Signature>>,

    pub verification_requests: BTreeMap<DeviceId, (Address, BitcoinBip32Path)>,

    // options
    pub enter_invalid_backup: bool,
}

impl Env for TestEnv {
    fn user_react_to_coordinator(
        &mut self,
        run: &mut Run,
        message: CoordinatorToUserMessage,
        rng: &mut impl RngCore,
    ) {
        match message {
            CoordinatorToUserMessage::KeyGen {
                keygen_id,
                inner: keygen_message,
            } => match keygen_message {
                CoordinatorToUserKeyGenMessage::ReceivedShares { from, .. } => {
                    assert!(
                        self.received_keygen_shares.insert(from),
                        "should not have already received"
                    )
                }
                CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash, .. } => {
                    assert!(
                        self.coordinator_check.replace(session_hash).is_none(),
                        "should not have already set this"
                    );
                }
                CoordinatorToUserKeyGenMessage::KeyGenAck {
                    from,
                    all_acks_received,
                } => {
                    assert!(
                        self.coordinator_got_keygen_acks.insert(from),
                        "should only receive this once"
                    );

                    if all_acks_received {
                        assert_eq!(
                            self.coordinator_got_keygen_acks.len(),
                            self.received_keygen_shares.len()
                        );
                        let send_finalize_keygen = run
                            .coordinator
                            .finalize_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                            .unwrap();
                        self.keygen_acks
                            .insert(send_finalize_keygen.access_structure_ref.key_id);
                        run.extend(send_finalize_keygen);
                    }
                }
            },
            CoordinatorToUserMessage::Signing(signing_message) => match signing_message {
                CoordinatorToUserSigningMessage::GotShare { from, session_id } => {
                    assert!(
                        self.received_signing_shares
                            .entry(session_id)
                            .or_default()
                            .insert(from),
                        "should only send share once"
                    );
                }
                CoordinatorToUserSigningMessage::Signed {
                    session_id,
                    signatures,
                } => {
                    let sigs = self.signatures.entry(session_id).or_default();
                    assert!(sigs.is_empty(), "should only get the signed event once");
                    sigs.extend(
                        signatures
                            .into_iter()
                            .map(EncodedSignature::into_decoded)
                            .map(Option::unwrap),
                    );
                }
            },
            CoordinatorToUserMessage::Restoration(msg) => {
                use frostsnap_core::coordinator::restoration::ToUserRestoration::*;
                match msg {
                    GotHeldShares {
                        held_by, shares, ..
                    } => {
                        // This logic here is just about doing something sensible in the context of a test.
                        // We start a new restoration if we get a new share but don't already know about it.
                        for held_share in shares {
                            let recover_share = RecoverShare {
                                held_by,
                                held_share: held_share.clone(),
                            };

                            match held_share.access_structure_ref {
                                Some(access_structure_ref)
                                    if run
                                        .coordinator
                                        .get_access_structure(access_structure_ref)
                                        .is_some() =>
                                {
                                    if !run.coordinator.knows_about_share(
                                        held_by,
                                        access_structure_ref,
                                        held_share.share_image.index,
                                    ) {
                                        run.coordinator
                                            .recover_share(
                                                access_structure_ref,
                                                &recover_share,
                                                TEST_ENCRYPTION_KEY,
                                            )
                                            .unwrap();
                                    }
                                }
                                _ => {
                                    let existing_restoration =
                                        run.coordinator.restoring().find(|state| {
                                            state.access_structure_ref
                                                == held_share.access_structure_ref
                                        });

                                    match existing_restoration {
                                        Some(existing_restoration) => {
                                            if !existing_restoration
                                                .access_structure
                                                .has_got_share_image(
                                                    recover_share.held_by,
                                                    recover_share.held_share.share_image,
                                                )
                                            {
                                                run.coordinator
                                                    .add_recovery_share_to_restoration(
                                                        existing_restoration.restoration_id,
                                                        &recover_share,
                                                    )
                                                    .unwrap();
                                            }
                                        }
                                        None => {
                                            run.coordinator.start_restoring_key_from_recover_share(
                                                &recover_share,
                                                RestorationId::new(rng),
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                    PhysicalBackupEntered(physical_backup_phase) => {
                        self.physical_backups_entered.push(*physical_backup_phase);
                    }
                    _ => { /* ignored */ }
                }
            }
        }
    }

    fn user_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
        rng: &mut impl RngCore,
    ) {
        match message {
            DeviceToUserMessage::FinalizeKeyGen { .. } => {}
            DeviceToUserMessage::CheckKeyGen { phase, .. } => {
                self.keygen_checks.insert(from, phase.session_hash());
                let ack = run
                    .device(from)
                    .keygen_ack(*phase, &mut TestDeviceKeyGen, rng)
                    .unwrap();
                run.extend_from_device(from, ack);
            }
            DeviceToUserMessage::SignatureRequest { phase } => {
                self.sign_tasks.insert(from, phase.sign_task().clone());
                let sign_ack = run
                    .device(from)
                    .sign_ack(*phase, &mut TestDeviceKeyGen)
                    .unwrap();
                run.extend_from_device(from, sign_ack);
            }
            DeviceToUserMessage::Restoration(restoration) => {
                use device::restoration::ToUserRestoration::*;
                match restoration {
                    DisplayBackup { key_name, backup } => {
                        self.backups.insert(from, (key_name, backup));
                    }
                    EnterBackup { phase } => {
                        let device = run.device(from);
                        let (_, mut share_backup) = self.backups.get(&from).unwrap().clone();

                        if self.enter_invalid_backup {
                            share_backup = match u16::try_from(share_backup.index()).unwrap() {
                                1 =>"#1 MISS DRAFT FOLD BRIGHT HURRY CONCERT SOURCE CLUB EQUIP ELEGANT TOY LYRICS CAR CABIN SYRUP LECTURE TEAM EQUIP WET ECHO LINK SILVER PURCHASE LECTURE NEXT",
                                2 => "#2 BEST MIXTURE FOOT HABIT WORLD OBSERVE ADVICE ANNUAL ISSUE CAUSE PROPERTY GUESS RETURN HURDLE WEASEL CUP ONCE NOVEL MARCH VALVE BLIND TRIGGER CHAIR ACTOR MONTH",
                                _ => "#3 PANDA SPHERE HAIR BRAVE VIRUS CATTLE LOOP WRAP RAMP READY TIP BODY GIANT OYSTER DIZZY CRUSH DANGER SNOW PLANET SHOVE LIQUID CLAW RICE AMONG JOB",
                            }.parse().unwrap()
                        }

                        let response =
                            device.tell_coordinator_about_backup_load_result(phase, share_backup);
                        run.extend_from_device(from, response);
                    }
                    DisplayBackupRequest { phase } => {
                        let backup_ack = run
                            .device(from)
                            .display_backup_ack(*phase, &mut TestDeviceKeyGen)
                            .unwrap();
                        run.extend_from_device(from, backup_ack);
                    }
                    ConsolidateBackup(phase) => {
                        let ack = run.device(from).finish_consolidation(
                            &mut TestDeviceKeyGen,
                            phase,
                            rng,
                        );
                        run.extend_from_device(from, ack);
                    }
                    BackupSaved { .. } => { /* informational */ }
                }
            }
            DeviceToUserMessage::VerifyAddress {
                address,
                bip32_path,
            } => {
                self.verification_requests
                    .insert(from, (address, bip32_path));
            }
        }
    }
}
