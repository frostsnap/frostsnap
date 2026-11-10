//! Trusted silent payment contacts: adding one with a threshold of devices.

use bitcoin::Network;
use frostsnap_core::{
    coordinator::StartSignError,
    device::KeyPurpose,
    silent_payments::{verify_contact_shares, SilentPaymentRecipient, SpContactDetails},
    AccessStructureRef, DeviceId, SignTaskError,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use schnorr_fun::fun::{g, marker::*, Point, Scalar, G};
use std::collections::BTreeSet;

mod common;
mod env;
use crate::{
    common::{Run, TEST_ENCRYPTION_KEY},
    env::TestEnv,
};

const NETWORK: Network = Network::Bitcoin;

fn purpose() -> KeyPurpose {
    KeyPurpose::Bitcoin(NETWORK)
}

fn scalar(byte: u8) -> Scalar {
    Scalar::<Secret, Zero>::from_bytes_mod_order([byte; 32])
        .non_zero()
        .unwrap()
}

fn public(secret: Scalar) -> Point {
    g!(secret * G).normalize().non_zero().unwrap()
}

fn details(name: &str, scan: u8, spend: u8) -> SpContactDetails {
    SpContactDetails {
        name: name.into(),
        recipient: SilentPaymentRecipient {
            scan_key: public(scalar(scan)),
            spend_key: public(scalar(spend)),
        },
    }
}

/// A 2-of-3 bitcoin key, ready to sign.
struct Wallet {
    run: Run,
    env: TestEnv,
    rng: ChaCha20Rng,
    access_structure_ref: AccessStructureRef,
    devices: Vec<DeviceId>,
}

impl Wallet {
    fn new() -> Self {
        let mut env = TestEnv::default();
        let mut rng = ChaCha20Rng::from_seed([42u8; 32]);
        let run = Run::start_after_keygen_and_nonces(3, 2, &mut env, &mut rng, 1, purpose());
        let key = run.coordinator.iter_keys().next().unwrap().clone();
        let access_structure_ref = key
            .access_structures()
            .next()
            .unwrap()
            .access_structure_ref();
        let devices = run.device_vec();
        Self {
            run,
            env,
            rng,
            access_structure_ref,
            devices,
        }
    }

    fn signers(&self, indices: &[usize]) -> BTreeSet<DeviceId> {
        indices.iter().map(|i| self.devices[*i]).collect()
    }

    /// Runs the ECDH round of adding `details` on `signers`, stopping before anything is signed.
    fn contact_ecdh(
        &mut self,
        details: SpContactDetails,
        signers: &[usize],
    ) -> frostsnap_core::message::signing::SpContactSessionId {
        let signers = self.signers(signers);
        let (session_id, sends) = self
            .run
            .coordinator
            .start_sp_contact(
                self.access_structure_ref,
                details,
                &signers,
                TEST_ENCRYPTION_KEY,
                &mut self.rng,
            )
            .unwrap();
        self.run.extend(sends);
        self.run
            .run_until_finished(&mut self.env, &mut self.rng)
            .unwrap();
        session_id
    }
}

/// A proof only counts for the record its device confirmed, so devices shown different records
/// can't be combined. The scan key is the same here, as a swapped spend key would leave it.
#[test]
fn a_proof_does_not_carry_over_to_other_details() {
    let mut wallet = Wallet::new();
    let confirmed = details("Lee", 0xb1, 0xb2);
    let session_id = wallet.contact_ecdh(confirmed.clone(), &[0, 1]);
    let session = wallet
        .run
        .coordinator
        .sp_contact_session(session_id)
        .unwrap()
        .clone();
    let shares = session.shares.values().copied().collect::<Vec<_>>();
    let key = wallet
        .run
        .coordinator
        .get_access_structure(wallet.access_structure_ref)
        .unwrap()
        .app_shared_key();

    assert!(verify_contact_shares(&key, &confirmed, NETWORK, &shares).is_ok());

    let mut other_spend_key = confirmed.clone();
    other_spend_key.recipient.spend_key = public(scalar(0xb3));
    assert!(verify_contact_shares(&key, &other_spend_key, NETWORK, &shares).is_err());

    let mut other_name = confirmed.clone();
    other_name.name = "Mallory".into();
    assert!(verify_contact_shares(&key, &other_name, NETWORK, &shares).is_err());

    assert!(verify_contact_shares(&key, &confirmed, Network::Testnet, &shares).is_err());
}

/// A device that holds more than one share of an access structure must answer with the one the
/// coordinator expects from it.
#[test]
fn a_device_answers_with_the_share_the_request_names() {
    use frostsnap_core::{
        coordinator::CoordinatorSend,
        message::{signing::CoordinatorSigning, CoordinatorToDeviceMessage},
    };
    let mut wallet = Wallet::new();
    let signers = wallet.signers(&[0, 1]);
    let (_, sends) = wallet
        .run
        .coordinator
        .start_sp_contact(
            wallet.access_structure_ref,
            details("Pia", 0xe1, 0xe2),
            &signers,
            TEST_ENCRYPTION_KEY,
            &mut wallet.rng,
        )
        .unwrap();
    let request_for = |device: DeviceId| {
        sends
            .iter()
            .find_map(|send| match send {
                CoordinatorSend::ToDevice {
                    message:
                        CoordinatorToDeviceMessage::Signing(
                            CoordinatorSigning::RequestSpContactEcdh(request),
                        ),
                    destinations,
                } if destinations.contains(&device) => Some((**request).clone()),
                _ => None,
            })
            .unwrap()
    };
    let mut request = request_for(wallet.devices[0]);
    let other_index = request_for(wallet.devices[1]).share_index;
    assert_ne!(request.share_index, other_index);

    let mut device_accepts = |request| {
        let id = wallet.devices[0];
        wallet
            .run
            .devices
            .get_mut(&id)
            .unwrap()
            .recv_coordinator_message(
                CoordinatorToDeviceMessage::Signing(CoordinatorSigning::RequestSpContactEcdh(
                    Box::new(request),
                )),
                &mut wallet.rng,
            )
            .is_ok()
    };
    assert!(device_accepts(request.clone()));
    request.share_index = other_index;
    assert!(!device_accepts(request));
}

#[test]
fn contact_names_are_short_printable_ascii() {
    let check = |name: &str| details(name, 1, 2).has_valid_name();
    assert!(check("Alice @ work"));
    assert!(check(&"x".repeat(20)));
    assert!(!check(""));
    assert!(!check(&"x".repeat(21)));
    assert!(!check("Al\u{0456}ce"));
    assert!(!check("Alice\n"));

    let mut wallet = Wallet::new();
    let signers = wallet.signers(&[0, 1]);
    assert!(matches!(
        wallet.run.coordinator.start_sp_contact(
            wallet.access_structure_ref,
            details("", 1, 2),
            &signers,
            TEST_ENCRYPTION_KEY,
            &mut wallet.rng,
        ),
        Err(StartSignError::SignTask(
            SignTaskError::SilentPaymentInvalidContactName
        ))
    ));
}
