use bitcoin::Address;
use common::{TestDeviceKeygen, TEST_ENCRYPTION_KEY};
use frostsnap_core::bitcoin_transaction::{LocalSpk, TransactionTemplate};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::{
    CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, CoordinatorToUserSigningMessage,
    DeviceToUserMessage, EncodedSignature,
};
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_core::KeyId;
use frostsnap_core::{
    coordinator::FrostCoordinator, device::FrostSigner, CheckedSignTask, DeviceId, MasterAppkey,
    SessionHash, SignTask,
};
use rand::seq::IteratorRandom;
use rand::RngCore;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::frost::SecretShare;
use schnorr_fun::fun::{g, G};
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
    pub backup_confirmed_on_coordinator: BTreeSet<DeviceId>,

    // signing
    pub received_signing_shares: BTreeSet<DeviceId>,
    pub sign_tasks: BTreeMap<DeviceId, CheckedSignTask>,
    pub signatures: Vec<Signature>,

    pub verification_requests: BTreeMap<DeviceId, (Address, BitcoinBip32Path)>,
}

impl common::Env for TestEnv {
    fn user_react_to_coordinator(
        &mut self,
        run: &mut Run,
        message: CoordinatorToUserMessage,
        rng: &mut impl RngCore,
    ) {
        match message {
            CoordinatorToUserMessage::KeyGen(keygen_message) => match keygen_message {
                CoordinatorToUserKeyGenMessage::ReceivedShares { from } => {
                    assert!(
                        self.received_keygen_shares.insert(from),
                        "should not have already received"
                    )
                }
                CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
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
                        let access_structure_ref = run
                            .coordinator
                            .final_keygen_ack(TEST_ENCRYPTION_KEY, rng)
                            .unwrap();
                        self.keygen_acks.insert(access_structure_ref.key_id);
                    }
                }
            },
            CoordinatorToUserMessage::Signing(signing_message) => match signing_message {
                CoordinatorToUserSigningMessage::GotShare { from } => {
                    assert!(
                        self.received_signing_shares.insert(from),
                        "should only send share once"
                    );
                }
                CoordinatorToUserSigningMessage::Signed { signatures } => {
                    self.signatures = signatures
                        .into_iter()
                        .map(EncodedSignature::into_decoded)
                        .collect::<Option<Vec<_>>>()
                        .unwrap();
                }
            },
            CoordinatorToUserMessage::DisplayBackupConfirmed { device_id } => {
                self.backup_confirmed_on_coordinator.insert(device_id);
            }
            CoordinatorToUserMessage::EnteredBackup { valid, .. } => {
                assert!(valid, "entered share was valid");
            }
            CoordinatorToUserMessage::PromptRecoverShare(recover_share) => {
                run.coordinator
                    .recover_share_and_maybe_recover_access_structure(
                        *recover_share,
                        TEST_ENCRYPTION_KEY,
                        rng,
                    )
                    .unwrap();
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
            DeviceToUserMessage::CheckKeyGen {
                session_hash,
                key_id: _,
                ..
            } => {
                self.keygen_checks.insert(from, session_hash);
                let ack = run
                    .device(from)
                    .keygen_ack(&mut TestDeviceKeygen, rng)
                    .unwrap();
                run.extend_from_device(from, ack);
            }
            DeviceToUserMessage::SignatureRequest {
                sign_task,
                master_appkey: _,
            } => {
                self.sign_tasks.insert(from, sign_task);
                let sign_ack = run.device(from).sign_ack(&mut TestDeviceKeygen).unwrap();
                run.extend_from_device(from, sign_ack);
            }
            DeviceToUserMessage::DisplayBackupRequest { .. } => {
                let backup_ack = run
                    .device(from)
                    .display_backup_ack(&mut TestDeviceKeygen)
                    .unwrap();
                run.extend_from_device(from, backup_ack);
            }
            DeviceToUserMessage::DisplayBackup { key_name, backup } => {
                self.backups.insert(from, (key_name, backup));
            }
            DeviceToUserMessage::Canceled { .. } => {
                panic!("no cancelling done");
            }
            DeviceToUserMessage::EnterBackup { .. } => {
                let device = run.device(from);
                let (_, backup) = self.backups.get(&from).unwrap();
                let secret_share = SecretShare::from_bech32_backup(backup).unwrap();
                let response = device.loaded_share_backup(secret_share).unwrap();
                run.extend_from_device(from, response);
            }
            DeviceToUserMessage::EnteredBackup(_) => {
                panic!("restoring backups untested")
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
    let coordinator = FrostCoordinator::new();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let devices = (0..n_parties)
        .map(|_| FrostSigner::new_random(&mut test_rng))
        .map(|device| (device.device_id(), device))
        .collect::<BTreeMap<_, _>>();

    let device_set = devices.keys().cloned().collect::<BTreeSet<_>>();
    let device_list = devices.keys().cloned().collect::<Vec<_>>();
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([123u8; 32]);

    let mut run = Run::new(coordinator, devices);

    // set up nonces for devices first
    for &device_id in &device_set {
        run.extend(run.coordinator.maybe_request_nonce_replenishment(device_id));
    }
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my new key".to_string(),
            KeyPurpose::Test,
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let session_hash = env
        .coordinator_check
        .expect("coordinator should have seen session_hash");
    assert_eq!(
        env.keygen_checks.keys().cloned().collect::<BTreeSet<_>>(),
        device_set
    );
    assert!(
        env.keygen_checks.values().all(|v| *v == session_hash),
        "devices should have seen the same hash"
    );

    assert_eq!(env.coordinator_got_keygen_acks, device_set);
    assert_eq!(env.received_keygen_shares, device_set);
    let key_data = run.coordinator.iter_keys().next().unwrap().clone();
    let (access_structure_ref, _) = key_data.access_structures().next().unwrap();

    for (message, signers) in [("johnmcafee47", [0, 1]), ("pyramid schmee", [1, 2])] {
        env.signatures.clear();
        env.sign_tasks.clear();
        env.received_signing_shares.clear();
        let task = SignTask::Plain {
            message: message.into(),
        };
        let complete_key = key_data.complete_key.as_ref().unwrap();
        let checked_task = task.clone().check(complete_key.master_appkey).unwrap();
        let set = BTreeSet::from_iter(signers.iter().map(|i| device_list[*i]));

        let sign_init = run
            .coordinator
            .start_sign(
                access_structure_ref,
                task.clone(),
                set.clone(),
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        run.extend(sign_init);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
        assert_eq!(env.sign_tasks.keys().cloned().collect::<BTreeSet<_>>(), set);
        assert!(env.sign_tasks.values().all(|v| *v == checked_task));
        assert_eq!(env.received_signing_shares, set);
        assert!(checked_task.verify_final_signatures(&schnorr, &env.signatures));

        // check view of the coordianttor and device nonces are the same
        for &device in &device_set {
            let coord_nonces = run.coordinator.device_nonces().get(&device).cloned();
            let coord_nonce_counter = coord_nonces
                .clone()
                .map(|nonces| nonces.start_index)
                .unwrap_or(0);
            let device_nonce_counter = run.device(device).nonce_counter();
            assert_eq!(device_nonce_counter, coord_nonce_counter);
            let coord_next_nonce =
                coord_nonces.and_then(|nonces| nonces.nonces.iter().next().cloned());

            let device_nonce = run
                .devices
                .get(&device)
                .unwrap()
                .generate_public_nonces(device_nonce_counter)
                .next();
            assert_eq!(device_nonce, coord_next_nonce);
        }
    }
}

#[test]
fn test_display_backup() {
    use rand::seq::SliceRandom;
    let n_parties = 3;
    let threshold = 2;
    let coordinator = FrostCoordinator::new();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let devices = (0..n_parties)
        .map(|_| FrostSigner::new_random(&mut test_rng))
        .map(|device| (device.device_id(), device))
        .collect::<BTreeMap<_, _>>();

    let device_set = devices.keys().cloned().collect::<BTreeSet<_>>();
    let device_list = devices.keys().cloned().collect::<Vec<_>>();
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([123u8; 32]);

    let mut run = Run::new(coordinator, devices);

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my key".to_string(),
            KeyPurpose::Test,
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let (access_structure_ref, access_structure) =
        run.coordinator.iter_access_structures().next().unwrap();

    assert_eq!(
        env.backups.len(),
        0,
        "no backups should have been displayed automatically"
    );

    env.backups = BTreeMap::new(); // clear backups so we can request one again for a party
    let display_backup = run
        .coordinator
        .request_device_display_backup(
            device_list[0],
            access_structure.access_structure_ref(),
            TEST_ENCRYPTION_KEY,
        )
        .unwrap();

    run.extend(display_backup);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    assert_eq!(env.backups.len(), 1);
    assert_eq!(env.backup_confirmed_on_coordinator.len(), 1);

    let mut display_backup = run
        .coordinator
        .request_device_display_backup(
            device_list[1],
            access_structure.access_structure_ref(),
            TEST_ENCRYPTION_KEY,
        )
        .unwrap();
    display_backup.extend(
        run.coordinator
            .request_device_display_backup(
                device_list[2],
                access_structure.access_structure_ref(),
                TEST_ENCRYPTION_KEY,
            )
            .unwrap(),
    );
    run.extend(display_backup);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    assert_eq!(env.backups.len(), 3);
    assert_eq!(env.backup_confirmed_on_coordinator.len(), 3);

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
        key_data.complete_key.as_ref().unwrap().master_appkey
    );
}

#[test]
fn test_verify_address() {
    let n_parties = 3;
    let threshold = 2;
    let coordinator = FrostCoordinator::new();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let devices = (0..n_parties)
        .map(|_| FrostSigner::new_random(&mut test_rng))
        .map(|device| (device.device_id(), device))
        .collect::<BTreeMap<_, _>>();

    let device_set = devices.keys().cloned().collect::<BTreeSet<_>>();
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([123u8; 32]);

    let mut run = Run::new(coordinator, devices);

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my key".to_string(),
            KeyPurpose::Bitcoin(bitcoin::Network::Signet),
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let key_data = run.coordinator.iter_keys().next().unwrap().clone();

    let verify_request = run.coordinator.verify_address(key_data.key_id, 0).unwrap();
    run.extend(verify_request);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    assert_eq!(env.verification_requests.len(), 3);
}

// this test needs a better name and to properly explain what it's doing
#[test]
fn when_we_abandon_a_sign_request_we_should_be_able_to_start_a_new_one() {
    let threshold = 1;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut env = TestEnv::default();
    let mut run = Run::generate(1, &mut test_rng);
    let device_set = run.device_set();
    let device_id = *device_set.iter().next().unwrap();

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my key".to_string(),
            KeyPurpose::Test,
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);

    let request_nonces = run
        .coordinator
        .maybe_request_nonce_replenishment(device_id)
        .unwrap();
    run.extend(std::iter::once(request_nonces));

    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let (access_structure_ref, _access_structure) =
        run.coordinator.iter_access_structures().next().unwrap();

    let uncompleting_sign_task = SignTask::Plain {
        message: "frostsnap in taiwan".into(),
    };

    let initial_nonces_on_coordinator = run
        .coordinator
        .device_nonces()
        .get(&device_id)
        .unwrap()
        .clone();

    let _unused_sign_request = run
        .coordinator
        .start_sign(
            access_structure_ref,
            uncompleting_sign_task,
            device_set.clone(),
            TEST_ENCRYPTION_KEY,
        )
        .unwrap();

    let fewer_nonces_on_coordinator = run
        .coordinator
        .device_nonces()
        .get(&device_id)
        .unwrap()
        .clone();

    assert_eq!(
        fewer_nonces_on_coordinator.start_index,
        initial_nonces_on_coordinator.start_index + 1,
        "sanity check the coordinator counter increased"
    );
    assert_eq!(
        fewer_nonces_on_coordinator.nonces.len(),
        initial_nonces_on_coordinator.nonces.len() - 1,
        "testing coordinator has expended one nonce"
    );

    run.coordinator.cancel();

    let completing_sign_task = SignTask::Plain {
        message: "rip purple boards rip blue boards rip frostypedeV1".into(),
    };

    let used_sign_request = run
        .coordinator
        .start_sign(
            access_structure_ref,
            completing_sign_task,
            device_set,
            TEST_ENCRYPTION_KEY,
        )
        .unwrap()
        .clone();

    run.extend(used_sign_request);
    // Test that this run completes without erroring, fully replenishing the nonces
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let final_nonces_on_coordinator = run.coordinator.device_nonces().get(&device_id).unwrap();

    assert_eq!(
        final_nonces_on_coordinator.start_index,
        initial_nonces_on_coordinator.start_index + 2,
        "sanity check the coordinator counter has increased by two"
    );

    // Nonces should be fully replenished despite only having requested one signature!
    assert_eq!(
        final_nonces_on_coordinator.nonces.len(),
        initial_nonces_on_coordinator.nonces.len(),
        "testing that the device response fully replenished the coordinator nonces"
    );
}

#[test]
fn signing_a_bitcoin_transaction_produces_valid_signatures() {
    let n_parties = 3;
    let threshold = 2;
    let schnorr = Schnorr::<sha2::Sha256>::verify_only();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut run = Run::generate(n_parties, &mut test_rng);
    let mut env = TestEnv::default();
    let device_set = run.device_set();

    // set up nonces for devices first
    for &device_id in &device_set {
        run.extend(run.coordinator.maybe_request_nonce_replenishment(device_id));
    }

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my key".into(),
            KeyPurpose::Bitcoin(bitcoin::Network::Signet),
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let (access_structure_ref, _access_structure) =
        run.coordinator.iter_access_structures().next().unwrap();
    let key_data = run
        .coordinator
        .get_frost_key(access_structure_ref.key_id)
        .unwrap();
    let mut tx_template = TransactionTemplate::new();

    let master_appkey = key_data.complete_key.as_ref().unwrap().master_appkey;

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

    let task = SignTask::BitcoinTransaction(tx_template);
    let checked_task = task.clone().check(master_appkey).unwrap();

    let set = device_set
        .iter()
        .choose_multiple(&mut test_rng, 2)
        .into_iter()
        .cloned()
        .collect();

    let sign_init = run
        .coordinator
        .start_sign(access_structure_ref, task.clone(), set, TEST_ENCRYPTION_KEY)
        .unwrap();
    run.extend(sign_init);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    assert!(checked_task.verify_final_signatures(&schnorr, &env.signatures));
    // TODO: test actual transaction validity
}

#[test]
fn check_share_for_valid_share_works() {
    let n_parties = 3;
    let threshold = 2;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut run = Run::generate(n_parties, &mut test_rng);
    let mut env = TestEnv::default();
    let device_set = run.device_set();

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my key".into(),
            KeyPurpose::Test,
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let (access_structure_ref, _access_structure) =
        run.coordinator.iter_access_structures().next().unwrap();

    for device_id in device_set {
        let display_backup = run
            .coordinator
            .request_device_display_backup(device_id, access_structure_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(display_backup);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
        let check_share = run
            .coordinator
            .check_share(access_structure_ref, device_id, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(check_share);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }
}

#[test]
fn restore_a_share_by_connecting_devices_to_a_new_coordinator() {
    let n_parties = 3;
    let threshold = 2;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut run = Run::generate(n_parties, &mut test_rng);
    let mut env = TestEnv::default();
    let device_set = run.device_set();

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my key".into(),
            KeyPurpose::Test,
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    run.check_mutations();
    let (access_structure_ref, access_structure) =
        run.coordinator.iter_access_structures().next().unwrap();

    // replace coordinator with a fresh one that doesn't know about the key
    run.replace_coordiantor(FrostCoordinator::new());

    let restoring_devices = device_set.iter().cloned().take(2).collect::<Vec<_>>();

    for device_id in restoring_devices {
        let messages = run.coordinator.request_held_shares(device_id);
        run.extend(messages);
    }

    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let (restored_access_structure_ref, restored_access_structure) = run
        .coordinator
        .iter_access_structures()
        .next()
        .expect("two devices should have been enough to restore the share");

    assert_eq!(restored_access_structure_ref, access_structure_ref);
    assert_eq!(
        restored_access_structure.device_to_share_indicies().len(),
        2
    );

    let final_device = device_set.iter().cloned().nth(2).unwrap();
    let messages = run.coordinator.request_held_shares(final_device);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    let (_, restored_access_structure) = run
        .coordinator
        .iter_access_structures()
        .next()
        .expect("should still be restored");

    assert_eq!(
        run.coordinator
            .get_frost_key(access_structure_ref.key_id)
            .unwrap()
            .purpose,
        KeyPurpose::Test,
        "check the purpose was restored"
    );

    assert_eq!(restored_access_structure, access_structure);
}

#[test]
fn delete_then_restore_a_share_by_connecting_devices_to_coordinator() {
    let n_parties = 3;
    let threshold = 2;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut run = Run::generate(n_parties, &mut test_rng);
    let mut env = TestEnv::default();
    let device_set = run.device_set();

    let keygen_init = run
        .coordinator
        .do_keygen(
            &device_set,
            threshold,
            "my key".into(),
            KeyPurpose::Test,
            &mut test_rng,
        )
        .unwrap();
    run.extend(keygen_init);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();
    run.check_mutations();
    let (access_structure_ref, access_structure) =
        run.coordinator.iter_access_structures().next().unwrap();

    run.coordinator.delete_key(access_structure_ref.key_id);

    assert_eq!(run.coordinator.iter_access_structures().count(), 0);
    assert_eq!(
        run.coordinator.get_frost_key(access_structure_ref.key_id),
        None
    );

    let mut recover_next_share = |run: &mut Run, i: usize| {
        let device_id = device_set.iter().cloned().skip(i).take(1).next().unwrap();
        let messages = run.coordinator.request_held_shares(device_id);
        run.extend(messages);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    };

    recover_next_share(&mut run, 0);
    assert!(
        !run.coordinator.staged_mutations().is_empty(),
        "recovering share should mutate something"
    );
    run.check_mutations(); // this clears mutations

    recover_next_share(&mut run, 0);

    assert!(
        run.coordinator.staged_mutations().is_empty(),
        "recovering share again should not mutate"
    );

    let coord_key = run
        .coordinator
        .iter_keys()
        .next()
        .expect("one device should be enough to restore the key");

    assert_eq!(coord_key.complete_key, None);
    assert_eq!(coord_key.key_id, access_structure_ref.key_id);

    recover_next_share(&mut run, 1);

    assert!(
        !run.coordinator.staged_mutations().is_empty(),
        "recovering share should mutate something"
    );

    let (restored_access_structure_ref, restored_access_structure) = run
        .coordinator
        .iter_access_structures()
        .next()
        .expect("two devices should have restored the access structure");

    assert_eq!(restored_access_structure_ref, access_structure_ref);
    assert_eq!(
        restored_access_structure.device_to_share_indicies().len(),
        2
    );

    recover_next_share(&mut run, 2);

    let (_, restored_access_structure) = run
        .coordinator
        .iter_access_structures()
        .next()
        .expect("should still be restored");

    assert_eq!(restored_access_structure, access_structure);

    run.check_mutations();

    recover_next_share(&mut run, 0);
    assert!(
        run.coordinator.staged_mutations().is_empty(),
        "receiving same shares again should not mutate"
    );
    recover_next_share(&mut run, 1);
    recover_next_share(&mut run, 2);
}
