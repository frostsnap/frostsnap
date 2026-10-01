use common::{TEST_ENCRYPTION_KEY, TEST_FINGERPRINT};
use frost_backup::ShareBackup;
use frostsnap_core::coordinator::restoration::{
    RecoverShare, RestorationError, RestorationMutation, RestorePhysicalBackupError,
};
use frostsnap_core::coordinator::{KeyLocationState, Mutation};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::HeldShare2;
use frostsnap_core::{AccessStructureRef, DeviceId, EnterPhysicalId, RestorationId};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::fun::Scalar;

mod common;
mod env;
use crate::common::Run;
use crate::env::TestEnv;

fn held_share(
    backup: &ShareBackup,
    claimed_access_structure: Option<AccessStructureRef>,
) -> HeldShare2 {
    HeldShare2 {
        access_structure_ref: claimed_access_structure,
        share_image: backup.share_image(),
        threshold: Some(2),
        key_name: Some("Savings".into()),
        purpose: Some(KeyPurpose::Test),
        needs_consolidation: claimed_access_structure.is_none(),
    }
}

fn start(run: &mut Run, rng: &mut ChaCha20Rng) -> RestorationId {
    let restoration_id = RestorationId::new(rng);
    run.coordinator.start_restoring_key(
        "Savings".into(),
        Some(2),
        KeyPurpose::Test,
        restoration_id,
    );
    restoration_id
}

#[test]
fn a_backup_being_saved_counts_as_held() {
    let mut rng = ChaCha20Rng::from_seed([41u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::generate(0, &mut rng);
    let (backups, _) =
        ShareBackup::generate_shares(Scalar::random(&mut rng), 2, 2, TEST_FINGERPRINT, &mut rng);
    let mut entered = vec![];
    for _ in 0..2 {
        let device_id = run.new_device(&mut rng);
        env.backup_to_enter.insert(device_id, backups[0].clone());
        let enter = run
            .coordinator
            .tell_device_to_load_physical_backup(EnterPhysicalId::new(&mut rng), device_id);
        run.extend(enter);
        run.run_until_finished(&mut env, &mut rng).unwrap();
        entered.push(*env.physical_backups_entered.last().unwrap());
    }
    let first = start(&mut run, &mut rng);
    let second = start(&mut run, &mut rng);

    // The device never confirms, so the save stays pending.
    let _ = run
        .coordinator
        .tell_device_to_save_physical_backup(entered[0], first);

    match run
        .coordinator
        .check_physical_backup_compatible_with_restoration(second, entered[1], TEST_ENCRYPTION_KEY)
    {
        Err(RestorePhysicalBackupError::ShareBelongsElsewhere { location }) => {
            assert_eq!(
                location.key_state,
                KeyLocationState::Restoring {
                    restoration_id: first
                }
            );
            assert_eq!(location.device_ids, vec![entered[0].from]);
        }
        other => panic!("expected ShareBelongsElsewhere, got {other:?}"),
    }
}

#[test]
fn a_share_naming_a_wallet_being_restored_is_found_there() {
    let mut rng = ChaCha20Rng::from_seed([42u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);
    let claimed_access_structure = run
        .coordinator
        .iter_access_structures()
        .next()
        .unwrap()
        .access_structure_ref();
    let (backups, _) =
        ShareBackup::generate_shares(Scalar::random(&mut rng), 2, 2, TEST_FINGERPRINT, &mut rng);
    run.clear_coordinator();
    let first = RestorationId::new(&mut rng);
    run.coordinator
        .start_restoring_key_from_recover_share(
            &RecoverShare {
                held_by: DeviceId([90; 33]),
                held_share: held_share(&backups[0], Some(claimed_access_structure)),
            },
            first,
        )
        .unwrap();

    let location = run
        .coordinator
        .find_share(
            backups[1].share_image(),
            Some(claimed_access_structure),
            TEST_ENCRYPTION_KEY,
        )
        .unwrap();
    assert_eq!(
        location.key_state,
        KeyLocationState::Restoring {
            restoration_id: first
        }
    );
    assert!(run
        .coordinator
        .find_share(backups[1].share_image(), None, TEST_ENCRYPTION_KEY)
        .is_none());
}

/// Restorations from before the duplicate checks, written straight to state, each restoring the
/// same key under a different access structure.
#[test]
fn a_restoration_of_a_key_the_app_has_is_blocked_and_cannot_finish() {
    let mut rng = ChaCha20Rng::from_seed([43u8; 32]);
    let mut run = Run::generate(0, &mut rng);
    let secret = Scalar::random(&mut rng);
    let mut legacy = vec![];
    for device in [90u8, 92] {
        let (backups, _) = ShareBackup::generate_shares(secret, 2, 2, TEST_FINGERPRINT, &mut rng);
        let restoration_id = start(&mut run, &mut rng);
        for (i, backup) in backups.iter().enumerate() {
            run.coordinator.mutate(Mutation::Restoration(
                RestorationMutation::RestorationProgress2 {
                    restoration_id,
                    device_id: DeviceId([device + i as u8; 33]),
                    held_share: held_share(backup, None),
                },
            ));
        }
        legacy.push(restoration_id);
    }
    assert_eq!(
        run.coordinator.restored_wallet_already_exists(legacy[1]),
        None
    );

    let restored = run
        .coordinator
        .finish_restoring(legacy[0], TEST_ENCRYPTION_KEY, &mut rng)
        .unwrap();
    assert_eq!(
        run.coordinator.restored_wallet_already_exists(legacy[1]),
        Some("Savings".into())
    );
    let err = run
        .coordinator
        .finish_restoring(legacy[1], TEST_ENCRYPTION_KEY, &mut rng)
        .unwrap_err();
    assert!(matches!(err, RestorationError::AlreadyExists { .. }));
    assert!(run.coordinator.get_restoration_state(legacy[1]).is_some());
    let key = run.coordinator.get_frost_key(restored.key_id).unwrap();
    assert_eq!(key.access_structures().count(), 1);
}
