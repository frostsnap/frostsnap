use frostsnap_core::message::{
    CoordinatorToUserKeyGenMessage, CoordinatorToUserMessage, DeviceToUserMessage, SignTask,
};
use frostsnap_core::{
    CoordinatorState, DeviceId, FrostCoordinator, FrostSigner, KeyId, SessionHash,
};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::{Schnorr, Signature};
use std::collections::{BTreeMap, BTreeSet};

mod common;
use crate::common::Run;

#[derive(Default)]
struct TestEnv {
    // keygen
    pub keygen_checks: BTreeMap<DeviceId, SessionHash>,
    pub received_shares: BTreeSet<DeviceId>,
    pub coordinator_check: Option<SessionHash>,
    pub coordinator_got_keygen_acks: BTreeSet<DeviceId>,
    pub key_id_on_coordinator: Option<KeyId>,

    // signing
    pub sign_tasks: BTreeMap<DeviceId, SignTask>,
    pub signatures: Vec<Signature>,
}

impl common::Env for TestEnv {
    fn user_react_to_coordinator(&mut self, _run: &mut Run, message: CoordinatorToUserMessage) {
        /* nothing to do here -- need keygen ack*/
        match message {
            CoordinatorToUserMessage::KeyGen(keygen_message) => match keygen_message {
                CoordinatorToUserKeyGenMessage::ReceivedShares { id } => {
                    assert!(
                        self.received_shares.insert(id),
                        "should not have already received"
                    )
                }
                CoordinatorToUserKeyGenMessage::CheckKeyGen { session_hash } => {
                    assert!(
                        self.coordinator_check.replace(session_hash).is_none(),
                        "should not have already set this"
                    );
                }
                CoordinatorToUserKeyGenMessage::KeyGenAck { id } => {
                    assert!(
                        self.coordinator_got_keygen_acks.insert(id),
                        "should only receive this once"
                    );
                }
                CoordinatorToUserKeyGenMessage::FinishedKey { key_id } => {
                    assert!(
                        self.key_id_on_coordinator.replace(key_id).is_none(),
                        "should only receive this once"
                    );
                }
            },
            CoordinatorToUserMessage::Signed { signatures } => {
                self.signatures = signatures;
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
            DeviceToUserMessage::SignatureRequest { sign_task } => {
                self.sign_tasks.insert(from, sign_task);
                let sign_ack = run.device(from).sign_ack().unwrap();
                run.extend_from_device(from, sign_ack);
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

    let device_set = devices.clone().into_keys().collect::<BTreeSet<_>>();
    let device_list = devices.clone().into_keys().collect::<Vec<_>>();

    let mut run = Run::new(coordinator, devices);

    let keygen_init = vec![run.coordinator.do_keygen(&device_set, threshold).unwrap()];
    run.extend(keygen_init);

    let mut env = TestEnv::default();
    run.run_until_finished(&mut env);
    assert!(matches!(
        run.coordinator.state(),
        CoordinatorState::FrostKey { .. }
    ));
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
    assert_eq!(env.received_shares, device_set);
    let public_key = run
        .coordinator
        .frost_key_state()
        .unwrap()
        .frost_key()
        .public_key();

    for (message, signers) in &[
        (b"johnmcafee47".as_slice(), [0, 1]),
        (b"pyramid schmee".as_slice(), [1, 2]),
    ] {
        env.signatures.clear();
        env.sign_tasks.clear();
        let task = SignTask::Plain(message.to_vec());
        let set = BTreeSet::from_iter(signers.iter().map(|i| device_list[*i]));

        let sign_init = run
            .coordinator
            .start_sign(task.clone(), set.clone())
            .unwrap();
        run.extend(sign_init);
        run.run_until_finished(&mut env);
        assert!(matches!(
            run.coordinator.state(),
            CoordinatorState::FrostKey { .. }
        ));
        assert_eq!(env.sign_tasks.keys().cloned().collect::<BTreeSet<_>>(), set);
        assert!(env.sign_tasks.values().all(|v| *v == task));

        assert!(task.verify(&schnorr, public_key, &env.signatures));
    }
}
