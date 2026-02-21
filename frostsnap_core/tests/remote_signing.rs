use common::TEST_ENCRYPTION_KEY;
use frostsnap_core::coordinator::ParticipantBinonces;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::GroupSignReq;
use frostsnap_core::WireSignTask;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::Schnorr;

mod common;
mod env;
use crate::common::Run;
use crate::env::TestEnv;

/// Tests the nonce reservation signing API with two independent coordinators.
///
/// This simulates a remote signing scenario where multiple parties coordinate signing without
/// being connected to the same coordinator. Two coordinators share the same FROST key but
/// each manages different local devices.
///
/// The test exercises the full nonce reservation flow:
/// - Each coordinator reserves nonces for its local device
/// - Raw binonces are wrapped with share indices and exchanged (simulating Nostr)
/// - Both call `sign_with_nonce_reservation` which creates the session and signs in one step
/// - Signature shares are exchanged
/// - Both coordinators complete the session and produce identical valid signatures
#[test]
fn test_nonce_reservation_signing_two_coordinators() {
    let n_parties = 3;
    let threshold = 2;
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut run1 = Run::start_after_keygen(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        KeyPurpose::Test,
    );

    // Clone BEFORE nonce sync - both have same key, different local devices
    let mut run2 = run1.clone();

    let device_ids: Vec<_> = run1.devices.keys().copied().collect();
    run1.devices.retain(|id, _| *id == device_ids[0]);
    run2.devices.retain(|id, _| *id == device_ids[1]);
    run1.start_devices.retain(|id, _| *id == device_ids[0]);
    run2.start_devices.retain(|id, _| *id == device_ids[1]);

    for run in [&mut run1, &mut run2] {
        run.extend(run.coordinator.maybe_request_nonce_replenishment(
            &run.device_set(),
            2,
            &mut test_rng,
        ));
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }

    let access_structure_ref = run1
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    let device_share_indices = run1
        .coordinator
        .get_access_structure(access_structure_ref)
        .unwrap()
        .device_to_share_indicies();

    let sign_task = WireSignTask::Test {
        message: "nonce reservation test".into(),
    };

    // Each coordinator reserves nonces for its local device and wraps with share_index
    let binonces: Vec<_> = [(&mut run1, device_ids[0]), (&mut run2, device_ids[1])]
        .into_iter()
        .map(|(run, device_id)| {
            let raw_binonces = run.coordinator.reserve_nonces(device_id, 1).unwrap();
            let share_index = *device_share_indices.get(&device_id).unwrap();
            ParticipantBinonces {
                share_index,
                binonces: raw_binonces,
            }
        })
        .collect();

    for run in [&mut run1, &mut run2] {
        run.check_mutations();
    }

    let all_binonces = vec![binonces[0].clone(), binonces[1].clone()];

    // can_sign is true for devices that reserved nonces, false otherwise
    assert!(run1.coordinator.can_sign_with_nonce_reservation(
        &sign_task,
        access_structure_ref,
        &all_binonces,
        device_ids[0],
    ));
    assert!(!run1.coordinator.can_sign_with_nonce_reservation(
        &sign_task,
        access_structure_ref,
        &all_binonces,
        device_ids[2],
    ));

    // Signing with a device that didn't reserve nonces should fail
    assert!(matches!(
        run1.coordinator.sign_with_nonce_reservation(
            sign_task.clone(),
            access_structure_ref,
            &all_binonces,
            device_ids[2],
            TEST_ENCRYPTION_KEY,
        ),
        Err(frostsnap_core::coordinator::StartSignError::DeviceNotLocalSigner { .. })
    ));

    assert_eq!(
        run1.coordinator.active_signing_sessions().count(),
        0,
        "failed signing attempt didn't mutate"
    );

    // Both sign with nonce reservation — creates session + requests device sign in one step
    for (run, device_id) in [(&mut run1, device_ids[0]), (&mut run2, device_ids[1])] {
        let sign_req = run
            .coordinator
            .sign_with_nonce_reservation(
                sign_task.clone(),
                access_structure_ref,
                &all_binonces,
                device_id,
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();

        assert!(
            run.coordinator.can_sign_with_nonce_reservation(
                &sign_task,
                access_structure_ref,
                &all_binonces,
                device_id,
            ),
            "can still sign even after doing it the first time so"
        );

        let _sign_req = run
            .coordinator
            .sign_with_nonce_reservation(
                sign_task.clone(),
                access_structure_ref,
                &all_binonces,
                device_id,
                TEST_ENCRYPTION_KEY,
            )
            .expect("calls should be idempotent");

        run.extend(sign_req);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }

    // Derive session_id deterministically for share exchange
    let session_id = GroupSignReq::from_binonces(
        sign_task.clone(),
        access_structure_ref.access_structure_id,
        &all_binonces,
    )
    .session_id();

    // Exchange signature shares between coordinators
    let all_shares: Vec<_> = [&run1, &run2]
        .into_iter()
        .flat_map(|run| run.coordinator.get_signature_shares(session_id).unwrap())
        .collect();

    for run in [&mut run1, &mut run2] {
        for shares in &all_shares {
            run.coordinator
                .add_remote_signature_shares(session_id, shares.clone())
                .unwrap();
        }
    }

    // Both coordinators produce the same signatures
    let signatures: Vec<_> = [&mut run1, &mut run2]
        .into_iter()
        .map(|run| run.coordinator.complete_sign_session(session_id).unwrap())
        .collect();

    assert_eq!(
        signatures[0], signatures[1],
        "both coordinators should produce the same signatures"
    );

    let schnorr = Schnorr::<sha2::Sha256>::verify_only();
    let key_data = run1.coordinator.iter_keys().next().unwrap();
    let checked_task = sign_task
        .check(key_data.complete_key.master_appkey, KeyPurpose::Test)
        .unwrap();
    assert!(checked_task.verify_final_signatures(
        &schnorr,
        &signatures[0]
            .iter()
            .map(|s| (*s).into_decoded().unwrap())
            .collect::<Vec<_>>()
    ));
}

#[test]
fn test_cancel_nonce_reservation_reuses_nonces() {
    let n_parties = 3;
    let threshold = 2;
    let mut env = TestEnv::default();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut run = Run::start_after_keygen_and_nonces(
        n_parties,
        threshold,
        &mut env,
        &mut test_rng,
        2,
        KeyPurpose::Test,
    );

    let device_id = *run.devices.keys().next().unwrap();

    let binonces_1 = run.coordinator.reserve_nonces(device_id, 1).unwrap();

    run.coordinator.cancel_nonce_reservation(&binonces_1);

    let binonces_2 = run.coordinator.reserve_nonces(device_id, 1).unwrap();
    assert_eq!(binonces_1, binonces_2, "cancelled nonces should be reused");
}
