//! Silent payment sends from a threshold wallet: the checks a template must pass, the ECDH round
//! that derives the silent payment scripts, and the rejections of a tampered ECDH request.

use bitcoin::{hashes::Hash, Amount, Network, OutPoint, Txid};
use frostsnap_core::{
    bitcoin_transaction::{LocalSpk, PushInput, TransactionTemplate},
    device::KeyPurpose,
    message::{
        signing::{CoordinatorSigning, DeviceSigning},
        CoordinatorToDeviceMessage, DeviceToCoordinatorMessage,
    },
    silent_payments::SilentPaymentRecipient,
    tweak::BitcoinBip32Path,
    AccessStructureRef, MasterAppkey, SignTaskError, WireSignTask,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use schnorr_fun::fun::{g, marker::*, Point, Scalar, G};

use frostsnap_core::test::{RunSingleCoordinator as Run, Send, TestEnv, TEST_ENCRYPTION_KEY};

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

/// A 2-of-3 bitcoin wallet with its nonces ready, and the coordinator's view of its key.
struct Wallet {
    run: Run,
    env: TestEnv,
    rng: ChaCha20Rng,
    access_structure_ref: AccessStructureRef,
    master_appkey: MasterAppkey,
}

fn wallet(seed: u8) -> Wallet {
    let mut rng = ChaCha20Rng::from_seed([seed; 32]);
    let mut env = TestEnv::default();
    let run = Run::start_after_keygen_and_nonces(3, 2, &mut env, &mut rng, 1, purpose());
    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    let master_appkey = run
        .coordinator
        .get_frost_key(access_structure_ref.key_id)
        .unwrap()
        .complete_key
        .master_appkey;
    Wallet {
        run,
        env,
        rng,
        access_structure_ref,
        master_appkey,
    }
}

/// A template spending one owned input of `sats` and paying `recipient` a silent payment.
fn one_silent_payment(
    master_appkey: MasterAppkey,
    recipient: SilentPaymentRecipient,
    address: String,
) -> TransactionTemplate {
    let mut template = TransactionTemplate::new();
    template.push_imaginary_owned_input(
        LocalSpk {
            master_appkey,
            bip32_path: BitcoinBip32Path::external(0),
        },
        Amount::from_sat(100_000),
    );
    template.push_silent_payment_output(Amount::from_sat(50_000), recipient, address);
    template
}

#[test]
fn silent_payment_send_refuses_foreign_inputs() {
    let master = wallet(15).master_appkey;
    let (recipient, address) = recipient(0x61, 0x62);

    let mut template = one_silent_payment(master, recipient, address);
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

    let result = WireSignTask::BitcoinTransaction(template).check(master, purpose());
    assert!(matches!(result, Err(SignTaskError::SilentPaymentInputs)));
}

#[test]
fn silent_payment_send_refuses_an_address_that_is_not_its_keys() {
    let master = wallet(17).master_appkey;
    let (mismatched, _) = recipient(0x71, 0x72);
    let (_, other_address) = recipient(0x73, 0x74);

    let template = one_silent_payment(master, mismatched, other_address);

    let result = WireSignTask::BitcoinTransaction(template).check(master, purpose());
    assert!(matches!(
        result,
        Err(SignTaskError::SilentPaymentAddressMismatch)
    ));
}

#[test]
fn a_template_with_holes_is_not_signable() {
    let master = wallet(19).master_appkey;
    let (recipient, address) = recipient(0x81, 0x82);

    let template = one_silent_payment(master, recipient, address);

    let result = WireSignTask::BitcoinTransaction(template).check(master, purpose());
    assert!(matches!(
        result,
        Err(SignTaskError::SilentPaymentScriptMissing)
    ));
}

#[test]
fn silent_payment_ecdh_round_with_one_device_completes() {
    let mut wallet = wallet(21);
    let device = wallet.run.device_vec()[0];
    let (recipient, address) = recipient(0x91, 0x92);
    let template = one_silent_payment(wallet.master_appkey, recipient, address);

    let (_, sends) = wallet
        .run
        .coordinator
        .start_remote_sp_ecdh(
            wallet.access_structure_ref,
            WireSignTask::BitcoinTransaction(template),
            device,
            TEST_ENCRYPTION_KEY,
            &mut wallet.rng,
        )
        .unwrap();
    wallet.run.extend(sends);
    wallet
        .run
        .run_until_finished(&mut wallet.env, &mut wallet.rng)
        .unwrap();
}

#[test]
fn device_refuses_an_ecdh_request_for_other_scan_keys() {
    let mut wallet = wallet(23);
    let device = wallet.run.device_vec()[0];
    let (recipient, address) = recipient(0xa1, 0xa2);
    let template = one_silent_payment(wallet.master_appkey, recipient, address);

    let (_, sends) = wallet
        .run
        .coordinator
        .start_remote_sp_ecdh(
            wallet.access_structure_ref,
            WireSignTask::BitcoinTransaction(template),
            device,
            TEST_ENCRYPTION_KEY,
            &mut wallet.rng,
        )
        .unwrap();
    wallet.run.extend(sends);

    let result =
        wallet
            .run
            .run_until(&mut wallet.env, &mut wallet.rng, |run| {
                for send in run.message_queue_mut().iter_mut() {
                    if let Send::CoordinatorToDevice {
                        message:
                            CoordinatorToDeviceMessage::Signing(CoordinatorSigning::RequestSpEcdh(
                                request,
                            )),
                        ..
                    } = send
                    {
                        request.scan_keys.push(public(scalar(0xa3)));
                    }
                }
                run.message_queue().is_empty()
            });
    let error = result.expect_err("the device must refuse the tampered request");
    assert!(
        error
            .to_string()
            .contains("silent payment ECDH request does not match its transaction"),
        "refused for the wrong reason: {error}"
    );
}

#[test]
fn coordinator_refuses_a_device_with_a_bad_proof() {
    let mut wallet = wallet(11);
    let device = wallet.run.device_vec()[0];
    let (recipient, address) = recipient(0x41, 0x42);
    let template = one_silent_payment(wallet.master_appkey, recipient, address);

    let (_, sends) = wallet
        .run
        .coordinator
        .start_remote_sp_ecdh(
            wallet.access_structure_ref,
            WireSignTask::BitcoinTransaction(template),
            device,
            TEST_ENCRYPTION_KEY,
            &mut wallet.rng,
        )
        .unwrap();
    wallet.run.extend(sends);

    let result = wallet
        .run
        .run_until(&mut wallet.env, &mut wallet.rng, |run| {
            for send in run.message_queue_mut().iter_mut() {
                if let Send::DeviceToCoordinator {
                    message:
                        DeviceToCoordinatorMessage::Signing(DeviceSigning::SpEcdhShares {
                            shares, ..
                        }),
                    ..
                } = send
                {
                    shares[0].proof = [0u8; 64];
                }
            }
            run.message_queue().is_empty()
        });
    let error = result.expect_err("the coordinator must refuse the bad proof");
    assert!(
        error
            .to_string()
            .contains("silent payment ECDH share has a bad proof"),
        "refused for the wrong reason: {error}"
    );
}
