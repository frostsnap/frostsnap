use frostsnap_core::message::{
    CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, CoordinatorToUserSigningMessage,
    DeviceToUserMessage, EncodedSignature, SignTask,
};
use frostsnap_core::{DeviceId, FrostCoordinator, FrostSigner, KeyId, SessionHash};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::binonce::Nonce;
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
    pub key_ids: BTreeSet<KeyId>,

    // signing
    pub received_signing_shares: BTreeSet<DeviceId>,
    pub sign_tasks: BTreeMap<DeviceId, SignTask>,
    pub signatures: Vec<Signature>,

    // storage
    pub coord_nonces: BTreeMap<(DeviceId, u64), Nonce>,
    pub device_nonces: BTreeMap<DeviceId, u64>,

    pub backups: BTreeMap<DeviceId, (KeyId, String)>,
    pub backup_confirmed_on_coordinator: BTreeSet<DeviceId>,
}

impl common::Env for TestEnv {
    fn storage_react_to_coordinator(
        &mut self,
        _run: &mut Run,
        message: frostsnap_core::message::CoordinatorToStorageMessage,
    ) {
        use frostsnap_core::message::CoordinatorToStorageMessage::*;
        match message {
            NewKey(_) => { /*  */ }
            NoncesUsed {
                device_id,
                nonce_counter,
            } => {
                let to_remove = self
                    .coord_nonces
                    .range((device_id, 0)..(device_id, nonce_counter))
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>();
                for k in to_remove {
                    self.coord_nonces.remove(&k);
                }
            }
            ResetNonces { nonces, device_id } => {
                for k in self
                    .coord_nonces
                    .range((device_id, 0)..(device_id, u64::MAX))
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>()
                {
                    self.coord_nonces.remove(&k);
                }
                self.coord_nonces.extend(
                    nonces
                        .nonces
                        .into_iter()
                        .enumerate()
                        .map(|(i, nonce)| ((device_id, nonces.start_index + i as u64), nonce)),
                );
            }
            NewNonces {
                device_id,
                new_nonces,
            } => {
                let start = self
                    .coord_nonces
                    .range((device_id, 0)..=(device_id, u64::MAX))
                    .last()
                    .map(|((_, i), _)| *i + 1)
                    .unwrap_or(0);
                self.coord_nonces.extend(
                    new_nonces
                        .into_iter()
                        .enumerate()
                        .map(|(i, nonce)| ((device_id, start + i as u64), nonce)),
                );
            }
            StoreSigningState(_) => { /*  */ }
        }
    }

    fn storage_react_to_device(
        &mut self,
        _run: &mut Run,
        from: DeviceId,
        message: frostsnap_core::message::DeviceToStorageMessage,
    ) {
        use frostsnap_core::message::DeviceToStorageMessage::*;
        match message {
            SaveKey(_) => { /*  */ }
            ExpendNonce { nonce_counter } => {
                self.device_nonces.insert(from, nonce_counter);
            }
        }
    }

    fn user_react_to_coordinator(&mut self, _run: &mut Run, message: CoordinatorToUserMessage) {
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
                CoordinatorToUserKeyGenMessage::KeyGenAck { from } => {
                    assert!(
                        self.coordinator_got_keygen_acks.insert(from),
                        "should only receive this once"
                    );
                }
                CoordinatorToUserKeyGenMessage::FinishedKey { key_id } => {
                    assert!(self.key_ids.insert(key_id), "should only receive this once");
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
        }
    }

    fn user_react_to_device(
        &mut self,
        run: &mut Run,
        from: DeviceId,
        message: DeviceToUserMessage,
    ) {
        match message {
            DeviceToUserMessage::CheckKeyGen { session_hash } => {
                self.keygen_checks.insert(from, session_hash);
                let ack = run.device(from).keygen_ack().unwrap();
                run.extend_from_device(from, ack);
            }
            DeviceToUserMessage::SignatureRequest {
                sign_task,
                key_id: _,
            } => {
                self.sign_tasks.insert(from, sign_task);
                let sign_ack = run.device(from).sign_ack().unwrap();
                run.extend_from_device(from, sign_ack);
            }
            DeviceToUserMessage::DisplayBackupRequest { .. } => {
                let backup_ack = run.device(from).display_backup_ack().unwrap();
                run.extend_from_device(from, backup_ack);
            }
            DeviceToUserMessage::DisplayBackup { key_id, backup } => {
                self.backups.insert(from, (key_id, backup));
            }
            DeviceToUserMessage::Canceled { .. } => {
                panic!("no cancelling done");
            }
        }
    }
}

#[test]
fn test_end_to_end() {
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
    run.run_until_finished(&mut env, &mut test_rng);

    let keygen_init = run.coordinator.do_keygen(&device_set, threshold).unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut env, &mut test_rng);
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
    let coord_frost_key = run.coordinator.iter_keys().next().unwrap();

    let key_id = coord_frost_key.key_id();
    let public_key = coord_frost_key.frost_key().public_key();

    run.run_until_finished(&mut env, &mut test_rng);

    for (message, signers) in [("johnmcafee47", [0, 1]), ("pyramid schmee", [1, 2])] {
        env.signatures.clear();
        env.sign_tasks.clear();
        env.received_signing_shares.clear();
        let task = SignTask::Plain {
            message: message.as_bytes().to_vec(),
        };
        let set = BTreeSet::from_iter(signers.iter().map(|i| device_list[*i]));

        let sign_init = run
            .coordinator
            .start_sign(key_id, task.clone(), set.clone())
            .unwrap();
        run.extend(sign_init);
        run.run_until_finished(&mut env, &mut test_rng);
        assert_eq!(env.sign_tasks.keys().cloned().collect::<BTreeSet<_>>(), set);
        assert!(env.sign_tasks.values().all(|v| *v == task));
        assert_eq!(env.received_signing_shares, set);
        assert!(task.verify(&schnorr, public_key, &env.signatures));

        // check view of the coordianttor and device nonces are the same
        for &device in &device_set {
            let (&(_, idx), &coord_next_nonce) = env
                .coord_nonces
                .range((device, 0)..=(device, u64::MAX))
                .next()
                .unwrap();
            let &nonce_counter = env.device_nonces.get(&device).unwrap_or(&0);
            assert_eq!(nonce_counter, idx);

            let device_nonce = run
                .devices
                .get(&device)
                .unwrap()
                .generate_public_nonces(idx)
                .next()
                .unwrap();
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

    let keygen_init = run.coordinator.do_keygen(&device_set, threshold).unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut env, &mut test_rng);
    let coord_frost_key = run.coordinator.iter_keys().next().unwrap().clone();
    let key_id = coord_frost_key.key_id();

    let display_backup = run
        .coordinator
        .request_device_display_backup(device_list[0], key_id)
        .unwrap();
    run.extend(display_backup);
    run.run_until_finished(&mut env, &mut test_rng);

    assert_eq!(env.backups.len(), 1);
    assert_eq!(env.backup_confirmed_on_coordinator.len(), 1);

    let mut display_backup = run
        .coordinator
        .request_device_display_backup(device_list[1], key_id)
        .unwrap();
    display_backup.extend(
        run.coordinator
            .request_device_display_backup(device_list[2], key_id)
            .unwrap(),
    );
    run.extend(display_backup);
    run.run_until_finished(&mut env, &mut test_rng);

    assert_eq!(env.backups.len(), 3);
    assert_eq!(env.backup_confirmed_on_coordinator.len(), 3);

    let decoded_backups = env
        .backups
        .values()
        .map(|(bu_key_id, backup)| {
            assert_eq!(*bu_key_id, key_id);
            let decoded =
                schnorr_fun::share_backup::decode_backup(backup.clone()).expect("valid backup");
            (decoded.share_index, decoded.secret_share)
        })
        .collect::<Vec<_>>();

    let threshold_interpolated_joint_secret =
        schnorr_fun::fun::poly::scalar::interpolate_and_eval_poly_at_0(
            decoded_backups
                .choose_multiple(&mut test_rng, 2)
                .cloned()
                .collect(),
        );
    let interpolated_joint_secret =
        schnorr_fun::fun::poly::scalar::interpolate_and_eval_poly_at_0(decoded_backups);
    assert_eq!(
        threshold_interpolated_joint_secret,
        interpolated_joint_secret
    );
    assert_eq!(
        g!(interpolated_joint_secret * G),
        coord_frost_key.frost_key().public_key()
    );
}

#[test]
fn fully_replenish_nonces() {
    let threshold = 1;
    let coordinator = FrostCoordinator::new();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let device = FrostSigner::new_random(&mut test_rng);
    let devices = BTreeMap::from_iter(std::iter::once((device.device_id(), device.clone())));

    let device_set = devices.keys().cloned().collect::<BTreeSet<_>>();
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([123u8; 32]);

    let mut run = Run::new(coordinator, devices);

    let keygen_init = run.coordinator.do_keygen(&device_set, threshold).unwrap();
    run.extend(keygen_init);

    let request_nonces = run
        .coordinator
        .maybe_request_nonce_replenishment(device.device_id())
        .unwrap();
    run.extend(std::iter::once(request_nonces));

    run.run_until_finished(&mut env, &mut test_rng);
    let coord_frost_key = run.coordinator.iter_keys().next().unwrap().clone();
    let key_id = coord_frost_key.key_id();

    let uncompleting_sign_task = SignTask::Plain {
        message: b"frostsnap in taiwan".to_vec(),
    };

    let initial_nonces_on_coordinator = run
        .coordinator
        .device_nonces()
        .get(&device.device_id())
        .unwrap()
        .clone();

    let _unused_sign_request = run
        .coordinator
        .start_sign(key_id, uncompleting_sign_task, device_set.clone())
        .unwrap();

    let fewer_nonces_on_coordinator = run
        .coordinator
        .device_nonces()
        .get(&device.device_id())
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
        message: b"rip purple boards rip blue boards rip frostypedeV1".to_vec(),
    };

    let used_sign_request = run
        .coordinator
        .start_sign(key_id, completing_sign_task, device_set)
        .unwrap()
        .clone();

    run.extend(used_sign_request);
    // Test that this run completes without erroring, fully replenishing the nonces
    run.run_until_finished(&mut env, &mut test_rng);

    let final_nonces_on_coordinator = run
        .coordinator
        .device_nonces()
        .get(&device.device_id())
        .unwrap();

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
