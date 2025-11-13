use common::TEST_ENCRYPTION_KEY;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::EnterPhysicalId;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

mod common;
mod env;
use crate::common::Run;
use crate::env::TestEnv;

#[test]
fn restore_2_of_3_with_physical_backups_propagates_threshold() {
    let mut test_rng = ChaCha20Rng::from_seed([99u8; 32]);
    let mut env = TestEnv::default();

    // Create a 2-of-3 key
    let mut run =
        Run::start_after_keygen_and_nonces(3, 2, &mut env, &mut test_rng, 2, KeyPurpose::Test);

    let device_set = run.device_set();
    let _devices: Vec<_> = device_set.iter().cloned().collect();

    let key_data = run.coordinator.iter_keys().next().unwrap();
    let access_structure_ref = key_data
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    // Display backups for all devices
    for &device_id in &device_set {
        let display_backup = run
            .coordinator
            .request_device_display_backup(device_id, access_structure_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(display_backup);
    }
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    assert_eq!(env.backups.len(), 3);

    // Get the backups
    let backups: Vec<_> = env
        .backups
        .values()
        .map(|(_, backup)| backup.clone())
        .collect();

    // Clear the coordinator and create fresh devices to simulate starting fresh restoration
    run.clear_coordinator();
    let devices: Vec<_> = (0..3).map(|_| run.new_device(&mut test_rng)).collect();

    // Assign backups to devices before they enter them
    for (i, &device_id) in devices.iter().enumerate() {
        env.backup_to_enter.insert(device_id, backups[i].clone());
    }

    // Start restoration WITHOUT specifying threshold
    let restoration_id = frostsnap_core::RestorationId::new(&mut test_rng);
    run.coordinator.start_restoring_key(
        "Restored Wallet".to_string(),
        None, // No threshold specified
        KeyPurpose::Test,
        restoration_id,
    );

    // First device enters its backup - coordinator doesn't know threshold yet
    let enter_physical_id_1 = EnterPhysicalId::new(&mut test_rng);
    let enter_backup_1 = run
        .coordinator
        .tell_device_to_load_physical_backup(enter_physical_id_1, devices[0]);
    run.extend(enter_backup_1);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let physical_backup_phase_1 = *env
        .physical_backups_entered
        .last()
        .expect("Should have a physical backup phase");

    // Check and save the first backup
    let check_result = run
        .coordinator
        .check_physical_backup_compatible_with_restoration(
            restoration_id,
            physical_backup_phase_1,
            TEST_ENCRYPTION_KEY,
        );
    assert!(check_result.is_ok(), "First backup should be compatible");

    let save_messages_1 = run
        .coordinator
        .tell_device_to_save_physical_backup(physical_backup_phase_1, restoration_id);
    run.extend(save_messages_1);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Second device enters its backup
    let enter_physical_id_2 = EnterPhysicalId::new(&mut test_rng);
    let enter_backup_2 = run
        .coordinator
        .tell_device_to_load_physical_backup(enter_physical_id_2, devices[1]);
    run.extend(enter_backup_2);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let physical_backup_phase_2 = *env
        .physical_backups_entered
        .last()
        .expect("Should have a physical backup phase");

    let save_messages_2 = run
        .coordinator
        .tell_device_to_save_physical_backup(physical_backup_phase_2, restoration_id);
    run.extend(save_messages_2);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Third device enters its backup
    let enter_physical_id_3 = EnterPhysicalId::new(&mut test_rng);
    let enter_backup_3 = run
        .coordinator
        .tell_device_to_load_physical_backup(enter_physical_id_3, devices[2]);
    run.extend(enter_backup_3);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let physical_backup_phase_3 = *env
        .physical_backups_entered
        .last()
        .expect("Should have a physical backup phase");

    let save_messages_3 = run
        .coordinator
        .tell_device_to_save_physical_backup(physical_backup_phase_3, restoration_id);
    run.extend(save_messages_3);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Check that the second device has the threshold in its saved backup
    let device_2 = run.device(devices[1]);
    let saved_backups_2 = device_2.saved_backups();

    let saved_backup_2 = saved_backups_2
        .get(&physical_backup_phase_2.backup.share_image)
        .expect("Second device should have saved the backup");

    assert_eq!(
        saved_backup_2.threshold,
        Some(2),
        "Second device should know the threshold is 2 (discovered through fuzzy recovery when it entered its share)"
    );

    // Check that the third device has the threshold in its saved backup
    let device_3 = run.device(devices[2]);
    let saved_backups_3 = device_3.saved_backups();

    let saved_backup_3 = saved_backups_3
        .get(&physical_backup_phase_3.backup.share_image)
        .expect("Third device should have saved the backup");

    assert_eq!(
        saved_backup_3.threshold,
        Some(2),
        "Third device should know the threshold is 2 (discovered through fuzzy recovery of first two shares)"
    );

    // Now finish the restoration
    let restoration_state = run
        .coordinator
        .get_restoration_state(restoration_id)
        .expect("Restoration should exist");

    assert!(
        restoration_state.is_restorable(),
        "Should be restorable with 2 out of 3 shares"
    );

    let restored_access_structure_ref = run
        .coordinator
        .finish_restoring(restoration_id, TEST_ENCRYPTION_KEY, &mut test_rng)
        .expect("Should finish restoring");

    // Verify the key was restored with the correct threshold
    let _restored_key = run
        .coordinator
        .get_frost_key(restored_access_structure_ref.key_id)
        .expect("Restored key should exist");

    let restored_access_structure = run
        .coordinator
        .get_access_structure(restored_access_structure_ref)
        .expect("Restored access structure should exist");

    assert_eq!(
        restored_access_structure.threshold(),
        2,
        "Restored key should have threshold of 2"
    );

    // Now consolidate the physical backups on all devices
    for &device_id in &devices {
        if run
            .coordinator
            .has_backups_that_need_to_be_consolidated(device_id)
        {
            let consolidate_messages = run
                .coordinator
                .consolidate_pending_physical_backups(device_id, TEST_ENCRYPTION_KEY);
            run.extend(consolidate_messages);
        }
    }

    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Verify all devices now have properly encrypted shares
    for (i, &device_id) in devices.iter().enumerate() {
        assert!(
            !run.coordinator
                .has_backups_that_need_to_be_consolidated(device_id),
            "Device {:?} should have all backups consolidated",
            device_id
        );

        // Verify the device actually has the encrypted share
        let device = run.device(device_id);
        let share_index = backups[i].share_image().index;
        let _encrypted_share = device
            .get_encrypted_share(restored_access_structure_ref, share_index)
            .unwrap_or_else(|| {
                panic!(
                    "Device {:?} should have consolidated share at index {:?}",
                    device_id, share_index
                )
            });
    }

    // Verify the restored access structure matches the original
    assert_eq!(
        restored_access_structure_ref, access_structure_ref,
        "Restored access structure should match original"
    );
}

#[test]
fn deleting_restoration_shares_reverts_state() {
    let mut test_rng = ChaCha20Rng::from_seed([100u8; 32]);
    let mut env = TestEnv::default();

    // Create a 2-of-3 key
    let mut run =
        Run::start_after_keygen_and_nonces(3, 2, &mut env, &mut test_rng, 2, KeyPurpose::Test);

    let device_set = run.device_set();
    let key_data = run.coordinator.iter_keys().next().unwrap();
    let access_structure_ref = key_data
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    // Display backups for all devices
    for &device_id in &device_set {
        let display_backup = run
            .coordinator
            .request_device_display_backup(device_id, access_structure_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(display_backup);
    }
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let backups: Vec<_> = env
        .backups
        .values()
        .map(|(_, backup)| backup.clone())
        .collect();

    // Clear the coordinator and create fresh devices
    run.clear_coordinator();
    let devices: Vec<_> = (0..3).map(|_| run.new_device(&mut test_rng)).collect();

    for (i, &device_id) in devices.iter().enumerate() {
        env.backup_to_enter.insert(device_id, backups[i].clone());
    }

    // Start restoration
    let restoration_id = frostsnap_core::RestorationId::new(&mut test_rng);
    run.coordinator.start_restoring_key(
        "Test Wallet".to_string(),
        None,
        KeyPurpose::Test,
        restoration_id,
    );

    // Add all three shares
    for &device_id in &devices {
        let enter_physical_id = EnterPhysicalId::new(&mut test_rng);
        let enter_backup = run
            .coordinator
            .tell_device_to_load_physical_backup(enter_physical_id, device_id);
        run.extend(enter_backup);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();

        let physical_backup_phase = *env
            .physical_backups_entered
            .last()
            .expect("Should have a physical backup phase");

        let save_messages = run
            .coordinator
            .tell_device_to_save_physical_backup(physical_backup_phase, restoration_id);
        run.extend(save_messages);
        run.run_until_finished(&mut env, &mut test_rng).unwrap();
    }

    // Before removal: we have 3 shares and the key is recovered
    let restoration_state_before = run
        .coordinator
        .get_restoration_state(restoration_id)
        .expect("Restoration should exist");

    assert!(
        restoration_state_before.is_restorable(),
        "Should be restorable with 3 shares"
    );
    assert_eq!(
        restoration_state_before.status().shares.len(),
        3,
        "Should have 3 shares"
    );
    assert!(
        restoration_state_before.status().shared_key.is_some(),
        "Shared key should be recovered"
    );

    // Remove the third share
    run.coordinator
        .delete_restoration_share(restoration_id, devices[2]);

    // After removal: we should still have 2 shares and key still recovered (2-of-3 only needs 2)
    let restoration_state_after_one_removal = run
        .coordinator
        .get_restoration_state(restoration_id)
        .expect("Restoration should exist");

    assert!(
        restoration_state_after_one_removal.is_restorable(),
        "Should still be restorable with 2 shares"
    );
    assert_eq!(
        restoration_state_after_one_removal.status().shares.len(),
        2,
        "Should have 2 shares after removal"
    );
    assert!(
        restoration_state_after_one_removal
            .status()
            .shared_key
            .is_some(),
        "Shared key should still be recovered with 2 shares"
    );

    // Remove another share - now we should lose the shared_key
    run.coordinator
        .delete_restoration_share(restoration_id, devices[1]);

    let restoration_state_after_two_removals = run
        .coordinator
        .get_restoration_state(restoration_id)
        .expect("Restoration should exist");

    assert!(
        !restoration_state_after_two_removals.is_restorable(),
        "Should NOT be restorable with only 1 share (need 2 for 2-of-3)"
    );
    assert_eq!(
        restoration_state_after_two_removals.status().shares.len(),
        1,
        "Should have 1 share after second removal"
    );
    assert!(
        restoration_state_after_two_removals
            .status()
            .shared_key
            .is_none(),
        "Shared key should be None with only 1 share"
    );
}

#[test]
fn consolidate_backup_with_polynomial_checksum_validation() {
    let mut test_rng = ChaCha20Rng::from_seed([43u8; 32]);
    let mut env = TestEnv::default();

    // Do a 2-of-2 keygen to get valid keys
    let mut run =
        Run::start_after_keygen_and_nonces(2, 2, &mut env, &mut test_rng, 2, KeyPurpose::Test);

    let device_set = run.device_set();
    let key_data = run.coordinator.iter_keys().next().unwrap();
    let access_structure_ref = key_data
        .access_structures()
        .next()
        .unwrap()
        .access_structure_ref();

    // Display backups for all devices
    for &device_id in &device_set {
        let display_backup = run
            .coordinator
            .request_device_display_backup(device_id, access_structure_ref, TEST_ENCRYPTION_KEY)
            .unwrap();
        run.extend(display_backup);
    }
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Now we have backups in env.backups
    assert_eq!(env.backups.len(), 2);

    // Pick the first device
    let device_id = *env.backups.keys().next().unwrap();
    let (_, backup) = env.backups.get(&device_id).unwrap().clone();

    // Assign the backup for this device to enter
    env.backup_to_enter.insert(device_id, backup);

    // Tell the device to enter backup mode
    let enter_physical_id = frostsnap_core::EnterPhysicalId::new(&mut test_rng);
    let enter_backup = run
        .coordinator
        .tell_device_to_load_physical_backup(enter_physical_id, device_id);
    run.extend(enter_backup);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // The env should have received an EnterBackup message and we simulate entering it
    // The TestEnv will handle entering the backup we already have for this device

    // Get the PhysicalBackupPhase from the env that was populated when the device responded
    let physical_backup_phase = *env
        .physical_backups_entered
        .last()
        .expect("Should have a physical backup phase");

    let consolidate = run
        .coordinator
        .tell_device_to_consolidate_physical_backup(
            physical_backup_phase,
            access_structure_ref,
            TEST_ENCRYPTION_KEY,
        )
        .unwrap();
    run.extend(consolidate);

    // This should succeed because the polynomial checksum is valid
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Verify that the device now has the consolidated share
    let device = run.device(device_id);
    let _encrypted_share = device
        .get_encrypted_share(
            access_structure_ref,
            physical_backup_phase.backup.share_image.index,
        )
        .expect("Device should have the consolidated share");

    // Verify the coordinator also knows about this share
    assert!(
        run.coordinator.knows_about_share(
            device_id,
            access_structure_ref,
            physical_backup_phase.backup.share_image.index
        ),
        "Coordinator should know about the consolidated share"
    );
}
