//! Tests for a malicious actions. A malicious coordinator, a malicious device or both.
use common::{DefaultTestEnv, TEST_ENCRYPTION_KEY};
use frostsnap_core::coordinator::CoordinatorSend;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::{
    CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage, DoKeyGen,
};
use frostsnap_core::WireSignTask;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::common::{Run, Send};
mod common;

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

    let keygen_init = run
        .coordinator
        .do_keygen(
            DoKeyGen::new(
                device_set,
                1,
                "test".into(),
                KeyPurpose::Test,
                &mut test_rng,
            ),
            &mut test_rng,
        )
        .unwrap();
    let do_keygen = keygen_init
        .clone()
        .into_iter()
        .find_map(|msg| match msg {
            CoordinatorSend::ToDevice {
                message: dokeygen @ CoordinatorToDeviceMessage::DoKeyGen { .. },
                ..
            } => Some(dokeygen),
            _ => None,
        })
        .unwrap();

    run.extend(keygen_init.clone());

    let result = run.run_until(&mut DefaultTestEnv, &mut test_rng, move |run| {
        for send in run.message_queue.iter_mut() {
            if let Send::DeviceToCoordinator {
                from: _,
                message: DeviceToCoordinatorMessage::KeyGenResponse(input),
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
                            DeviceToCoordinatorMessage::KeyGenResponse(response) => Some(response),
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
fn send_sign_req_with_same_nonces_but_different_message() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut run = Run::start_after_keygen_and_nonces(
        1,
        1,
        &mut DefaultTestEnv,
        &mut test_rng,
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
    run.run_until_finished(&mut DefaultTestEnv, &mut test_rng)
        .unwrap();

    let mut sign_req = sign_req.unwrap();
    sign_req.request_sign.group_sign_req.sign_task = WireSignTask::Test {
        message: "we lost track of first FROST txn on bitcoin mainnet @ bushbash 2022".into(),
    };

    run.extend(sign_req);
    let sign_request_result = run.run_until_finished(&mut DefaultTestEnv, &mut test_rng);

    assert!(matches!(
        sign_request_result,
        Err(frostsnap_core::Error::InvalidMessage { .. })
    ));

    assert!(sign_request_result
        .expect_err("should be error")
        .to_string()
        .contains("Attempt to reuse nonces!"))
}
