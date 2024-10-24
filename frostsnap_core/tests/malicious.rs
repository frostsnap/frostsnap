//! Tests for a malicious actions. A malicious coordinator, a malicious device or both.
use common::{DefaultTestEnv, TEST_ENCRYPTION_KEY};
use frostsnap_core::message::{
    CoordinatorSend, CoordinatorToDeviceMessage, DeviceSend, DeviceToCoordinatorMessage,
    SignRequest,
};
use frostsnap_core::SignTask;
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
        .do_keygen(&device_set, 1, "test".into(), &mut test_rng)
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
fn nonce_reuse() {
    let threshold = 1;
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut run = Run::generate(1, &mut test_rng);
    let device_set = run.device_set();
    // set up nonces for devices first
    for &device_id in &device_set {
        run.extend(run.coordinator.maybe_request_nonce_replenishment(device_id));
    }
    run.run_until_finished(&mut DefaultTestEnv, &mut test_rng)
        .unwrap();

    let keygen_init = run
        .coordinator
        .do_keygen(&device_set, threshold, "my key".to_string(), &mut test_rng)
        .unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut DefaultTestEnv, &mut test_rng)
        .unwrap();
    let key_data = run.coordinator.iter_keys().next().unwrap();
    let task1 = SignTask::Plain {
        message: "utxo.club!".into(),
    };
    let access_structure = key_data.access_structures[0].clone();
    let sign_init = run
        .coordinator
        .start_sign(
            access_structure.access_structure_ref(),
            task1,
            device_set,
            TEST_ENCRYPTION_KEY,
        )
        .unwrap();
    run.extend(sign_init);
    run.run_until_finished(&mut DefaultTestEnv, &mut test_rng)
        .unwrap();

    let sign_req = run
        .transcript
        .iter()
        .find_map(|m| match m {
            Send::CoordinatorToDevice {
                message: CoordinatorToDeviceMessage::RequestSign(sign_req),
                ..
            } => Some(sign_req),
            _ => None,
        })
        .unwrap();

    // Receive a new sign request with the same nonces as the previous session
    let new_sign_request = CoordinatorToDeviceMessage::RequestSign(SignRequest {
        sign_task: SignTask::Plain {
            message: "we lost track of first FROST txn on bitcoin mainnet @ bushbash 2022".into(),
        },
        ..sign_req.clone()
    });
    let sign_request_result = run
        .devices
        .values_mut()
        .next()
        .unwrap()
        .recv_coordinator_message(new_sign_request, &mut test_rng);

    assert!(matches!(
        sign_request_result,
        Err(frostsnap_core::Error::InvalidMessage { .. })
    ));
    assert!(sign_request_result
        .expect_err("should be error")
        .to_string()
        .contains("Attempt to reuse nonces!"))
}
