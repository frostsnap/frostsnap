//! Checks on silent payment sends: the template must spend this wallet's own inputs, and each
//! silent payment address must be the one for the keys it pays.

use bitcoin::{hashes::Hash, Amount, Network, OutPoint, Txid};
use frostsnap_core::{
    bitcoin_transaction::{LocalSpk, PushInput, TransactionTemplate},
    device::KeyPurpose,
    silent_payments::SilentPaymentRecipient,
    tweak::BitcoinBip32Path,
    MasterAppkey, SignTaskError, WireSignTask,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use schnorr_fun::fun::{g, marker::*, Point, Scalar, G};

use frostsnap_core::test::{RunSingleCoordinator as Run, TestEnv};

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

/// A silent payment recipient with keys from `scan` and `spend`, and its address.
fn recipient(scan: u8, spend: u8) -> (SilentPaymentRecipient, String) {
    let recipient = SilentPaymentRecipient {
        scan_key: public(scalar(scan)),
        spend_key: public(scalar(spend)),
    };
    let address = recipient.address(NETWORK).unwrap();
    (recipient, address)
}

/// The master key of a 2-of-3 bitcoin wallet, with nonces ready to sign.
fn master_appkey(seed: u8) -> MasterAppkey {
    let mut rng = ChaCha20Rng::from_seed([seed; 32]);
    let mut env = TestEnv::default();
    let run = Run::start_after_keygen_and_nonces(3, 2, &mut env, &mut rng, 1, purpose());
    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    run.coordinator
        .get_frost_key(access_structure_ref.key_id)
        .unwrap()
        .complete_key
        .master_appkey
}

fn owned_input(template: &mut TransactionTemplate, master_appkey: MasterAppkey, sats: u64) {
    template.push_imaginary_owned_input(
        LocalSpk {
            master_appkey,
            bip32_path: BitcoinBip32Path::external(0),
        },
        Amount::from_sat(sats),
    );
}

#[test]
fn silent_payment_send_refuses_foreign_inputs() {
    let master = master_appkey(15);
    let (recipient, address) = recipient(0x61, 0x62);

    let mut template = TransactionTemplate::new();
    owned_input(&mut template, master, 100_000);
    let foreign = bitcoin::TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: bitcoin::ScriptBuf::from_bytes(
            [&[0x51u8, 0x20][..], &[0xee; 32][..]].concat(),
        ),
    };
    template.push_foreign_input(PushInput::spend_outpoint(
        &foreign,
        OutPoint {
            txid: Txid::from_byte_array([9u8; 32]),
            vout: 0,
        },
    ));
    template.push_silent_payment_output(Amount::from_sat(50_000), recipient, address);

    let result = WireSignTask::BitcoinTransaction(template).check(master, purpose());
    assert!(matches!(result, Err(SignTaskError::SilentPaymentInputs)));
}

#[test]
fn silent_payment_send_refuses_an_address_that_is_not_its_keys() {
    let master = master_appkey(17);
    let (mismatched, _) = recipient(0x71, 0x72);
    let (_, other_address) = recipient(0x73, 0x74);

    let mut template = TransactionTemplate::new();
    owned_input(&mut template, master, 100_000);
    template.push_silent_payment_output(Amount::from_sat(50_000), mismatched, other_address);

    let result = WireSignTask::BitcoinTransaction(template).check(master, purpose());
    assert!(matches!(
        result,
        Err(SignTaskError::SilentPaymentAddressMismatch)
    ));
}

#[test]
fn a_template_with_holes_is_not_signable() {
    let master = master_appkey(19);
    let (recipient, address) = recipient(0x81, 0x82);

    let mut template = TransactionTemplate::new();
    owned_input(&mut template, master, 100_000);
    template.push_silent_payment_output(Amount::from_sat(50_000), recipient, address);

    let result = WireSignTask::BitcoinTransaction(template).check(master, purpose());
    assert!(matches!(
        result,
        Err(SignTaskError::SilentPaymentScriptMissing)
    ));
}
