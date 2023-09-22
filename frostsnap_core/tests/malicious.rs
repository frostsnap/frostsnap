//! Tests for a malicious actions. A malicious coordinator, a malicious device or both.
use frostsnap_core::message::{CoordinatorToDeviceMessage, KeyGenProvideShares};
use frostsnap_core::FrostSigner;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::frost;
use std::collections::BTreeSet;

/// Models a coordinator maliciously replacing a public polynomial contribution and providing a
/// correct share under that malicious polynomial. The device that has had their share replaced
/// should notice it and abort.
#[test]
fn keygen_maliciously_replace_public_poly() {
    let mut test_rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut device = FrostSigner::new_random(&mut test_rng);
    let devices = BTreeSet::from_iter([device.device_id()]);
    let _ = device
        .recv_coordinator_message(CoordinatorToDeviceMessage::DoKeyGen {
            devices: devices.clone(),
            threshold: 1,
        })
        .unwrap();

    let frost = frost::new_with_deterministic_nonces::<sha2::Sha256>();
    let malicious_poly = frost::generate_scalar_poly(1, &mut rand::thread_rng());
    let provide_shares =
        KeyGenProvideShares::generate(&frost, &malicious_poly, &devices, &mut rand::thread_rng());

    let result = device.recv_coordinator_message(CoordinatorToDeviceMessage::FinishKeyGen {
        shares_provided: FromIterator::from_iter([(device.device_id(), provide_shares)]),
    });
    assert!(matches!(
        result,
        Err(frostsnap_core::SignerError::InvalidMessage { .. })
    ))
}
