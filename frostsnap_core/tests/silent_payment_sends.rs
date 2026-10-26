//! Silent payment sends over remote signing: the checks a template must pass, the ECDH round that
//! derives the scripts, the rejections of a tampered request, and a restart of the coordinator.

use bitcoin::{hashes::Hash, Amount, OutPoint, Txid};
use frostsnap_core::{
    bitcoin_transaction::{PushInput, TransactionTemplate},
    device::KeyPurpose,
    message::{
        signing::{CoordinatorSigning, RequestSpSign},
        CoordinatorToDeviceMessage, DeviceToCoordinatorMessage,
    },
    silent_payments::SilentPaymentRecipient,
    test::{
        silent_payments::{
            binonces_of, expected_output_keys, public, recipient, reconstruct_root_secret, scalar,
            template, SpRemote, NETWORK, SP_ID,
        },
        Send, TEST_ENCRYPTION_KEY,
    },
    tweak::BitcoinBip32Path,
    SignTaskError, WireSignTask,
};
use schnorr_fun::Schnorr;

fn purpose() -> KeyPurpose {
    KeyPurpose::Bitcoin(NETWORK)
}

/// The template of a send with one owned input of 100_000 sats and one silent payment output.
fn one_silent_payment(
    master_appkey: frostsnap_core::MasterAppkey,
    recipient: SilentPaymentRecipient,
    address: String,
) -> TransactionTemplate {
    template(
        master_appkey,
        &[(BitcoinBip32Path::external(0), 100_000)],
        &[(50_000, recipient, address)],
        40_000,
    )
}

/// The template the device tests sign: one silent payment output and a change output.
fn device_test_template(remote: &SpRemote) -> TransactionTemplate {
    let (recipient, address) = recipient(0x31, 0x32);
    template(
        remote.master_appkey,
        &[(BitcoinBip32Path::external(1), 90_000)],
        &[(60_000, recipient, address)],
        20_000,
    )
}

fn sp_request(send: &mut Send) -> Option<&mut RequestSpSign> {
    match send {
        Send::CoordinatorToDevice {
            message: CoordinatorToDeviceMessage::Signing(CoordinatorSigning::RequestSpSign(request)),
            ..
        } => Some(request.as_mut()),
        _ => None,
    }
}

/// Both signers of a 2-of-3 send make their offers for `template`, then the signing requests are
/// sent with `corrupt` applied to the messages in flight. Returns the reason the devices refused.
fn refusal_of_corrupted_requests(corrupt: impl Fn(&mut Send)) -> String {
    let mut remote = SpRemote::new(9);
    let template = device_test_template(&remote);
    let offers = [remote.offer_sp(0, &template), remote.offer_sp(1, &template)];
    let task = WireSignTask::BitcoinTransaction(template);
    let sp = remote.sp_subset(&task, &[&offers[0], &offers[1]]);
    let binonces = binonces_of(&[&offers[0], &offers[1]]);
    for i in 0..2 {
        let device_id = remote.device(i);
        let sends = remote.run.participants[i]
            .coordinator
            .sign_with_nonce_reservation(
                SP_ID,
                device_id,
                &binonces,
                Some(&sp),
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        remote.run.extend_from_coordinator(i, sends);
    }

    let result = remote
        .run
        .run_until(&mut remote.env, &mut remote.rng, |run| {
            for send in run.message_queue.iter_mut() {
                corrupt(send);
            }
            run.message_queue.is_empty()
        });
    result
        .expect_err("the devices must refuse the corrupted request")
        .to_string()
}

/// The template with the first silent payment script replaced by the script of another key.
fn with_other_sp_script(task: &WireSignTask) -> WireSignTask {
    let WireSignTask::BitcoinTransaction(template) = task else {
        panic!("expected a bitcoin transaction")
    };
    // The key is 32 bytes in the encoding, so its first occurrence is the script's.
    let encoded = bincode::encode_to_vec(template, bincode::config::standard()).unwrap();
    let script = template
        .silent_payment_outputs()
        .next()
        .unwrap()
        .script
        .as_ref()
        .unwrap();
    let key = script.as_bytes()[2..34].to_vec();
    let other = public(scalar(0x79)).to_xonly_bytes();
    let at = encoded
        .windows(32)
        .position(|window| window == key.as_slice())
        .expect("the key is in the encoding");
    let mut tampered = encoded.clone();
    tampered[at..at + 32].copy_from_slice(&other);
    let (decoded, _) = bincode::decode_from_slice(&tampered, bincode::config::standard())
        .expect("still a template");
    WireSignTask::BitcoinTransaction(decoded)
}

#[test]
fn silent_payment_send_refuses_foreign_inputs() {
    let remote = SpRemote::new(15);
    let (recipient, address) = recipient(0x61, 0x62);

    let mut template = one_silent_payment(remote.master_appkey, recipient, address);
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

    let result = WireSignTask::BitcoinTransaction(template).check(remote.master_appkey, purpose());
    assert!(matches!(result, Err(SignTaskError::SilentPaymentInputs)));
}

#[test]
fn silent_payment_send_refuses_an_address_that_is_not_its_keys() {
    let remote = SpRemote::new(17);
    let (mismatched, _) = recipient(0x71, 0x72);
    let (_, other_address) = recipient(0x73, 0x74);

    let template = one_silent_payment(remote.master_appkey, mismatched, other_address);

    let result = WireSignTask::BitcoinTransaction(template).check(remote.master_appkey, purpose());
    assert!(matches!(
        result,
        Err(SignTaskError::SilentPaymentAddressMismatch)
    ));
}

#[test]
fn a_template_with_holes_is_not_signable() {
    let remote = SpRemote::new(19);
    let (recipient, address) = recipient(0x81, 0x82);

    let template = one_silent_payment(remote.master_appkey, recipient, address);

    let result = WireSignTask::BitcoinTransaction(template).check(remote.master_appkey, purpose());
    assert!(matches!(
        result,
        Err(SignTaskError::SilentPaymentScriptMissing)
    ));
}

#[test]
fn silent_payment_ecdh_round_with_one_device_completes() {
    let mut remote = SpRemote::new(21);
    let (recipient, address) = recipient(0x91, 0x92);
    let template = one_silent_payment(remote.master_appkey, recipient, address);

    // `offer_sp` panics unless the device's reply reached the user.
    let offer = remote.offer_sp(0, &template);
    assert_eq!(offer.sp_ecdh.len(), 1, "one share for the one scan key");
}

#[test]
fn device_refuses_an_ecdh_request_for_other_scan_keys() {
    let mut remote = SpRemote::new(23);
    let (recipient, address) = recipient(0xa1, 0xa2);
    let task = WireSignTask::BitcoinTransaction(one_silent_payment(
        remote.master_appkey,
        recipient,
        address,
    ));
    let device_id = remote.device(0);
    let (_, sends) = remote.run.participants[0]
        .coordinator
        .start_remote_sp_ecdh(
            remote.access_structure_ref,
            task,
            device_id,
            TEST_ENCRYPTION_KEY,
            &mut remote.rng,
        )
        .unwrap();
    remote.run.extend_from_coordinator(0, sends);

    let result =
        remote
            .run
            .run_until(&mut remote.env, &mut remote.rng, |run| {
                for send in run.message_queue.iter_mut() {
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
                run.message_queue.is_empty()
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
    let mut remote = SpRemote::new(11);
    let (recipient, address) = recipient(0x41, 0x42);
    let task = WireSignTask::BitcoinTransaction(one_silent_payment(
        remote.master_appkey,
        recipient,
        address,
    ));
    let device_id = remote.device(0);
    let (_, sends) = remote.run.participants[0]
        .coordinator
        .start_remote_sp_ecdh(
            remote.access_structure_ref,
            task,
            device_id,
            TEST_ENCRYPTION_KEY,
            &mut remote.rng,
        )
        .unwrap();
    remote.run.extend_from_coordinator(0, sends);

    let result = remote
        .run
        .run_until(&mut remote.env, &mut remote.rng, |run| {
            for send in run.message_queue.iter_mut() {
                if let Send::DeviceToCoordinator {
                    message:
                        DeviceToCoordinatorMessage::Signing(
                            frostsnap_core::message::signing::DeviceSigning::SpEcdhShares {
                                shares,
                                ..
                            },
                        ),
                    ..
                } = send
                {
                    shares[0].proof = [0u8; 64];
                }
            }
            run.message_queue.is_empty()
        });
    let error = result.expect_err("the coordinator must refuse the bad proof");
    assert!(
        error
            .to_string()
            .contains("silent payment ECDH share has a bad proof"),
        "refused for the wrong reason: {error}"
    );
}

#[test]
fn device_refuses_a_tampered_ecdh_share() {
    let refusal = refusal_of_corrupted_requests(|send| {
        if let Some(request) = sp_request(send) {
            request.sp_proofs.shares[0].ecdh_share = public(scalar(0x77));
        }
    });
    assert!(
        refusal.contains("silent payment ECDH share has a bad proof"),
        "refused for the wrong reason: {refusal}"
    );
}

#[test]
fn device_refuses_a_bad_proof() {
    let refusal = refusal_of_corrupted_requests(|send| {
        if let Some(request) = sp_request(send) {
            // Another share's proof, which is valid for that share and not for this one.
            let other = request.sp_proofs.shares[1].proof;
            request.sp_proofs.shares[0].proof = other;
        }
    });
    assert!(
        refusal.contains("silent payment ECDH share has a bad proof"),
        "refused for the wrong reason: {refusal}"
    );
}

#[test]
fn device_refuses_an_app_poly_that_does_not_match_the_access_structure() {
    let refusal = refusal_of_corrupted_requests(|send| {
        if let Some(request) = sp_request(send) {
            request.sp_proofs.app_poly[1] = public(scalar(0x78)).mark_zero();
        }
    });
    assert!(
        refusal.contains("app polynomial does not match the access structure"),
        "refused for the wrong reason: {refusal}"
    );
}

#[test]
fn device_refuses_a_template_whose_silent_payment_script_is_not_derived() {
    let refusal = refusal_of_corrupted_requests(|send| {
        if let Some(request) = sp_request(send) {
            request.request_sign.group_sign_req.sign_task =
                with_other_sp_script(&request.request_sign.group_sign_req.sign_task);
        }
    });
    assert!(
        refusal.contains("silent payment output scripts do not match the ECDH shares"),
        "refused for the wrong reason: {refusal}"
    );
}

#[test]
fn device_refuses_fewer_ecdh_shares_than_the_threshold() {
    let remote = SpRemote::new(9);
    let holes = device_test_template(&remote);
    let master_appkey = remote.master_appkey;

    let refusal = refusal_of_corrupted_requests(|send| {
        if let Some(request) = sp_request(send) {
            // One of the two shares: its proof is valid, and the scripts are rederived from it so
            // they match the bundle. Only the threshold check can catch it.
            let first = request.sp_proofs.shares[0].share_index;
            request
                .sp_proofs
                .shares
                .retain(|share| share.share_index == first);
            let secrets = holes
                .sp_shared_secrets(master_appkey, &request.sp_proofs.shares)
                .expect("a single share still combines");
            let scripts = holes
                .with_sp_scripts(&secrets)
                .expect("scripts derive from the single share");
            request.request_sign.group_sign_req.sign_task =
                WireSignTask::BitcoinTransaction(scripts);
        }
    });
    assert!(
        refusal.contains("fewer silent payment ECDH shares than the threshold"),
        "refused for the wrong reason: {refusal}"
    );
}

#[test]
fn device_refuses_a_template_with_a_missing_silent_payment_script() {
    let remote = SpRemote::new(9);
    let holes = WireSignTask::BitcoinTransaction(device_test_template(&remote));

    let refusal = refusal_of_corrupted_requests(|send| {
        if let Some(request) = sp_request(send) {
            request.request_sign.group_sign_req.sign_task = holes.clone();
        }
    });
    assert!(
        refusal.contains("A silent payment output has no script yet"),
        "refused for the wrong reason: {refusal}"
    );
}

#[test]
fn device_refuses_silent_payment_outputs_sent_without_their_proofs() {
    let refusal = refusal_of_corrupted_requests(|send| {
        let Send::CoordinatorToDevice { message, .. } = send else {
            return;
        };
        let CoordinatorToDeviceMessage::Signing(CoordinatorSigning::RequestSpSign(request)) =
            message
        else {
            return;
        };
        let plain = CoordinatorToDeviceMessage::Signing(CoordinatorSigning::RequestSign(Box::new(
            request.request_sign.clone(),
        )));
        *message = plain;
    });
    assert!(
        refusal.contains("silent payment outputs without their proofs"),
        "refused for the wrong reason: {refusal}"
    );
}

#[test]
fn silent_payment_ecdh_survives_a_coordinator_restart() {
    let mut remote = SpRemote::new(13);
    let paths = [
        BitcoinBip32Path::external(9),
        BitcoinBip32Path::internal(10),
    ];
    let (recipient, address) = recipient(0x51, 0x52);
    let template = template(
        remote.master_appkey,
        &[(paths[0], 500_000), (paths[1], 600_000)],
        &[(70_000, recipient, address)],
        1_000_000,
    );
    let offers = [remote.offer_sp(0, &template), remote.offer_sp(1, &template)];
    let task = WireSignTask::BitcoinTransaction(template);
    let sp = remote.sp_subset(&task, &[&offers[0], &offers[1]]);
    let binonces = binonces_of(&[&offers[0], &offers[1]]);

    // Restart: every coordinator is rebuilt from the mutations it has staged.
    remote.run.check_mutations();
    for participant in &mut remote.run.participants {
        participant.coordinator = participant.start_coordinator.clone();
    }

    remote.sign(&[0, 1], &binonces, Some(&sp)).unwrap();
    let signatures = remote.combine(&sp.completed_task, &[0, 1], &binonces);
    let checked = sp
        .completed_task
        .clone()
        .check(remote.master_appkey, purpose())
        .unwrap();
    assert!(checked.verify_final_signatures(&Schnorr::<sha2::Sha256>::verify_only(), &signatures));

    let WireSignTask::BitcoinTransaction(paid) = &sp.completed_task else {
        panic!("expected a bitcoin transaction")
    };
    let inputs = paths
        .iter()
        .copied()
        .zip(paid.inputs().iter().map(|input| input.outpoint()))
        .collect::<Vec<_>>();
    let root_secret = reconstruct_root_secret(&remote.run, &[0, 1]);
    let expected = expected_output_keys(root_secret, &inputs, &[recipient]);
    let paid_keys = paid
        .silent_payment_outputs()
        .map(|sp| {
            sp.script.as_ref().unwrap().as_bytes()[2..34]
                .try_into()
                .unwrap()
        })
        .collect::<Vec<[u8; 32]>>();
    assert_eq!(paid_keys, expected);
}
