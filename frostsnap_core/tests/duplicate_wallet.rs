//! A wallet must never be restored twice — not while it is already complete on this
//! coordinator, and not while another restoration of it is already in progress.
//!
//! A share reported by a device carries the wallet's identity (`access_structure_ref`), so
//! both are detectable from the very first share: before a restoration holds enough shares
//! to recognise a share image cryptographically, and without decrypting anything.

use common::TEST_ENCRYPTION_KEY;
use frostsnap_core::coordinator::restoration::{
    RecoverShare, RestorationMutation, RestoreRecoverShareError, StartRestorationFromShareError,
};
use frostsnap_core::coordinator::{KeyLocationState, Mutation, ShareSearch};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::HeldShare2;
use frostsnap_core::schnorr_fun::frost::ShareImage;
use frostsnap_core::{
    AccessStructureId, AccessStructureRef, DeviceId, KeyId, RestorationId, SymmetricKey,
};
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use schnorr_fun::fun::prelude::*;

mod common;
mod env;
use crate::common::Run;
use crate::env::TestEnv;

/// A key the wallet was not encrypted under.
const WRONG_ENCRYPTION_KEY: SymmetricKey = SymmetricKey([7u8; 32]);

/// What a device holding `share_image` reports when asked for its shares.
fn device_share(run: &Run, held_by: DeviceId, share_image: ShareImage) -> RecoverShare {
    let key = run.coordinator.iter_keys().next().unwrap();
    let access_structure = key.access_structures().next().unwrap();
    RecoverShare {
        held_by,
        held_share: HeldShare2 {
            access_structure_ref: Some(access_structure.access_structure_ref()),
            share_image,
            threshold: Some(access_structure.threshold()),
            key_name: Some(key.key_name.clone()),
            purpose: Some(key.purpose),
            needs_consolidation: false,
        },
    }
}

/// One device share per device of the coordinator's (only) wallet.
fn device_shares(run: &Run) -> Vec<RecoverShare> {
    run.coordinator
        .iter_shares(TEST_ENCRYPTION_KEY)
        .map(|(share_image, location)| device_share(run, location.device_ids[0], share_image))
        .collect()
}

/// Create a restoration holding `share` directly, the way one already in the database would
/// look — bypassing the checks under test.
fn restoration_holding(
    run: &mut Run,
    share: &RecoverShare,
    rng: &mut ChaCha20Rng,
) -> RestorationId {
    let restoration_id = RestorationId::new(rng);
    run.coordinator.mutate(Mutation::Restoration(
        RestorationMutation::NewRestoration2 {
            restoration_id,
            key_name: share.held_share.key_name.clone().unwrap(),
            starting_threshold: share.held_share.threshold,
            key_purpose: share.held_share.purpose.unwrap(),
        },
    ));
    run.coordinator.mutate(Mutation::Restoration(
        RestorationMutation::RestorationProgress2 {
            restoration_id,
            device_id: share.held_by,
            held_share: share.held_share.clone(),
        },
    ));
    restoration_id
}

/// The same share as reported by a device that does not know which wallet it belongs to:
/// a physical backup it has entered but not yet consolidated.
fn without_wallet_identity(share: &RecoverShare) -> RecoverShare {
    let mut share = share.clone();
    share.held_share.access_structure_ref = None;
    share.held_share.needs_consolidation = true;
    share
}

#[test]
fn a_second_restoration_of_the_same_wallet_is_refused() {
    let mut rng = ChaCha20Rng::from_seed([21u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);
    let shares = device_shares(&run);

    run.clear_coordinator();

    // The first device starts a restoration.
    let messages = run.coordinator.request_held_shares(shares[0].held_by);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut rng).unwrap();
    let restoration = run.coordinator.restoring().next().unwrap();
    let restoration_id = restoration.restoration_id;
    // One share of a 2-of-3: the restoration cannot yet recognise the others by their image.
    assert!(!restoration.is_restorable());

    // The second device's share is nonetheless known to belong to that restoration ...
    let second = &shares[1];
    match run.coordinator.find_share(
        second.held_share.share_image,
        second.held_share.access_structure_ref,
        TEST_ENCRYPTION_KEY,
    ) {
        ShareSearch::Found(location) => assert_eq!(
            location.key_state,
            KeyLocationState::Restoring { restoration_id }
        ),
        other => panic!("expected the share to be located in the restoration, got {other:?}"),
    }

    // ... so it cannot start a second restoration of the same wallet.
    let err = run
        .coordinator
        .start_restoring_key_from_recover_share(second, RestorationId::new(&mut rng))
        .unwrap_err();
    match err {
        StartRestorationFromShareError::ShareBelongsElsewhere { location } => assert_eq!(
            location.key_state,
            KeyLocationState::Restoring { restoration_id }
        ),
        other => panic!("expected ShareBelongsElsewhere, got {other:?}"),
    }
    assert_eq!(run.coordinator.restoring().count(), 1);
}

/// Used to trip an `assert!` in `start_restoring_key_from_recover_share`.
#[test]
fn restoring_a_wallet_that_already_exists_is_refused() {
    let mut rng = ChaCha20Rng::from_seed([22u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);
    let shares = device_shares(&run);
    let access_structure_ref = shares[0].held_share.access_structure_ref.unwrap();

    let err = run
        .coordinator
        .start_restoring_key_from_recover_share(&shares[0], RestorationId::new(&mut rng))
        .unwrap_err();
    match err {
        StartRestorationFromShareError::ShareBelongsElsewhere { location } => assert_eq!(
            location.key_state,
            KeyLocationState::Complete {
                access_structure_ref
            }
        ),
        other => panic!("expected ShareBelongsElsewhere, got {other:?}"),
    }
    assert_eq!(run.coordinator.restoring().count(), 0);
}

/// `find_share` used to answer "not found" for a wallet it could not unlock, which read as
/// "safe to restore" to every caller.
#[test]
fn a_locked_wallet_is_reported_rather_than_ignored() {
    let mut rng = ChaCha20Rng::from_seed([23u8; 32]);
    let mut env = TestEnv::default();
    let run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);
    let key_name = run.coordinator.iter_keys().next().unwrap().key_name.clone();
    let share = &device_shares(&run)[0];
    let share_image = share.held_share.share_image;
    let access_structure_ref = share.held_share.access_structure_ref.unwrap();

    // Without the wallet's identity the share can only be recognised by unlocking the wallet.
    assert_eq!(
        run.coordinator
            .find_share(share_image, None, WRONG_ENCRYPTION_KEY),
        ShareSearch::CouldNotCheck {
            locked_key_names: vec![key_name]
        }
    );

    // With it nothing needs unlocking.
    match run.coordinator.find_share(
        share_image,
        Some(access_structure_ref),
        WRONG_ENCRYPTION_KEY,
    ) {
        ShareSearch::Found(location) => assert_eq!(
            location.key_state,
            KeyLocationState::Complete {
                access_structure_ref
            }
        ),
        other => panic!("expected the share to be located by its wallet's identity, got {other:?}"),
    }

    // And the right key still finds it by its image, as before.
    match run
        .coordinator
        .find_share(share_image, None, TEST_ENCRYPTION_KEY)
    {
        ShareSearch::Found(location) => assert_eq!(location.device_ids, vec![share.held_by]),
        other => panic!("expected the share to be located by its image, got {other:?}"),
    }
}

/// A wallet that cannot be unlocked must not get in the way of restoring some other wallet:
/// a device share names its wallet, and that decides the question without unlocking anything.
#[test]
fn a_locked_wallet_does_not_block_a_share_of_some_other_wallet() {
    let mut rng = ChaCha20Rng::from_seed([24u8; 32]);
    let mut env = TestEnv::default();
    let run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);

    let other_wallet = AccessStructureRef {
        key_id: KeyId([1u8; 32]),
        access_structure_id: AccessStructureId([2u8; 32]),
    };
    let other_share_image = ShareImage {
        index: s!(5).public(),
        image: g!(11 * G).normalize().mark_zero(),
    };

    assert_eq!(
        run.coordinator
            .find_share(other_share_image, Some(other_wallet), WRONG_ENCRYPTION_KEY),
        ShareSearch::NotFound
    );
    // Without the identity the locked wallet cannot be ruled out.
    assert!(matches!(
        run.coordinator
            .find_share(other_share_image, None, WRONG_ENCRYPTION_KEY),
        ShareSearch::CouldNotCheck { .. }
    ));
}

/// A share already entered into one restoration is reported there, even when the device
/// names a wallet that another restoration is recovering.
#[test]
fn a_share_held_by_one_restoration_cannot_be_added_to_another() {
    let mut rng = ChaCha20Rng::from_seed([25u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);
    let shares = device_shares(&run);
    run.clear_coordinator();

    // Restoration A starts from device 0, so it knows the wallet.
    let messages = run.coordinator.request_held_shares(shares[0].held_by);
    run.extend(messages);
    run.run_until_finished(&mut env, &mut rng).unwrap();
    let restoration_a = run.coordinator.restoring().next().unwrap().restoration_id;
    // Restoration B holds device 1's share as a physical backup, so it does not.
    let restoration_b =
        restoration_holding(&mut run, &without_wallet_identity(&shares[1]), &mut rng);

    // Device 1 consolidates and now names the wallet: its share is still B's.
    let second = &shares[1];
    match run.coordinator.find_share(
        second.held_share.share_image,
        second.held_share.access_structure_ref,
        TEST_ENCRYPTION_KEY,
    ) {
        ShareSearch::Found(location) => {
            assert_eq!(
                location.key_state,
                KeyLocationState::Restoring {
                    restoration_id: restoration_b
                }
            );
            assert_eq!(location.device_ids, vec![second.held_by]);
        }
        other => panic!("expected the share to be located in restoration B, got {other:?}"),
    }
    let err = run
        .coordinator
        .check_recover_share_compatible_with_restoration(restoration_a, second, TEST_ENCRYPTION_KEY)
        .unwrap_err();
    assert!(
        matches!(&err, RestoreRecoverShareError::ShareBelongsElsewhere { location }
            if location.key_state == KeyLocationState::Restoring { restoration_id: restoration_b }),
        "expected ShareBelongsElsewhere naming restoration B, got {err:?}"
    );
}

/// A share that names no wallet is still recognised as already being restored, by its image.
#[test]
fn a_second_restoration_from_a_share_without_wallet_identity_is_refused() {
    let mut rng = ChaCha20Rng::from_seed([26u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);
    let shares = device_shares(&run);
    run.clear_coordinator();

    let share = without_wallet_identity(&shares[0]);
    let restoration_id = restoration_holding(&mut run, &share, &mut rng);

    let err = run
        .coordinator
        .start_restoring_key_from_recover_share(&share, RestorationId::new(&mut rng))
        .unwrap_err();
    match err {
        StartRestorationFromShareError::ShareBelongsElsewhere { location } => assert_eq!(
            location.key_state,
            KeyLocationState::Restoring { restoration_id }
        ),
        other => panic!("expected ShareBelongsElsewhere, got {other:?}"),
    }
    assert_eq!(run.coordinator.restoring().count(), 1);
}

/// Two restorations of one wallet could exist before this was prevented. Each must still
/// be able to continue: the other's claim to the wallet is not a reason to refuse a share.
#[test]
fn a_restoration_that_knows_its_wallet_is_not_blocked_by_a_duplicate_claiming_it() {
    let mut rng = ChaCha20Rng::from_seed([27u8; 32]);
    let mut env = TestEnv::default();
    let mut run = Run::start_after_keygen(3, 2, &mut env, &mut rng, KeyPurpose::Test);
    let shares = device_shares(&run);
    run.clear_coordinator();

    let first = restoration_holding(&mut run, &shares[0], &mut rng);
    let second = restoration_holding(&mut run, &shares[1], &mut rng);
    assert_eq!(run.coordinator.restoring().count(), 2);

    for restoration_id in [first, second] {
        run.coordinator
            .check_recover_share_compatible_with_restoration(
                restoration_id,
                &shares[2],
                TEST_ENCRYPTION_KEY,
            )
            .unwrap_or_else(|err| {
                panic!("device 2 should be able to join either restoration: {err}")
            });
    }
}
