use common::TEST_ENCRYPTION_KEY;
use frostsnap_core::coordinator::KeyLocationState;
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::HeldShare2;
use frostsnap_core::DeviceId;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;

mod common;
mod env;
use crate::common::Run;
use crate::env::TestEnv;

/// Tests that find_share correctly locates a share in a complete wallet held by a single device.
///
/// Setup: Generate a 2-of-3 key.
/// Action: Query for a share that exists on one device in the complete wallet.
/// Expected: find_share returns the ShareLocation with:
/// - device_ids containing only that one device
/// - KeyLocationState::Complete with the correct access_structure_ref
/// - The correct share_index and key_name
#[test]
fn test_find_share_in_complete_wallet_single_device() {
    let mut test_rng = ChaCha20Rng::from_seed([2u8; 32]);
    let mut env = TestEnv::default();
    let run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    let key_data = run.coordinator.iter_keys().next().unwrap().clone();
    let access_structure = key_data.access_structures().next().unwrap();
    let access_structure_ref = access_structure.access_structure_ref();

    // Use iter_shares to get a share from the complete wallet
    let (share_image, expected_location) = run
        .coordinator
        .iter_shares(TEST_ENCRYPTION_KEY)
        .next()
        .unwrap();

    // Verify the location from iter_shares
    assert_eq!(expected_location.device_ids.len(), 1);
    let first_device_id = expected_location.device_ids[0];

    // Now test find_share
    let result = run.coordinator.find_share(share_image, TEST_ENCRYPTION_KEY);

    assert!(result.is_some(), "Should find share in complete wallet");
    let location = result.unwrap();
    assert_eq!(location.key_name, key_data.key_name);
    assert_eq!(location.device_ids, vec![first_device_id]);
    assert_eq!(location.share_index, share_image.index);
    match location.key_state {
        KeyLocationState::Complete {
            access_structure_ref: found_ref,
        } => {
            assert_eq!(found_ref, access_structure_ref);
        }
        _ => panic!("Expected Complete key state"),
    }
}

/// Tests that find_share returns all devices holding the same share in a complete wallet.
///
/// Setup: Generate a 2-of-3 key, then use recover_share to restore the same physical backup
///        onto a second device (simulating loading a backup onto multiple devices).
/// Action: Query for the share that now exists on two different devices.
/// Expected: find_share returns ShareLocation with device_ids containing both devices,
///           since the same share (same ShareImage) exists on multiple devices.
#[test]
fn test_find_share_in_complete_wallet_multiple_devices() {
    let mut test_rng = ChaCha20Rng::from_seed([3u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    let key_data = run.coordinator.iter_keys().next().unwrap().clone();
    let access_structure = key_data.access_structures().next().unwrap();
    let access_structure_ref = access_structure.access_structure_ref();

    // Use iter_shares to get a share from the complete wallet
    let (share_image, location) = run
        .coordinator
        .iter_shares(TEST_ENCRYPTION_KEY)
        .next()
        .unwrap();
    let first_device_id = location.device_ids[0];
    let share_index = share_image.index;

    // Create a new device (not part of the original keygen) to simulate
    // loading a physical backup onto a fresh device
    let new_device = DeviceId([99u8; 33]);

    // Create the recover share for the second device with the same share
    let held_share = HeldShare2 {
        access_structure_ref: Some(access_structure_ref),
        share_image,
        key_name: Some(key_data.key_name.clone()),
        purpose: Some(key_data.purpose),
        threshold: Some(access_structure.threshold()),
        needs_consolidation: false,
    };

    let recover_share = frostsnap_core::coordinator::restoration::RecoverShare {
        held_by: new_device,
        held_share,
    };

    // Add the share to the existing access structure
    run.coordinator
        .recover_share(access_structure_ref, &recover_share, TEST_ENCRYPTION_KEY)
        .expect("should be able to add duplicate share");

    // Now find_share should return both devices
    let result = run.coordinator.find_share(share_image, TEST_ENCRYPTION_KEY);

    assert!(result.is_some());
    let location = result.unwrap();
    assert_eq!(
        location.device_ids.len(),
        2,
        "Should find share on two devices"
    );
    assert!(location.device_ids.contains(&first_device_id));
    assert!(location.device_ids.contains(&new_device));
    assert_eq!(location.share_index, share_index);
    match location.key_state {
        KeyLocationState::Complete {
            access_structure_ref: found_ref,
        } => {
            assert_eq!(found_ref, access_structure_ref);
        }
        _ => panic!("Expected Complete key state"),
    }
}

/// Tests that find_share locates a virtual share in a complete wallet.
///
/// Setup: Generate a 2-of-3 key where devices have indices 1, 2, 3.
/// Action: Compute a share_image for a valid but unassigned index (e.g., 4).
/// Expected: find_share returns ShareLocation with:
/// - device_ids EMPTY (no device has this share)
/// - KeyLocationState::Complete with the correct access_structure_ref
/// - The share exists mathematically via the root_shared_key
#[test]
fn test_find_share_virtual_in_complete_wallet() {
    let mut test_rng = ChaCha20Rng::from_seed([9u8; 32]);
    let mut env = TestEnv::default();
    let run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    let key_data = run.coordinator.iter_keys().next().unwrap().clone();
    let access_structure = key_data.access_structures().next().unwrap();
    let access_structure_ref = access_structure.access_structure_ref();

    // Get the root shared key to compute a virtual share
    let root_shared_key = run
        .coordinator
        .root_shared_key(access_structure_ref, TEST_ENCRYPTION_KEY)
        .unwrap();

    // Find an index that no device has
    let assigned_indices: std::collections::HashSet<_> = access_structure
        .device_to_share_indicies()
        .values()
        .cloned()
        .collect();

    // Use index 4 which won't be assigned in a 2-of-3 key (devices get 1, 2, 3)
    let virtual_index =
        schnorr_fun::fun::Scalar::from(core::num::NonZeroU32::new(4).expect("4 is non-zero"));
    assert!(!assigned_indices.contains(&virtual_index));

    let virtual_share_image = root_shared_key.share_image(virtual_index);

    // Find this virtual share
    let result = run
        .coordinator
        .find_share(virtual_share_image, TEST_ENCRYPTION_KEY);

    assert!(
        result.is_some(),
        "Should find virtual share in complete wallet"
    );
    let location = result.unwrap();
    assert!(
        location.device_ids.is_empty(),
        "Virtual share should have no device_ids"
    );
    assert_eq!(location.share_index, virtual_index);
    assert_eq!(location.key_name, key_data.key_name);
    match location.key_state {
        KeyLocationState::Complete {
            access_structure_ref: found_ref,
        } => {
            assert_eq!(found_ref, access_structure_ref);
        }
        _ => panic!("Expected Complete key state"),
    }
}

/// Tests that find_share locates a share that's been physically collected during restoration.
///
/// Setup: Generate a key, clear the coordinator, then start restoration by requesting
///        a held share from one device.
/// Action: Query for the share that was physically provided by the device.
/// Expected: find_share returns ShareLocation with:
/// - device_ids containing the device that provided the share
/// - KeyLocationState::Restoring with the restoration_id
/// - The share is "physical" because a device actually sent it
#[test]
fn test_find_share_in_restoration_physical() {
    let mut test_rng = ChaCha20Rng::from_seed([4u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    let device_set = run.device_set();

    // Clear coordinator to start restoration
    run.clear_coordinator();

    // Request shares from first device
    let first_device = device_set.iter().next().cloned().unwrap();
    let messages = run.coordinator.request_held_shares(first_device);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let restoration = run.coordinator.restoring().next().unwrap();
    let held_share = &restoration.access_structure.held_shares[0];
    let share_image = held_share.held_share.share_image;

    let result = run.coordinator.find_share(share_image, TEST_ENCRYPTION_KEY);

    assert!(result.is_some(), "Should find share in restoration");
    let location = result.unwrap();
    assert_eq!(location.device_ids, vec![first_device]);
    assert_eq!(location.share_index, share_image.index);
    assert_eq!(location.key_name, restoration.key_name);
    match location.key_state {
        KeyLocationState::Restoring { restoration_id } => {
            assert_eq!(restoration_id, restoration.restoration_id);
        }
        _ => panic!("Expected Restoring key state"),
    }
}

/// Tests that find_share locates a "virtual" share via the cached SharedKey in a restoration.
///
/// Setup: Generate a 2-of-3 key, clear the coordinator, then restore
///        by collecting shares from only 2 devices (meeting the threshold).
/// Action: Query for the share from the third device that was NOT physically collected.
/// Expected: find_share returns ShareLocation with:
/// - device_ids EMPTY (no device physically provided this share)
/// - KeyLocationState::Restoring with the restoration_id
/// - The share is "virtual" because fuzzy recovery succeeded and cached the SharedKey,
///   so we can compute that this ShareImage exists mathematically, even though no
///   device has provided it yet.
#[test]
fn test_find_share_in_restoration_virtual() {
    let mut test_rng = ChaCha20Rng::from_seed([5u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    // Get all shares before clearing coordinator
    let all_shares: Vec<_> = run.coordinator.iter_shares(TEST_ENCRYPTION_KEY).collect();
    assert_eq!(all_shares.len(), 3, "Should have 3 shares in 2-of-3 key");

    let device_set = run.device_set();

    // Clear coordinator to start restoration
    run.clear_coordinator();

    // Request shares from only 2 devices (threshold is 2)
    let restoring_devices: Vec<_> = device_set.iter().cloned().take(2).collect();
    for device_id in &restoring_devices {
        let messages = run.coordinator.request_held_shares(*device_id);
        run.extend(messages);
    }
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let restoration = run.coordinator.restoring().next().unwrap();

    // The restoration should have recovered (threshold met)
    assert!(restoration.is_restorable());

    // Find a share from the third device that wasn't included in the restoration
    let third_device = device_set.iter().cloned().nth(2).unwrap();
    let (third_device_share_image, _) = all_shares
        .iter()
        .find(|(_, loc)| loc.device_ids.contains(&third_device))
        .unwrap();
    let third_device_share_image = *third_device_share_image;
    let third_device_share_index = third_device_share_image.index;

    // This share should be found "virtually" - we know it exists via the recovered SharedKey
    // but no device has physically provided it to this coordinator
    let result = run
        .coordinator
        .find_share(third_device_share_image, TEST_ENCRYPTION_KEY);

    assert!(
        result.is_some(),
        "Should find share virtually via cached SharedKey"
    );
    let location = result.unwrap();
    assert!(
        location.device_ids.is_empty(),
        "Virtual share should have no device_ids"
    );
    assert_eq!(location.share_index, third_device_share_index);
    assert_eq!(location.key_name, restoration.key_name);
}

/// Tests that find_share detects when trying to add a duplicate share to the same restoration.
///
/// Setup: Generate a key, clear the coordinator, start restoration with one device's share.
/// Action: Query for the share that was just added to the restoration.
/// Expected: find_share returns the ShareLocation pointing to the ongoing restoration,
///           which allows the caller to detect "this share is already in this restoration"
///           and prevent duplicate addition.
#[test]
fn test_find_share_duplicate_same_restoration() {
    let mut test_rng = ChaCha20Rng::from_seed([6u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    let device_set = run.device_set();
    run.clear_coordinator();

    // Add first device's share
    let first_device = device_set.iter().next().cloned().unwrap();
    let messages = run.coordinator.request_held_shares(first_device);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let restoration = run.coordinator.restoring().next().unwrap();
    let share_image = restoration.access_structure.held_shares[0]
        .held_share
        .share_image;

    // Try to find this share (which already exists in the restoration)
    let result = run.coordinator.find_share(share_image, TEST_ENCRYPTION_KEY);

    assert!(result.is_some(), "Should find share already in restoration");
    let location = result.unwrap();
    match location.key_state {
        KeyLocationState::Restoring { restoration_id } => {
            assert_eq!(restoration_id, restoration.restoration_id);
        }
        _ => panic!("Expected Restoring key state"),
    }
    assert_eq!(location.device_ids, vec![first_device]);
}

/// Tests that find_share detects conflicts when a share exists in one restoration and
/// someone tries to add it to a different restoration.
///
/// Setup: Generate a key, clear coordinator, start first restoration with device 1,
///        then start second restoration with device 2.
/// Action: Query for the share from device 1 (which is in first restoration).
/// Expected: find_share returns the ShareLocation pointing to the FIRST restoration,
///           not the second one. This allows detecting cross-restoration conflicts.
#[test]
fn test_find_share_conflict_different_restorations() {
    let mut test_rng = ChaCha20Rng::from_seed([7u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    let device_set = run.device_set();
    run.clear_coordinator();

    // Start first restoration with device 1
    let first_device = device_set.iter().next().cloned().unwrap();
    let messages = run.coordinator.request_held_shares(first_device);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    let first_restoration = run.coordinator.restoring().next().unwrap();
    let share_image = first_restoration.access_structure.held_shares[0]
        .held_share
        .share_image;
    let first_restoration_id = first_restoration.restoration_id;

    // Start a second restoration with device 2
    let second_device = device_set.iter().nth(1).cloned().unwrap();
    let messages = run.coordinator.request_held_shares(second_device);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Now try to find the share from first restoration
    // It should be found in the first restoration
    let result = run.coordinator.find_share(share_image, TEST_ENCRYPTION_KEY);

    assert!(result.is_some(), "Should find share in first restoration");
    let location = result.unwrap();
    match location.key_state {
        KeyLocationState::Restoring { restoration_id } => {
            assert_eq!(
                restoration_id, first_restoration_id,
                "Should find in first restoration, not second"
            );
        }
        _ => panic!("Expected Restoring key state"),
    }
}

/// Tests that find_share prioritizes complete wallets over ongoing restorations.
///
/// Setup: Generate a complete key, then start a restoration with a different device
///        (simulating trying to restore while some keys already exist).
/// Action: Query for a share that exists in the complete wallet.
/// Expected: find_share returns ShareLocation pointing to the Complete wallet, not the
///           ongoing restoration. This allows detecting "this share already exists in a
///           complete wallet, don't try to restore it into a new wallet."
#[test]
fn test_find_share_conflict_complete_vs_restoration() {
    let mut test_rng = ChaCha20Rng::from_seed([8u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut test_rng, KeyPurpose::Test);

    let key_data = run.coordinator.iter_keys().next().unwrap().clone();
    let access_structure = key_data.access_structures().next().unwrap();
    let access_structure_ref = access_structure.access_structure_ref();

    // Use iter_shares to get a share from the complete wallet
    let (share_image, location) = run
        .coordinator
        .iter_shares(TEST_ENCRYPTION_KEY)
        .next()
        .unwrap();
    let first_device = location.device_ids[0];

    let device_set = run.device_set();

    // Now start a restoration with a different device
    let second_device = device_set
        .iter()
        .find(|&&d| d != first_device)
        .cloned()
        .unwrap();
    let messages = run.coordinator.request_held_shares(second_device);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut test_rng).unwrap();

    // Try to find the first device's share - it should be found in the complete wallet
    let result = run.coordinator.find_share(share_image, TEST_ENCRYPTION_KEY);

    assert!(
        result.is_some(),
        "Should find share in complete wallet even with restoration ongoing"
    );
    let location = result.unwrap();
    match location.key_state {
        KeyLocationState::Complete {
            access_structure_ref: found_ref,
        } => {
            assert_eq!(found_ref, access_structure_ref);
        }
        _ => panic!("Expected Complete key state, not restoration"),
    }
    assert_eq!(location.device_ids, vec![first_device]);
}
