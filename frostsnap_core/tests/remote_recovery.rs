use frostsnap_core::coordinator::restoration::{RecoverShare, RecoveringAccessStructure};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::HeldShare2;
use frostsnap_core::test::{RunSingleCoordinator as Run, TestEnv, TEST_ENCRYPTION_KEY, TEST_FINGERPRINT};
use frostsnap_core::{DeviceId, EnterPhysicalId};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::collections::BTreeSet;

/// The full remote-recovery flow from a local participant's
/// perspective: blank device enters its physical backup, coordinator
/// finalises the remote recovery with a bundle of one-local + two-remote
/// shares, coordinator drives consolidation, and the local device ends
/// up holding an encrypted share bound to the recovered access
/// structure. Verifies both the coordinator-side book-keeping
/// (`knows_about_share`) and the device-side outcome
/// (`get_encrypted_share`).
#[test]
fn end_to_end_local_device_consolidates_share() {
    let mut rng = ChaCha20Rng::from_seed([31u8; 32]);
    let mut env = TestEnv::default();

    // Fixture: run a real 2-of-3 keygen to produce three ShareBackups
    // for a specific wallet. These are the "reference" shares — in a
    // real remote recovery the group would have obtained them in prior
    // sessions and now be reconstructing.
    let mut run = Run::start_after_keygen_and_nonces(3, 2, &mut env, &mut rng, 2, KeyPurpose::Test);
    let asref_ref = run
        .coordinator
        .iter_keys()
        .next()
        .unwrap()
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    for &device_id in &run.device_set() {
        let msgs = run
            .coordinator
            .request_device_display_backup(device_id, asref_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(msgs);
    }
    run.run_until_finished(&mut env, &mut rng).unwrap();
    let mut fixture_backups: Vec<_> = env
        .backups
        .values()
        .map(|(_, backup)| backup.clone())
        .collect();
    fixture_backups.sort_by_key(|b| b.share_image().index);
    let local_backup = fixture_backups[0].clone();
    let remote_backup_a = fixture_backups[1].clone();
    let remote_backup_b = fixture_backups[2].clone();
    let local_share_index = local_backup.share_image().index;

    // Wipe everything and start over with a single blank device — this
    // is what a fresh install with one device looks like.
    run.clear_coordinator();
    env.backup_to_enter.clear();
    env.physical_backups_entered.clear();
    let local_device = run.new_device(&mut rng);

    // Blank device enters its physical backup. The share lands in the
    // device's tmp_loaded_backups (no save-to-restoration step — that
    // would emit local restoration mutations we don't want).
    env.backup_to_enter.insert(local_device, local_backup);
    let enter_id = EnterPhysicalId::new(&mut rng);
    let enter_msgs = run
        .coordinator
        .tell_device_to_load_physical_backup(enter_id, local_device);
    run.extend(enter_msgs);
    run.run_until_finished(&mut env, &mut rng).unwrap();

    // Compose the RecoverShare bundle: our local device's just-entered
    // share plus two hardcoded "remote" shares (fresh DeviceIds standing
    // in for what would be nostr_pubkey_to_device_id(peer_pubkey) in
    // production).
    let remote_device_a = run.new_device(&mut rng);
    let remote_device_b = run.new_device(&mut rng);
    let mk = |device: DeviceId, backup: &frost_backup::ShareBackup| RecoverShare {
        held_by: device,
        held_share: HeldShare2 {
            access_structure_ref: None,
            share_image: backup.share_image(),
            threshold: None,
            key_name: None,
            purpose: None,
            needs_consolidation: true,
        },
    };
    let shares = vec![
        mk(local_device, &fixture_backups[0]),
        mk(remote_device_a, &remote_backup_a),
        mk(remote_device_b, &remote_backup_b),
    ];

    let ras = RecoveringAccessStructure::new(&shares, Some(2), TEST_FINGERPRINT);
    assert!(ras.shared_key.is_some(), "3 valid shares reconstruct 2-of-3");

    // Finalize: coordinator persists the wallet and queues
    // PendingConsolidation only for local_device.
    let my_local: BTreeSet<DeviceId> = [local_device].into_iter().collect();
    let recovered_asref = run
        .coordinator
        .finalize_remote_recovery(
            &ras,
            "Recovered".to_string(),
            KeyPurpose::Test,
            &my_local,
            TEST_ENCRYPTION_KEY,
            &mut rng,
        )
        .expect("finalize succeeds");
    assert_eq!(recovered_asref, asref_ref);
    assert!(
        run.coordinator
            .has_backups_that_need_to_be_consolidated(local_device),
        "local device queued for consolidation"
    );

    // Drive consolidation. The coordinator sends ConsolidateBackup to
    // the device; the device pulls its share out of tmp_loaded_backups,
    // extracts the secret against the recovered SharedKey, stores it as
    // an encrypted CompleteSecretShare, and acks back.
    let consolidate_msgs = run
        .coordinator
        .consolidate_pending_physical_backups(local_device, TEST_ENCRYPTION_KEY);
    run.extend(consolidate_msgs);
    run.run_until_finished(&mut env, &mut rng).unwrap();

    // Coordinator now tracks the local device as a share holder for
    // the recovered access structure.
    assert!(
        run.coordinator
            .knows_about_share(local_device, recovered_asref, local_share_index),
        "coordinator records local device as holder of its share"
    );

    // Device holds an encrypted share it can decrypt and use to sign.
    let device = run.device(local_device);
    let _encrypted_share = device
        .get_encrypted_share(recovered_asref, local_share_index)
        .expect("local device holds a consolidated share");

    // Remote participants' devices are not tracked as share holders on
    // our coordinator — we're not lying about holding their shares.
    assert!(
        !run.coordinator.knows_about_share(
            remote_device_a,
            recovered_asref,
            remote_backup_a.share_image().index,
        ),
        "remote device A is not recorded as a share holder on our coordinator"
    );
    assert!(
        !run.coordinator.knows_about_share(
            remote_device_b,
            recovered_asref,
            remote_backup_b.share_image().index,
        ),
        "remote device B is not recorded as a share holder on our coordinator"
    );
}
