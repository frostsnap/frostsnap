//! Tests for a malicious actions. A malicious coordinator, a malicious device or both.
use common::TEST_ENCRYPTION_KEY;
use env::TestEnv;
use frostsnap_core::coordinator::{BeginKeygen, CoordinatorSend};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::{
    keygen::DeviceKeygen, CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage,
    Keygen,
};
use frostsnap_core::WireSignTask;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::common::{Run, Send};
mod common;
mod env;

/// Models a coordinator maliciously replacing a public polynomial contribution and providing a
/// correct share under that malicious polynomial. The device that has had their share replaced
/// should notice it and abort.
#[test]
fn keygen_maliciously_replace_public_poly() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut other_rng = ChaCha20Rng::from_seed([21u8; 32]);
    let mut run = Run::generate(1, &mut test_rng);
    let device_set = run.device_set();
    let mut shadow_device = run.devices.values().next().unwrap().clone();

    use rand_chacha::rand_core::{RngCore, SeedableRng};
    let mut seed = [0u8; 32];
    test_rng.fill_bytes(&mut seed);
    let mut coordinator_rng = ChaCha20Rng::from_seed(seed);

    let keygen_init = run
        .coordinator
        .begin_keygen(
            BeginKeygen::new(
                device_set.into_iter().collect(),
                1,
                "test".into(),
                KeyPurpose::Test,
                &mut test_rng,
            ),
            &mut coordinator_rng,
        )
        .unwrap();
    let do_keygen = keygen_init
        .clone()
        .into_iter()
        .find_map(|msg| match msg {
            CoordinatorSend::ToDevice {
                message: dokeygen @ CoordinatorToDeviceMessage::KeyGen(Keygen::Begin(_)),
                ..
            } => Some(dokeygen),
            _ => None,
        })
        .unwrap();

    run.extend(keygen_init.clone());

    let result = run.run_until(&mut TestEnv::default(), &mut test_rng, move |run| {
        for send in run.message_queue.iter_mut() {
            if let Send::DeviceToCoordinator {
                from: _,
                message: DeviceToCoordinatorMessage::KeyGen(DeviceKeygen::Response(input)),
            } = send
            {
                // A "man in the middle" replace the polynomial the coordinator actually
                // receives with a different one generated with different randomness. This should
                // cause the device to detect the switch and abort.
                let malicious_messages = shadow_device
                    .recv_coordinator_message(do_keygen.clone(), &mut other_rng)
                    .unwrap();
                let malicious_keygen_response = malicious_messages
                    .into_iter()
                    .find_map(|send| match send {
                        DeviceSend::ToCoordinator(boxed) => match *boxed {
                            DeviceToCoordinatorMessage::KeyGen(DeviceKeygen::Response(
                                response,
                            )) => Some(response),
                            _ => None,
                        },
                        _ => None,
                    })
                    .unwrap();
                *input = malicious_keygen_response;
            }
        }
        run.message_queue.is_empty()
    });

    assert!(result.is_err());
}

/// Send different signing requests with the same nonces twice.
/// The device should reject signing the second request.
#[test]
#[should_panic(expected = "Attempt to reuse nonces")]
fn send_sign_req_with_same_nonces_but_different_message() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut run = Run::start_after_keygen_and_nonces(
        1,
        1,
        &mut TestEnv::default(),
        &mut test_rng,
        1,
        KeyPurpose::Test,
    );
    let device_set = run.device_set();
    let key_data = run.coordinator.iter_keys().next().unwrap();
    let task1 = WireSignTask::Test {
        message: "utxo.club!".into(),
    };
    let access_structure_ref = key_data
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    let session_id = run
        .coordinator
        .start_sign(access_structure_ref, task1, &device_set, &mut test_rng)
        .unwrap();

    let mut sign_req = None;
    for device_id in &device_set {
        sign_req = Some(run.coordinator.request_device_sign(
            session_id,
            *device_id,
            TEST_ENCRYPTION_KEY,
        ));
        run.extend(sign_req.clone().unwrap());
    }
    run.run_until_finished(&mut TestEnv::default(), &mut test_rng)
        .unwrap();

    let mut sign_req = sign_req.unwrap();
    sign_req.request_sign.group_sign_req.sign_task = WireSignTask::Test {
        message: "we lost track of first FROST txn on bitcoin mainnet @ bushbash 2022".into(),
    };

    run.extend(sign_req);
    // This will panic when sign_ack tries to reuse nonces
    run.run_until_finished(&mut TestEnv::default(), &mut test_rng)
        .unwrap();
}

/// Test that a malicious coordinator providing wrong root_shared_key during consolidation
/// is detected by the polynomial checksum validation
#[test]
fn malicious_consolidation_wrong_root_shared_key() {
    use schnorr_fun::fun::{poly, prelude::*};

    let mut test_rng = ChaCha20Rng::from_seed([44u8; 32]);
    let mut env = TestEnv::default();

    // Do a 3-of-3 keygen to have enough degrees of freedom for the attack
    let mut run =
        Run::start_after_keygen_and_nonces(3, 3, &mut env, &mut test_rng, 3, KeyPurpose::Test);

    let device_set = run.device_set();
    let key_data = run.coordinator.iter_keys().next().unwrap();
    let access_structure_ref = key_data
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    // Display backups for all devices
    for &device_id in &device_set {
        let display_backup = run
            .coordinator
            .request_device_display_backup(device_id, access_structure_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(display_backup);
    }
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // We should have backups in the env
    assert_eq!(env.backups.len(), 3);

    // Pick the first device
    let device_id = *env.backups.keys().next().unwrap();
    let (_, backup) = env.backups.get(&device_id).unwrap().clone();

    // Assign the backup for this device to enter
    env.backup_to_enter.insert(device_id, backup);

    // Tell the device to enter backup mode
    let enter_physical_id = frostsnap_core::EnterPhysicalId::new(&mut test_rng);
    let enter_backup = run
        .coordinator
        .tell_device_to_load_physical_backup(enter_physical_id, device_id);
    run.extend(enter_backup);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Get the PhysicalBackupPhase from the env that was populated when the device responded
    let physical_backup_phase = *env
        .physical_backups_entered
        .last()
        .expect("Should have a physical backup phase");

    // Get the proper consolidation message from the coordinator
    let mut consolidate_message = run
        .coordinator
        .tell_device_to_consolidate_physical_backup(
            physical_backup_phase,
            access_structure_ref,
            TEST_ENCRYPTION_KEY,
        )
        .unwrap();

    // Create a malicious root_shared_key that:
    // 1. Has the same public key (first coefficient)
    // 2. Evaluates to the correct share for this device
    // 3. But is a different polynomial overall
    // This is the most pernicious attack - everything the device can verify looks correct!

    let device_share_image = physical_backup_phase.backup.share_image;

    // Create a malicious polynomial by interpolating through:
    // - The device's correct share point
    // - One other correct point from the polynomial
    // - One malicious point that's NOT on the correct polynomial

    let other_index = if consolidate_message.share_index == s!(2).public() {
        s!(3).public()
    } else {
        s!(2).public()
    };

    let malicious_share_images = [
        device_share_image,
        // Use a point that's on the correct polynomial
        consolidate_message.root_shared_key.share_image(other_index),
        // Create a malicious point that's NOT on the correct polynomial
        schnorr_fun::frost::ShareImage {
            index: s!(99).public(),
            image: g!(77 * G).normalize().mark_zero(),
        },
    ];

    // Interpolate to get the malicious polynomial
    let malicious_shares: Vec<(schnorr_fun::frost::ShareIndex, Point<Normal, Public, Zero>)> =
        malicious_share_images
            .iter()
            .map(|img| (img.index, img.image))
            .collect();
    let malicious_interpolated = poly::point::interpolate(&malicious_shares);
    let malicious_poly_points: Vec<Point<Normal, Public, Zero>> =
        poly::point::normalize(malicious_interpolated).collect();

    let malicious_shared_key =
        schnorr_fun::frost::SharedKey::from_poly(malicious_poly_points.clone());

    // Verify it evaluates to the correct share for this device
    assert_eq!(
        malicious_shared_key.share_image(consolidate_message.share_index),
        device_share_image,
        "Malicious key should evaluate to correct share for device"
    );

    // But it's a different polynomial overall
    assert_ne!(
        malicious_shared_key.point_polynomial(),
        consolidate_message.root_shared_key.point_polynomial(),
        "Malicious key should be a different polynomial"
    );

    // Mutate the consolidation message to have the malicious root_shared_key
    consolidate_message.root_shared_key =
        malicious_shared_key.non_zero().expect("Should be non-zero");

    // Send the malicious consolidation to the device
    run.extend(consolidate_message);
    let result = run.run_until_finished(&mut env, &mut test_rng);

    // The device should reject this because the polynomial checksum doesn't match
    assert!(
        result.is_err(),
        "Device should reject consolidation with wrong root_shared_key"
    );
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("polynomial checksum validation failed"),
        "Should fail to consolidate due to checksum mismatch, got: {}",
        error_msg
    );
}
