use bitcoin::Witness;
use frostsnap_core::coordinator::remote_signing::{RemoteSignSessionId, SignOffer, SpSubset};
use frostsnap_core::coordinator::StartSignError;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::silent_payments::{self, complete_sp_task, verify_sp_ecdh_shares};
use frostsnap_core::test::silent_payments::{
    binonces_of, bitcoin_template, expected_output_keys, public, recipient,
    reconstruct_root_secret, scalar, template, RemoteSigningEnv, SpRemote, NETWORK, SP_ID,
};
use frostsnap_core::test::{Run, RunSingleCoordinator, TestEnv, TEST_ENCRYPTION_KEY};
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_core::WireSignTask;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::Schnorr;

#[test]
fn two_coordinator_remote_signing() {
    let mut rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = RemoteSigningEnv::default();
    let mut run = Run::start_after_remote_keygen(
        &[1, 1],
        2,
        "remote sign key",
        KeyPurpose::Test,
        &mut env,
        &mut rng,
    );
    run.replenish_all_nonces(2, &mut env, &mut rng);

    let nonce_reservation_id = RemoteSignSessionId::new([0u8; 32]);

    let access_structure_ref = run.participants[0]
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    let sign_task = WireSignTask::Test {
        message: "remote signing test".into(),
    };

    let mut all_binonces = vec![];
    for i in 0..2 {
        let device_id = *run.participants[i].devices.keys().next().unwrap();
        let offer = run.participants[i]
            .coordinator
            .offer_to_sign(
                nonce_reservation_id,
                access_structure_ref,
                sign_task.clone(),
                device_id,
            )
            .unwrap();
        assert!(
            offer.sp_ecdh.is_empty(),
            "a plain offer carries no silent payment shares"
        );
        all_binonces.push(offer.participant_binonces.clone());
    }
    assert!(
        run.message_queue.is_empty(),
        "a plain offer never sends anything to a device"
    );

    for i in 0..2 {
        let device_id = *run.participants[i].devices.keys().next().unwrap();
        let sign_req = run.participants[i]
            .coordinator
            .sign_with_nonce_reservation(
                nonce_reservation_id,
                device_id,
                &all_binonces,
                None,
                TEST_ENCRYPTION_KEY,
            )
            .unwrap();
        run.extend_from_coordinator(i, sign_req);
    }

    run.run_until_finished(&mut env, &mut rng).unwrap();

    let session_id = frostsnap_core::message::GroupSignReq::from_binonces(
        sign_task.clone(),
        access_structure_ref.access_structure_id,
        &all_binonces,
    )
    .session_id();

    // Each coordinator emitted GotShare → env collected shares from each
    let shares_0 = &env.shares[&(0, session_id)];
    let shares_1 = &env.shares[&(1, session_id)];
    assert_eq!(shares_0.len(), 1);
    assert_eq!(shares_1.len(), 1);

    let all_shares = vec![shares_0[0].clone(), shares_1[0].clone()];

    let key_data = run.participants[0].coordinator.iter_keys().next().unwrap();
    let access_structure = key_data
        .get_access_structure(access_structure_ref.access_structure_id)
        .unwrap();
    let signing_key = frostsnap_core::coordinator::KeyContext {
        app_shared_key: access_structure.app_shared_key(),
        purpose: KeyPurpose::Test,
    };

    for shares in &all_shares {
        assert!(
            frostsnap_core::coordinator::remote_signing::verify_signature_shares(
                &sign_task,
                &signing_key,
                &all_binonces,
                shares,
            ),
            "each participant's shares should verify independently"
        );
    }

    let all_shares_refs: Vec<_> = all_shares.iter().collect();
    let signatures = frostsnap_core::coordinator::remote_signing::combine_signatures(
        sign_task.clone(),
        &signing_key,
        &all_binonces,
        &all_shares_refs,
    )
    .unwrap();

    let schnorr = Schnorr::<sha2::Sha256>::verify_only();
    let checked_task = sign_task
        .check(key_data.complete_key.master_appkey, KeyPurpose::Test)
        .unwrap();
    let decoded: Vec<_> = signatures
        .iter()
        .map(|s| (*s).into_decoded().unwrap())
        .collect();
    assert!(checked_task.verify_final_signatures(&schnorr, &decoded));
}

#[test]
fn completed_remote_signing_frees_its_nonce_stream() {
    let mut rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = RemoteSigningEnv::default();
    let mut run = Run::start_after_remote_keygen(
        &[1, 1],
        2,
        "remote sign key",
        KeyPurpose::Test,
        &mut env,
        &mut rng,
    );
    // One stream per device, so a second round needs the first one's stream back.
    run.replenish_all_nonces(1, &mut env, &mut rng);

    let access_structure_ref = run.participants[0]
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    for round in 0..2u8 {
        let id = RemoteSignSessionId::new([round; 32]);
        let sign_task = WireSignTask::Test {
            message: format!("round {round}"),
        };
        let mut all_binonces = vec![];
        for i in 0..2 {
            let device_id = *run.participants[i].devices.keys().next().unwrap();
            let offer = run.participants[i]
                .coordinator
                .offer_to_sign(id, access_structure_ref, sign_task.clone(), device_id)
                .unwrap();
            all_binonces.push(offer.participant_binonces.clone());
        }
        for i in 0..2 {
            let device_id = *run.participants[i].devices.keys().next().unwrap();
            let sign_req = run.participants[i]
                .coordinator
                .sign_with_nonce_reservation(
                    id,
                    device_id,
                    &all_binonces,
                    None,
                    TEST_ENCRYPTION_KEY,
                )
                .unwrap();
            run.extend_from_coordinator(i, sign_req);
        }
        run.run_until_finished(&mut env, &mut rng).unwrap();
    }
}

#[test]
fn cancel_remote_sign_session_reuses_nonces() {
    let mut env = TestEnv::default();
    let mut rng = ChaCha20Rng::from_seed([42u8; 32]);

    let mut run = RunSingleCoordinator::start_after_keygen_and_nonces(
        3,
        2,
        &mut env,
        &mut rng,
        2,
        KeyPurpose::Test,
    );

    let device_id = *run.devices.keys().next().unwrap();
    let access_structure_ref = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    let sign_task = WireSignTask::Test {
        message: "reuse nonces test".into(),
    };
    let id1 = RemoteSignSessionId::new([1u8; 32]);
    let offer1 = run
        .coordinator
        .offer_to_sign(id1, access_structure_ref, sign_task.clone(), device_id)
        .unwrap();
    let binonces_1 = offer1.participant_binonces.binonces.clone();

    run.coordinator.cancel_remote_sign_session(id1);

    let id2 = RemoteSignSessionId::new([2u8; 32]);
    let offer2 = run
        .coordinator
        .offer_to_sign(id2, access_structure_ref, sign_task, device_id)
        .unwrap();
    let binonces_2 = offer2.participant_binonces.binonces.clone();

    assert_eq!(binonces_1, binonces_2, "cancelled nonces should be reused");
}

#[test]
fn two_of_three_silent_payment_send_over_remote_signing() {
    let mut remote = SpRemote::new(42);
    let (first, first_address) = recipient(0x11, 0x12);
    let (second, second_address) = recipient(0x11, 0x13);
    let (third, third_address) = recipient(0x14, 0x15);
    let paths = [
        BitcoinBip32Path::external(7),
        BitcoinBip32Path::internal(42),
    ];
    let template = template(
        remote.master_appkey,
        &[(paths[0], 42_000), (paths[1], 1_337_000)],
        &[
            (50_000, first, first_address),
            (60_000, second, second_address),
            (70_000, third, third_address),
        ],
        1_000_000,
    );
    let committed = WireSignTask::BitcoinTransaction(template.clone());

    // Everyone offers; the template with holes is what is committed.
    let offers: Vec<_> = (0..3).map(|i| remote.offer_sp(i, &template)).collect();
    for offer in &offers {
        assert_eq!(offer.sp_ecdh.len(), 2, "one share per distinct scan key");
    }
    assert!(bitcoin_template(&committed)
        .silent_payment_outputs()
        .all(|sp| sp.script.is_none()));
    for i in 0..3 {
        let session = remote.run.participants[i]
            .coordinator
            .get_remote_sign_session(SP_ID, remote.device(i))
            .unwrap();
        assert_eq!(session.sign_task, committed);
    }

    // The subset {0, 2} signs; participant 1 only offered.
    let subset = [0usize, 2];
    let subset_offers = [&offers[0], &offers[2]];
    let binonces = binonces_of(&subset_offers);
    let sp = remote.sp_subset(&committed, &subset_offers);
    remote.sign(&subset, &binonces, Some(&sp)).unwrap();

    let signatures = remote.combine(&sp.completed_task, &subset, &binonces);
    let checked = sp
        .completed_task
        .clone()
        .check(remote.master_appkey, KeyPurpose::Bitcoin(NETWORK))
        .expect("the completed send is a valid sign task");
    assert!(checked.verify_final_signatures(&Schnorr::<sha2::Sha256>::verify_only(), &signatures));

    // The final transaction pays BIP352's keys, computed from the whole root secret.
    let completed = bitcoin_template(&sp.completed_task);
    let mut tx = completed.to_rust_bitcoin_tx();
    for (input, signature) in tx.input.iter_mut().zip(&signatures) {
        input.witness = Witness::from_slice(&[signature.to_bytes()]);
    }
    let inputs: Vec<_> = paths
        .iter()
        .copied()
        .zip(completed.inputs().iter().map(|input| input.outpoint()))
        .collect();
    let root_secret = reconstruct_root_secret(&remote.run, &subset);
    let expected = expected_output_keys(root_secret, &inputs, &[first, second, third]);
    let paid: Vec<[u8; 32]> = completed
        .silent_payment_outputs()
        .map(|sp| {
            sp.script.as_ref().unwrap().as_bytes()[2..34]
                .try_into()
                .unwrap()
        })
        .collect();
    assert_eq!(paid, expected);
    for key in &expected {
        assert!(
            tx.output.iter().any(|output| output.script_pubkey.is_p2tr()
                && output.script_pubkey.as_bytes()[2..34] == key[..]),
            "the final transaction pays {}",
            frostsnap_core::hex::encode(key)
        );
    }

    // Any other 2-subset of the offers completes to the same transaction.
    let other_offers = [&offers[1], &offers[2]];
    let other = remote.sp_subset(&committed, &other_offers);
    assert_eq!(other.completed_task, sp.completed_task);
}

#[test]
fn silent_payment_send_with_several_inputs_shared_scan_key_and_change() {
    let mut remote = SpRemote::new(44);
    let (first, first_address) = recipient(0x21, 0x22);
    let (second, second_address) = recipient(0x21, 0x23);
    let (third, third_address) = recipient(0x24, 0x25);
    let paths = [
        BitcoinBip32Path::external(0),
        BitcoinBip32Path::internal(1),
        BitcoinBip32Path::external(5),
    ];
    let mut template = template(
        remote.master_appkey,
        &[
            (paths[0], 100_000),
            (paths[1], 200_000),
            (paths[2], 300_000),
        ],
        &[
            (50_000, first, first_address),
            (50_000, second, second_address),
            (50_000, third, third_address),
        ],
        400_000,
    );
    template.push_foreign_output(bitcoin::TxOut {
        value: bitcoin::Amount::from_sat(40_000),
        script_pubkey: bitcoin::ScriptBuf::from_bytes(
            [&[0x51u8, 0x20][..], &[0xee; 32][..]].concat(),
        ),
    });
    let committed = WireSignTask::BitcoinTransaction(template.clone());

    let offers: Vec<_> = (0..3).map(|i| remote.offer_sp(i, &template)).collect();

    // The subset {0, 2} signs; participant 1 only offered.
    let subset = [0usize, 2];
    let subset_offers = [&offers[0], &offers[2]];
    let binonces = binonces_of(&subset_offers);
    let sp = remote.sp_subset(&committed, &subset_offers);
    remote.sign(&subset, &binonces, Some(&sp)).unwrap();

    let signatures = remote.combine(&sp.completed_task, &subset, &binonces);
    let checked = sp
        .completed_task
        .clone()
        .check(remote.master_appkey, KeyPurpose::Bitcoin(NETWORK))
        .expect("the completed send is a valid sign task");
    assert!(checked.verify_final_signatures(&Schnorr::<sha2::Sha256>::verify_only(), &signatures));

    // Each silent payment output pays the key derived from the input secrets, in output order.
    let completed = bitcoin_template(&sp.completed_task);
    let inputs: Vec<_> = paths
        .iter()
        .copied()
        .zip(completed.inputs().iter().map(|input| input.outpoint()))
        .collect();
    let root_secret = reconstruct_root_secret(&remote.run, &subset);
    let expected = expected_output_keys(root_secret, &inputs, &[first, second, third]);
    let paid: Vec<[u8; 32]> = completed
        .silent_payment_outputs()
        .map(|sp| {
            sp.script.as_ref().unwrap().as_bytes()[2..34]
                .try_into()
                .unwrap()
        })
        .collect();
    assert_eq!(paid, expected);
}

#[test]
fn tampered_share_or_bad_proof_in_an_offer_is_rejected() {
    let mut remote = SpRemote::new(7);
    let (recipient, address) = recipient(0x21, 0x22);
    let template = template(
        remote.master_appkey,
        &[(BitcoinBip32Path::external(1), 500_000)],
        &[(100_000, recipient, address)],
        390_000,
    );
    let committed = WireSignTask::BitcoinTransaction(template.clone());
    let offers: Vec<_> = (0..3).map(|i| remote.offer_sp(i, &template)).collect();
    let key = remote.signing_key.clone();
    let check = |offer: &SignOffer, shares: &[silent_payments::EcdhShare]| {
        verify_sp_ecdh_shares(
            &committed,
            &key,
            std::slice::from_ref(&offer.participant_binonces),
            shares,
        )
    };
    for offer in &offers {
        check(offer, &offer.sp_ecdh).expect("an honest offer verifies");
    }

    let mut tampered = offers[0].sp_ecdh.clone();
    tampered[0].ecdh_share = public(scalar(0x77));
    assert!(check(&offers[0], &tampered).is_err(), "tampered share");

    let mut bad_proof = offers[0].sp_ecdh.clone();
    bad_proof[0].proof[0] ^= 1;
    assert!(check(&offers[0], &bad_proof).is_err(), "bad proof");

    assert!(
        check(&offers[0], &offers[1].sp_ecdh).is_err(),
        "another participant's valid share is not this participant's"
    );
    assert!(check(&offers[0], &[]).is_err(), "missing shares");

    // A signer given a subset with the tampered share refuses it and nothing is spent.
    let binonces = binonces_of(&[&offers[0], &offers[1]]);
    let mut sp = remote.sp_subset(&committed, &[&offers[0], &offers[1]]);
    sp.shares[0].ecdh_share = public(scalar(0x77));
    let result = remote.sign(&[0], &binonces, Some(&sp));
    assert!(
        matches!(result, Err(StartSignError::SilentPaymentShares(_))),
        "{result:?}"
    );
    assert!(!remote.run.participants[0]
        .coordinator
        .can_sign_with_nonce_reservation(&binonces, Some(&sp), SP_ID, remote.device(0),));
}

#[test]
fn completed_task_that_differs_from_the_committed_template_is_refused() {
    let mut remote = SpRemote::new(11);
    let (recipient, address) = recipient(0x31, 0x32);
    let inputs = [(BitcoinBip32Path::external(2), 500_000)];
    let committed_template = template(
        remote.master_appkey,
        &inputs,
        &[(100_000, recipient, address.clone())],
        390_000,
    );
    // Same inputs, so the same input_hash and valid shares, but a different payment.
    let other_template = template(
        remote.master_appkey,
        &inputs,
        &[(300_000, recipient, address)],
        190_000,
    );
    let committed = WireSignTask::BitcoinTransaction(committed_template.clone());
    let offers: Vec<_> = (0..3)
        .map(|i| remote.offer_sp(i, &committed_template))
        .collect();
    let subset_offers = [&offers[0], &offers[1]];
    let binonces = binonces_of(&subset_offers);
    let honest = remote.sp_subset(&committed, &subset_offers);

    let other_completed = complete_sp_task(
        &WireSignTask::BitcoinTransaction(other_template),
        &remote.signing_key,
        &binonces,
        &honest.shares,
    )
    .unwrap();
    let swapped = SpSubset {
        completed_task: other_completed,
        shares: honest.shares.clone(),
    };
    let result = remote.sign(&[0], &binonces, Some(&swapped));
    assert!(
        matches!(result, Err(StartSignError::SilentPaymentTaskMismatch)),
        "{result:?}"
    );

    // Nothing was consumed by the refusal: the honest subset still signs.
    remote.sign(&[0, 1], &binonces, Some(&honest)).unwrap();
    let signatures = remote.combine(&honest.completed_task, &[0, 1], &binonces);
    let checked = honest
        .completed_task
        .clone()
        .check(remote.master_appkey, KeyPurpose::Bitcoin(NETWORK))
        .unwrap();
    assert!(checked.verify_final_signatures(&Schnorr::<sha2::Sha256>::verify_only(), &signatures));
}

#[test]
fn silent_payment_and_plain_offers_do_not_mix() {
    let mut remote = SpRemote::new(13);
    let (recipient, address) = recipient(0x41, 0x42);
    let sp_template = template(
        remote.master_appkey,
        &[(BitcoinBip32Path::external(4), 500_000)],
        &[(100_000, recipient, address)],
        390_000,
    );
    let plain_template = template(
        remote.master_appkey,
        &[(BitcoinBip32Path::external(4), 500_000)],
        &[],
        490_000,
    );
    let device_id = remote.device(0);
    let access_structure_ref = remote.access_structure_ref;
    let coordinator = &mut remote.run.participants[0].coordinator;

    // A template with holes cannot be offered without the device's ECDH.
    let result = coordinator.offer_to_sign(
        SP_ID,
        access_structure_ref,
        WireSignTask::BitcoinTransaction(sp_template.clone()),
        device_id,
    );
    assert!(
        matches!(result, Err(StartSignError::SignTask(_))),
        "{result:?}"
    );

    // A send with no silent payment outputs has nothing to ask the device.
    let result = coordinator.start_remote_sp_ecdh(
        access_structure_ref,
        WireSignTask::BitcoinTransaction(plain_template),
        device_id,
        TEST_ENCRYPTION_KEY,
        &mut remote.rng,
    );
    assert!(matches!(result, Err(StartSignError::NotSilentPayment)));

    // And a silent payment session cannot sign without its subset's shares.
    let offer = remote.offer_sp(0, &sp_template);
    let binonces = binonces_of(&[&offer, &offer]);
    let result = remote.sign(&[0], &binonces, None);
    assert!(
        matches!(result, Err(StartSignError::SilentPaymentSharesMissing)),
        "{result:?}"
    );
}

/// The offer a participant made is the one it finds again, and only for its own session and device.
#[test]
fn existing_sp_offer_finds_the_offer_already_made() {
    let mut remote = SpRemote::new(23);
    let (recipient, address) = recipient(0x91, 0x92);
    let sp_template = template(
        remote.master_appkey,
        &[(BitcoinBip32Path::external(0), 500_000)],
        &[(70_000, recipient, address)],
        420_000,
    );
    let offer = remote.offer_sp(0, &sp_template);

    let found = remote.run.participants[0]
        .coordinator
        .existing_sp_offer(SP_ID, remote.device(0))
        .expect("the offer made for this session and device is found");
    assert_eq!(
        found.participant_binonces.share_index,
        offer.participant_binonces.share_index
    );
    assert_eq!(
        found.participant_binonces.binonces,
        offer.participant_binonces.binonces
    );
    assert_eq!(found.sp_ecdh, offer.sp_ecdh);

    assert!(remote.run.participants[0]
        .coordinator
        .existing_sp_offer(SP_ID, remote.device(1))
        .is_none());
    assert!(remote.run.participants[0]
        .coordinator
        .existing_sp_offer(RemoteSignSessionId([1u8; 32]), remote.device(0))
        .is_none());

    // A plain offer is not a silent payment offer, even for a session it exists in.
    let plain_id = RemoteSignSessionId([2u8; 32]);
    let plain = template(
        remote.master_appkey,
        &[(BitcoinBip32Path::external(1), 500_000)],
        &[],
        490_000,
    );
    let access_structure_ref = remote.access_structure_ref;
    let device_id = remote.device(0);
    remote.run.participants[0]
        .coordinator
        .offer_to_sign(
            plain_id,
            access_structure_ref,
            WireSignTask::BitcoinTransaction(plain),
            device_id,
        )
        .unwrap();
    assert!(remote.run.participants[0]
        .coordinator
        .existing_sp_offer(plain_id, device_id)
        .is_none());
}
