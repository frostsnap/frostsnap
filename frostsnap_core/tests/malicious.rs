//! Tests for a malicious actions. A malicious coordinator, a malicious device or both.
use frostsnap_core::message::{
    CoordinatorToDeviceMessage, DeviceToUserMessage, KeyGenResponse, SignRequest, SignTask,
};
use frostsnap_core::{DeviceId, FrostCoordinator, FrostSigner};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::frost;
use schnorr_fun::fun::Scalar;
use std::collections::{BTreeMap, BTreeSet};

use crate::common::{Env, Run, Send};
mod common;

/// Models a coordinator maliciously replacing a public polynomial contribution and providing a
/// correct share under that malicious polynomial. The device that has had their share replaced
/// should notice it and abort.
#[test]
fn keygen_maliciously_replace_public_poly() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut device = FrostSigner::new_random(&mut test_rng);
    let devices = BTreeSet::from_iter([device.device_id()]);
    let device_to_share_index: BTreeMap<_, _> = devices
        .into_iter()
        .enumerate()
        .map(|(index, id)| (id, Scalar::from((index + 1) as u32).non_zero().unwrap()))
        .collect();
    let _ = device
        .recv_coordinator_message(
            CoordinatorToDeviceMessage::DoKeyGen {
                device_to_share_index: device_to_share_index.clone(),
                threshold: 1,
            },
            &mut test_rng,
        )
        .unwrap();

    let frost = frost::new_with_deterministic_nonces::<sha2::Sha256>();
    let malicious_poly = frost::generate_scalar_poly(1, &mut rand::thread_rng());
    let provide_shares = KeyGenResponse::generate(
        &frost,
        &malicious_poly,
        &device_to_share_index,
        &mut rand::thread_rng(),
    );

    let result = device.recv_coordinator_message(
        CoordinatorToDeviceMessage::FinishKeyGen {
            shares_provided: FromIterator::from_iter([(device.device_id(), provide_shares)]),
        },
        &mut test_rng,
    );
    assert!(matches!(
        result,
        Err(frostsnap_core::Error::InvalidMessage { .. })
    ))
}

/// Send different signing requests with the same nonces twice.
/// The device should reject signing the second request.
#[test]
fn nonce_reuse() {
    // just does enough to make progress
    struct TestEnv;
    impl Env for TestEnv {
        fn user_react_to_device(
            &mut self,
            run: &mut Run,
            from: DeviceId,
            message: DeviceToUserMessage,
        ) {
            match message {
                DeviceToUserMessage::CheckKeyGen { .. } => {
                    let ack = run.device(from).keygen_ack().unwrap();
                    run.extend_from_device(from, ack);
                }
                DeviceToUserMessage::SignatureRequest { .. } => {
                    let sign_ack = run.device(from).sign_ack().unwrap();
                    run.extend_from_device(from, sign_ack);
                }
                DeviceToUserMessage::Canceled { .. } => {
                    panic!("no cancelling done");
                }
            }
        }
    }
    let threshold = 1;
    let coordinator = FrostCoordinator::new();
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);

    let device = FrostSigner::new_random(&mut test_rng);
    let device_id = device.device_id();
    let devices = FromIterator::from_iter([(device_id, device)]);
    let device_set = BTreeSet::from_iter([device_id]);
    let mut run = Run::new(coordinator, devices);

    // set up nonces for devices first
    for &device_id in &device_set {
        run.extend(run.coordinator.maybe_request_nonce_replenishment(device_id));
    }
    run.run_until_finished(&mut TestEnv, &mut test_rng);

    let keygen_init = run.coordinator.do_keygen(&device_set, threshold).unwrap();
    run.extend(keygen_init);

    run.run_until_finished(&mut TestEnv, &mut test_rng);
    let key_id = run.coordinator.iter_keys().next().unwrap().key_id();
    let task1 = SignTask::Plain {
        message: b"utxo.club!".to_vec(),
    };
    let sign_init = run
        .coordinator
        .start_sign(key_id, task1, device_set)
        .unwrap();
    run.extend(sign_init);
    run.run_until_finished(&mut TestEnv, &mut test_rng);

    let nonces = run
        .transcript
        .iter()
        .find_map(|m| match m {
            Send::CoordinatorToDevice {
                message: CoordinatorToDeviceMessage::RequestSign(SignRequest { nonces, .. }),
                ..
            } => Some(nonces),
            _ => None,
        })
        .unwrap();

    // Receive a new sign request with the same nonces as the previous session
    let new_sign_request = CoordinatorToDeviceMessage::RequestSign(SignRequest {
        nonces: nonces.clone(),
        key_id,
        sign_task: SignTask::Plain {
            message: b"we lost track of first FROST txn on bitcoin mainnet @ bushbash 2022"
                .to_vec(),
        },
    });
    let sign_request_result = run
        .device(device_id)
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
