use frostsnap_core::coordinator::signing::ParticipantSignatureShares;
use frostsnap_core::coordinator::signing::RemoteSignSessionId;
use frostsnap_core::coordinator::{CoordinatorToUserMessage, CoordinatorToUserSigningMessage};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::test::{Env, Run, RunSingleCoordinator, TestEnv, TEST_ENCRYPTION_KEY};
use frostsnap_core::{SignSessionId, WireSignTask};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::Schnorr;
use std::collections::BTreeMap;

#[derive(Default)]
struct RemoteSigningEnv {
    shares: BTreeMap<(usize, SignSessionId), Vec<ParticipantSignatureShares>>,
}

impl Env for RemoteSigningEnv {
    fn user_react_to_coordinator(
        &mut self,
        run: &mut Run,
        ci: usize,
        message: CoordinatorToUserMessage,
        rng: &mut impl rand_chacha::rand_core::RngCore,
    ) {
        match message {
            CoordinatorToUserMessage::KeyGen {
                keygen_id,
                inner:
                    frostsnap_core::coordinator::CoordinatorToUserKeyGenMessage::KeyGenAck {
                        all_acks_received: true,
                        ..
                    },
            } => {
                let sends = run.participants[ci]
                    .coordinator
                    .finalize_keygen(keygen_id, TEST_ENCRYPTION_KEY, rng)
                    .unwrap();
                run.extend_from_coordinator(ci, sends);
            }
            CoordinatorToUserMessage::Signing(CoordinatorToUserSigningMessage::GotShare {
                session_id,
                shares,
                ..
            }) => {
                self.shares
                    .entry((ci, session_id))
                    .or_default()
                    .push(shares);
            }
            _ => {}
        }
    }
}

#[test]
fn two_coordinator_remote_signing() {
    let mut rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = RemoteSigningEnv::default();
    let mut run = Run::start_after_remote_keygen(
        &[1, 1],
        2,
        "remote sign key",
        KeyPurpose::Test,
        &mut env,
        &mut rng,
    );
    run.replenish_all_nonces(2, &mut env, &mut rng);

    let nonce_reservation_id = RemoteSignSessionId::new([0u8; 32]);

    let access_structure_ref = run.participants[0]
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    let sign_task = WireSignTask::Test {
        message: "remote signing test".into(),
    };

    let mut all_binonces = vec![];
    for i in 0..2 {
        let device_id = *run.participants[i].devices.keys().next().unwrap();
        let offer = run.participants[i]
            .coordinator
            .offer_to_sign(
                nonce_reservation_id,
                access_structure_ref,
                sign_task.clone(),
                device_id,
            )
            .unwrap();
        all_binonces.push(offer.participant_binonces.clone());
    }

    for i in 0..2 {
        let device_id = *run.participants[i].devices.keys().next().unwrap();
        let sign_req = run.participants[i]
            .coordinator
            .sign_with_nonce_reservation(
                nonce_reservation_id,
                device_id,
                &all_binonces,
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        run.extend_from_coordinator(i, sign_req);
    }

    run.run_until_finished(&mut env, &mut rng).unwrap();

    let session_id = frostsnap_core::message::GroupSignReq::from_binonces(
        sign_task.clone(),
        access_structure_ref.access_structure_id,
        &all_binonces,
    )
    .session_id();

    // Each coordinator emitted GotShare → env collected shares from each
    let shares_0 = &env.shares[&(0, session_id)];
    let shares_1 = &env.shares[&(1, session_id)];
    assert_eq!(shares_0.len(), 1);
    assert_eq!(shares_1.len(), 1);

    let all_shares = vec![shares_0[0].clone(), shares_1[0].clone()];

    let key_data = run.participants[0].coordinator.iter_keys().next().unwrap();
    let access_structure = key_data
        .get_access_structure(access_structure_ref.access_structure_id)
        .unwrap();
    let signing_key = frostsnap_core::coordinator::KeyContext {
        app_shared_key: access_structure.app_shared_key(),
        purpose: KeyPurpose::Test,
    };

    for shares in &all_shares {
        assert!(
            frostsnap_core::coordinator::remote_signing::verify_signature_shares(
                &sign_task,
                &signing_key,
                &all_binonces,
                shares,
            ),
            "each participant's shares should verify independently"
        );
    }

    let all_shares_refs: Vec<_> = all_shares.iter().collect();
    let signatures = frostsnap_core::coordinator::remote_signing::combine_signatures(
        sign_task.clone(),
        &signing_key,
        &all_binonces,
        &all_shares_refs,
    )
    .unwrap();

    let schnorr = Schnorr::<sha2::Sha256>::verify_only();
    let checked_task = sign_task
        .check(key_data.complete_key.master_appkey, KeyPurpose::Test)
        .unwrap();
    let decoded: Vec<_> = signatures
        .iter()
        .map(|s| (*s).into_decoded().unwrap())
        .collect();
    assert!(checked_task.verify_final_signatures(&schnorr, &decoded));
}

#[test]
fn cancel_remote_sign_session_reuses_nonces() {
    let mut env = TestEnv::default();
    let mut rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut run = RunSingleCoordinator::start_after_keygen_and_nonces(
        3,
        2,
        &mut env,
        &mut rng,
        2,
        KeyPurpose::Test,
    );

    let device_id = *run.devices.keys().next().unwrap();
    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    let sign_task = WireSignTask::Test {
        message: "reuse nonces test".into(),
    };
    let id1 = RemoteSignSessionId::new([1u8; 32]);
    let offer1 = run
        .coordinator
        .offer_to_sign(id1, access_structure_ref, sign_task.clone(), device_id)
        .unwrap();
    let binonces_1 = offer1.participant_binonces.binonces.clone();

    run.coordinator.cancel_remote_sign_session(id1);

    let id2 = RemoteSignSessionId::new([2u8; 32]);
    let offer2 = run
        .coordinator
        .offer_to_sign(id2, access_structure_ref, sign_task, device_id)
        .unwrap();
    let binonces_2 = offer2.participant_binonces.binonces.clone();

    assert_eq!(binonces_1, binonces_2, "cancelled nonces should be reused");
}
