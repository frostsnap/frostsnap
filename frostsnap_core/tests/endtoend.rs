use bitcoin::Address;
use common::{TestDeviceKeyGen, TEST_ENCRYPTION_KEY};
use frostsnap_core::bitcoin_transaction::{LocalSpk, TransactionTemplate};
use frostsnap_core::coordinator::restoration::{PhysicalBackupPhase, RecoverShare};
use frostsnap_core::device::{self, DeviceToUserMessage, KeyPurpose};
use frostsnap_core::message::EncodedSignature;
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_core::{
    coordinator::{
        CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, CoordinatorToUserSigningMessage,
        FrostCoordinator,
    },
    CheckedSignTask, DeviceId, MasterAppkey, SessionHash, WireSignTask,
};
use frostsnap_core::{EnterPhysicalId, KeyId, RestorationId, SignSessionId};
use rand::seq::IteratorRandom;
use rand::RngCore;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::frost::SecretShare;
use schnorr_fun::fun::{g, s, G};
use schnorr_fun::{Schnorr, Signature};
use std::collections::{BTreeMap, BTreeSet};

mod common;
use crate::common::Run;

#[derive(Default)]
struct TestEnv {
    // keygen
    pub keygen_checks: BTreeMap<DeviceId, SessionHash>,
    pub received_keygen_shares: BTreeSet<DeviceId>,
    pub coordinator_check: Option<SessionHash>,
    pub coordinator_got_keygen_acks: BTreeSet<DeviceId>,
    pub keygen_acks: BTreeSet<KeyId>,

    // backups
    pub backups: BTreeMap<DeviceId, (String, String)>,
    pub physical_backups_entered: Vec<PhysicalBackupPhase>,

    // signing
    pub received_signing_shares: BTreeMap<SignSessionId, BTreeSet<DeviceId>>,
    pub sign_tasks: BTreeMap<DeviceId, CheckedSignTask>,
    pub signatures: BTreeMap<SignSessionId, Vec<Signature>>,

    pub verification_requests: BTreeMap<DeviceId, (Address, BitcoinBip32Path)>,

    // options
    pub enter_invalid_backup: bool,
}

impl common::Env for TestEnv {
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
            DeviceToUserMessage::FinalizeKeyGen => {}
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
                        let (_, backup) = self.backups.get(&from).unwrap();
                        let mut secret_share = SecretShare::from_bech32_backup(backup).unwrap();
                        if self.enter_invalid_backup {
                            secret_share.share += s!(42);
                        }
                        let response =
                            device.tell_coordinator_about_backup_load_result(phase, secret_share);
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

#[test]
fn when_we_generate_a_key_we_should_be_able_to_sign_with_it_multiple_times() {
    let n_parties = 3;
    let threshold = 2;
    let schnorr = Schnorr::<sha2::Sha256>::verify_only();
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([123u8; 32]);

    let mut run = Run::start_after_keygen_and_nonces(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        1,
        KeyPurpose::Test,
    );
    let device_list = run.devices.keys().cloned().collect::<Vec<_>>();

    let session_hash = env
        .coordinator_check
        .expect("coordinator should have seen session_hash");
    assert_eq!(
        env.keygen_checks.keys().cloned().collect::<BTreeSet<_>>(),
        run.device_set()
    );
    assert!(
        env.keygen_checks.values().all(|v| *v == session_hash),
        "devices should have seen the same hash"
    );

    assert_eq!(env.coordinator_got_keygen_acks, run.device_set());
    assert_eq!(env.received_keygen_shares, run.device_set());
    let key_data = run.coordinator.iter_keys().next().unwrap().clone();
    let access_structure_ref = key_data
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    for (message, signers) in [("johnmcafee47", [0, 1]), ("pyramid schmee", [1, 2])] {
        env.signatures.clear();
        env.sign_tasks.clear();
        env.received_signing_shares.clear();
        let task = WireSignTask::Test {
            message: message.into(),
        };
        let complete_key = key_data.complete_key.clone();
        let checked_task = task
            .clone()
            .check(complete_key.master_appkey, KeyPurpose::Test)
            .unwrap();

        let signing_set = BTreeSet::from_iter(signers.iter().map(|i| device_list[*i]));

        let session_id = run
            .coordinator
            .start_sign(
                access_structure_ref,
                task.clone(),
                &signing_set,
                &mut test_rng,
            )
            .unwrap();

        for &device in &signing_set {
            let sign_req =
                run.coordinator
                    .request_device_sign(session_id, device, TEST_ENCRYPTION_KEY);
            run.extend(sign_req);
        }
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
        assert_eq!(
            env.received_signing_shares.get(&session_id).unwrap(),
            &signing_set
        );
        assert!(checked_task
            .verify_final_signatures(&schnorr, env.signatures.get(&session_id).unwrap()));

        // check view of the coordianttor and device nonces are the same
        // TODO: maybe try and do this check again
        // for &device in &device_set {
        //     let coord_nonces = run.coordinator.device_nonces().get(&device).cloned();
        //     let coord_nonce_counter = coord_nonces
        //         .clone()
        //         .map(|nonces| nonces.start_index)
        //         .unwrap_or(0);
        //     let device_nonce_counter = run.device(device).nonce_counter();
        //     assert_eq!(device_nonce_counter, coord_nonce_counter);
        //     let coord_next_nonce =
        //         coord_nonces.and_then(|nonces| nonces.nonces.iter().next().cloned());

        //     let device_nonce = run
        //         .devices
        //         .get(&device)
        //         .unwrap()
        //         .generate_public_nonces(device_nonce_counter)
        //         .next();
        //     assert_eq!(device_nonce, coord_next_nonce);
        // }
    }
}

#[test]
fn test_display_backup() {
    use rand::seq::SliceRandom;
    let n_parties = 3;
    let threshold = 2;
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([123u8; 32]);

    let mut run = Run::start_after_keygen(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        KeyPurpose::Test,
    );
    let device_list = run.devices.keys().cloned().collect::<Vec<_>>();

    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    assert_eq!(
        env.backups.len(),
        0,
        "no backups should have been displayed automatically"
    );

    env.backups = BTreeMap::new(); // clear backups so we can request one again for a party
    let display_backup = run
        .coordinator
        .request_device_display_backup(device_list[0], access_structure_ref, TEST_ENCRYPTION_KEY)
        .unwrap();

    run.extend(display_backup);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    assert_eq!(env.backups.len(), 1);

    let mut display_backup = run
        .coordinator
        .request_device_display_backup(device_list[1], access_structure_ref, TEST_ENCRYPTION_KEY)
        .unwrap();
    display_backup.extend(
        run.coordinator
            .request_device_display_backup(
                device_list[2],
                access_structure_ref,
                TEST_ENCRYPTION_KEY,
            )
            .unwrap(),
    );
    run.extend(display_backup);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    assert_eq!(env.backups.len(), 3);

    let decoded_backups = env
        .backups
        .values()
        .map(|(_name, backup)| {
            schnorr_fun::frost::SecretShare::from_bech32_backup(backup).expect("valid backup")
        })
        .collect::<Vec<_>>();

    let interpolated_joint_secret = schnorr_fun::frost::SecretShare::recover_secret(
        &decoded_backups
            .choose_multiple(&mut test_rng, 2)
            .cloned()
            .collect::<Vec<_>>(),
    )
    .non_zero()
    .unwrap();

    let key_data = run
        .coordinator
        .get_frost_key(access_structure_ref.key_id)
        .unwrap();
    assert_eq!(
        MasterAppkey::derive_from_rootkey(g!(interpolated_joint_secret * G).normalize()),
        key_data.complete_key.master_appkey
    );
}

#[test]
fn test_verify_address() {
    let n_parties = 3;
    let threshold = 2;
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([123u8; 32]);

    let mut run = Run::start_after_keygen(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
    );

    let key_data = run.coordinator.iter_keys().next().unwrap().clone();

    let verify_request = run.coordinator.verify_address(key_data.key_id, 0).unwrap();
    run.extend(verify_request);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    assert_eq!(env.verification_requests.len(), 3);
}

#[test]
fn when_we_abandon_a_sign_request_we_should_be_able_to_start_a_new_one() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = TestEnv::default();
    let mut run =
        Run::start_after_keygen_and_nonces(1, 1, &mut env, &mut test_rng, 1, KeyPurpose::Test);

    let device_set = run.device_set();

    for _ in 0..101 {
        let access_structure_ref = run
            .coordinator
            .iter_access_structures()
            .next()
            .unwrap()
            .access_structure_ref();

        let uncompleting_sign_task = WireSignTask::Test {
            message: "frostsnap in taiwan".into(),
        };

        let _unused_sign_session_id = run
            .coordinator
            .start_sign(
                access_structure_ref,
                uncompleting_sign_task,
                &device_set,
                &mut test_rng,
            )
            .unwrap();

        // fully cancel sign request
        run.coordinator.cancel_sign_session(_unused_sign_session_id);
        let completing_sign_task = WireSignTask::Test {
            message: "rip purple boards rip blue boards rip frostypedeV1".into(),
        };

        let used_sign_session_id = run
            .coordinator
            .start_sign(
                access_structure_ref,
                completing_sign_task,
                &device_set,
                &mut test_rng,
            )
            .unwrap();

        for &device_id in &device_set {
            let sign_req = run.coordinator.request_device_sign(
                used_sign_session_id,
                device_id,
                TEST_ENCRYPTION_KEY,
            );
            run.extend(sign_req);
        }

        // Test that this run completes without erroring, fully replenishing the nonces
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }
}

#[test]
fn signing_a_bitcoin_transaction_produces_valid_signatures() {
    let n_parties = 3;
    let threshold = 2;
    let schnorr = Schnorr::<sha2::Sha256>::verify_only();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen_and_nonces(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        1,
        KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
    );
    let device_set = run.device_set();

    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    let key_data = run
        .coordinator
        .get_frost_key(access_structure_ref.key_id)
        .unwrap();
    let mut tx_template = TransactionTemplate::new();

    let master_appkey = key_data.complete_key.master_appkey;

    tx_template.push_imaginary_owned_input(
        LocalSpk {
            master_appkey,
            bip32_path: BitcoinBip32Path::external(7),
        },
        bitcoin::Amount::from_sat(42_000),
    );

    tx_template.push_imaginary_owned_input(
        LocalSpk {
            master_appkey,
            bip32_path: BitcoinBip32Path::internal(42),
        },
        bitcoin::Amount::from_sat(1_337_000),
    );

    let task = WireSignTask::BitcoinTransaction(tx_template);
    let checked_task = task
        .clone()
        .check(
            master_appkey,
            KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
        )
        .unwrap();

    let set = device_set
        .iter()
        .choose_multiple(&mut test_rng, 2)
        .into_iter()
        .cloned()
        .collect();

    let session_id = run
        .coordinator
        .start_sign(access_structure_ref, task.clone(), &set, &mut test_rng)
        .unwrap();

    for &device_id in &set {
        let sign_req =
            run.coordinator
                .request_device_sign(session_id, device_id, TEST_ENCRYPTION_KEY);

        run.extend(sign_req);
    }
    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    assert!(
        checked_task.verify_final_signatures(&schnorr, env.signatures.get(&session_id).unwrap())
    );
    // TODO: test actual transaction validity
}

#[test]
fn check_share_for_valid_share_works() {
    let n_parties = 3;
    let threshold = 2;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        KeyPurpose::Test,
    );
    let device_set = run.device_set();
    let enter_physical_id = EnterPhysicalId::new(&mut test_rng);

    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    for device_id in device_set {
        let display_backup = run
            .coordinator
            .request_device_display_backup(device_id, access_structure_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(display_backup);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
        let enter_backup = run
            .coordinator
            .tell_device_to_load_physical_backup(enter_physical_id, device_id);
        run.extend(enter_backup);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }

    assert_eq!(env.physical_backups_entered.len(), n_parties);

    for physical_backup in env.physical_backups_entered {
        run.coordinator
            .check_physical_backup(access_structure_ref, physical_backup, TEST_ENCRYPTION_KEY)
            .unwrap();
    }
}

#[test]
fn check_share_for_invalid_share_fails() {
    let n_parties = 3;
    let threshold = 2;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = TestEnv {
        enter_invalid_backup: true,
        ..Default::default()
    };
    let mut run = Run::start_after_keygen(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        KeyPurpose::Test,
    );
    let enter_physical_id = EnterPhysicalId::new(&mut test_rng);
    let device_set = run.device_set();

    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    for device_id in device_set {
        let display_backup = run
            .coordinator
            .request_device_display_backup(device_id, access_structure_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(display_backup);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
        let enter_backup = run
            .coordinator
            .tell_device_to_load_physical_backup(enter_physical_id, device_id);
        run.extend(enter_backup);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }

    assert_eq!(env.physical_backups_entered.len(), n_parties);

    for physical_backup in env.physical_backups_entered {
        run.coordinator
            .check_physical_backup(access_structure_ref, physical_backup, TEST_ENCRYPTION_KEY)
            .unwrap_err();
    }
}

#[test]
fn restore_a_share_by_connecting_devices_to_a_new_coordinator() {
    let n_parties = 3;
    let threshold = 2;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        KeyPurpose::Test,
    );
    let device_set = run.device_set();

    run.check_mutations();
    let access_structure = run.coordinator.iter_access_structures().next().unwrap();

    // replace coordinator with a fresh one that doesn't know about the key
    run.replace_coordiantor(FrostCoordinator::new());

    let restoring_devices = device_set.iter().cloned().take(2).collect::<Vec<_>>();

    for device_id in restoring_devices {
        let messages = run.coordinator.request_held_shares(device_id);
        run.extend(messages);
    }

    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let restoration = run.coordinator.restoring().next().unwrap();
    run.coordinator
        .finish_restoring(
            restoration.restoration_id,
            TEST_ENCRYPTION_KEY,
            &mut test_rng,
        )
        .unwrap();
    let restored_access_structure = run
        .coordinator
        .iter_access_structures()
        .next()
        .expect("two devices should have been enough to restore the share");

    assert_eq!(
        restored_access_structure.access_structure_ref(),
        access_structure.access_structure_ref()
    );
    assert_eq!(
        restored_access_structure.device_to_share_indicies().len(),
        2
    );

    let final_device = device_set.iter().cloned().nth(2).unwrap();
    let messages = run.coordinator.request_held_shares(final_device);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let restored_access_structure = run
        .coordinator
        .iter_access_structures()
        .next()
        .expect("should still be restored");

    assert_eq!(
        run.coordinator
            .get_frost_key(access_structure.access_structure_ref().key_id)
            .unwrap()
            .purpose,
        KeyPurpose::Test,
        "check the purpose was restored"
    );

    assert_eq!(restored_access_structure, access_structure);
}

#[test]
fn delete_then_restore_a_key_by_connecting_devices_to_coordinator() {
    let n_parties = 3;
    let threshold = 2;
    let mut rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = TestEnv::default();
    let mut run =
        Run::start_after_keygen(n_parties, threshold, &mut env, &mut rng, KeyPurpose::Test);
    let device_set = run.device_set();

    run.check_mutations();
    let access_structure = run.coordinator.iter_access_structures().next().unwrap();
    let access_structure_ref = access_structure.access_structure_ref();

    run.coordinator.delete_key(access_structure_ref.key_id);

    assert_eq!(run.coordinator.iter_access_structures().count(), 0);
    assert_eq!(
        run.coordinator.get_frost_key(access_structure_ref.key_id),
        None
    );

    let mut recover_next_share = |run: &mut Run, i: usize, rng: &mut ChaCha20Rng| {
        let device_id = device_set.iter().cloned().skip(i).take(1).next().unwrap();
        let messages = run.coordinator.request_held_shares(device_id);
        run.extend(messages);
        run.run_until_finished(&mut env, rng).unwrap();
    };

    recover_next_share(&mut run, 0, &mut rng);
    assert!(
        !run.coordinator.staged_mutations().is_empty(),
        "recovering share should mutate something"
    );
    run.check_mutations(); // this clears mutations

    recover_next_share(&mut run, 0, &mut rng);

    assert!(
        run.coordinator.staged_mutations().is_empty(),
        "recovering share again should not mutate"
    );

    let restoration = run
        .coordinator
        .restoring()
        .next()
        .expect("one device should be enough to restore the key");
    let restoration_id = restoration.restoration_id;

    assert!(!restoration.access_structure.is_restorable());
    assert_eq!(restoration.access_structure_ref, Some(access_structure_ref));

    recover_next_share(&mut run, 1, &mut rng);

    assert!(
        !run.coordinator.staged_mutations().is_empty(),
        "recovering share should mutate something"
    );

    let restoration = run
        .coordinator
        .get_restoration_state(restoration_id)
        .unwrap();

    assert!(restoration.access_structure.is_restorable());
    run.coordinator
        .finish_restoring(restoration.restoration_id, TEST_ENCRYPTION_KEY, &mut rng)
        .unwrap();

    assert_eq!(run.coordinator.restoring().count(), 0);

    recover_next_share(&mut run, 2, &mut rng);

    let restored_access_structure = run.coordinator.iter_access_structures().next().unwrap();
    assert_eq!(
        restored_access_structure, access_structure,
        "should have fully recovered the access structure now"
    );

    run.check_mutations();

    recover_next_share(&mut run, 0, &mut rng);
    assert!(
        run.coordinator.staged_mutations().is_empty(),
        "receiving same shares again should not mutate"
    );
    recover_next_share(&mut run, 1, &mut rng);
    recover_next_share(&mut run, 2, &mut rng);
}

#[test]
fn we_should_be_able_to_switch_between_sign_sessions() {
    let n_parties = 3;
    let threshold = 2;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen_and_nonces(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        2,
        KeyPurpose::Test,
    );
    let device_set = run.device_set();

    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    let sign_task_1 = WireSignTask::Test {
        message: "one".into(),
    };

    let signing_set_1 = device_set.iter().cloned().take(2).collect();

    let ssid1 = run
        .coordinator
        .start_sign(
            access_structure_ref,
            sign_task_1,
            &signing_set_1,
            &mut test_rng,
        )
        .unwrap();

    let signing_set_2 = device_set.iter().cloned().skip(1).take(2).collect();

    let sign_task_2 = WireSignTask::Test {
        message: "two".into(),
    };

    let ssid2 = run
        .coordinator
        .start_sign(
            access_structure_ref,
            sign_task_2,
            &signing_set_2,
            &mut test_rng,
        )
        .unwrap();

    let mut signers_1 = signing_set_1.into_iter();
    let mut signers_2 = signing_set_2.into_iter();

    for _ in 0..2 {
        let sign_req = run.coordinator.request_device_sign(
            ssid1,
            signers_1.next().unwrap(),
            TEST_ENCRYPTION_KEY,
        );
        run.extend(sign_req);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();

        let sign_req = run.coordinator.request_device_sign(
            ssid2,
            signers_2.next().unwrap(),
            TEST_ENCRYPTION_KEY,
        );
        run.extend(sign_req);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }

    assert_eq!(env.signatures.len(), 2);
}

#[test]
fn nonces_available_should_heal_itself_when_outcome_of_sign_request_is_ambigious() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut env = TestEnv::default();
    let mut run =
        Run::start_after_keygen_and_nonces(1, 1, &mut env, &mut test_rng, 1, KeyPurpose::Test);
    let device_set = run.device_set();
    let device_id = device_set.iter().cloned().next().unwrap();

    let available_at_start = run.coordinator.nonces_available(device_id);

    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    let sign_task = WireSignTask::Test {
        message: "one".into(),
    };

    let ssid = run
        .coordinator
        .start_sign(access_structure_ref, sign_task, &device_set, &mut test_rng)
        .unwrap();

    // we going to take the request but not send it.
    // Coordinator is now unsure if it ever reached the device.
    let _sign_req = run
        .coordinator
        .request_device_sign(ssid, device_id, TEST_ENCRYPTION_KEY);

    let nonces_available_after_request = run.coordinator.nonces_available(device_id);
    assert_ne!(nonces_available_after_request, available_at_start);

    run.coordinator.cancel_sign_session(ssid);
    let nonces_available_after_cancel = run.coordinator.nonces_available(device_id);
    assert_ne!(
        nonces_available_after_cancel, available_at_start,
        "canceling should not reclaim ambigious nonces"
    );

    // now we simulate reconnecting the device. The coordinator should recognise once of its nonce
    // streams has less nonces than usual and reset that stream. Since the device never signed the
    // request it should happily reset the stream to what it was before.
    run.extend(
        run.coordinator
            .maybe_request_nonce_replenishment(device_id, 1, &mut test_rng),
    );
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let nonces_available = run.coordinator.nonces_available(device_id);

    assert_eq!(nonces_available, available_at_start);
}
