/// Tests to ensure backward compatibility of device mutations
mod common;

use frost_backup::ShareBackup;
use frostsnap_core::device::{
    restoration::{RestorationMutation, SavedBackup},
    EncryptedSecretShare, KeyPurpose, Mutation, SaveShareMutation,
};
use frostsnap_core::{AccessStructureId, AccessStructureKind, Kind};
use schnorr_fun::frost::{SecretShare, ShareImage, SharedKey};
use schnorr_fun::fun::prelude::*;

#[test]
fn test_all_device_mutations() {
    // Create test data
    let secret_share = SecretShare {
        index: s!(1).public(),
        share: s!(42).mark_zero(),
    };

    let poly = vec![
        g!(42 * G).normalize().mark_zero(),
        g!(7 * G).normalize().mark_zero(),
    ];

    let shared_key = SharedKey::from_poly(poly).non_zero().unwrap();
    let share_backup = ShareBackup::from_secret_share_and_shared_key(secret_share, &shared_key);

    let share_image = ShareImage {
        index: s!(1).public(),
        image: g!(42 * G).normalize().mark_zero(),
    };

    // Create test EncryptedSecretShare with properly encrypted data
    use frostsnap_core::{Ciphertext, SymmetricKey};
    let test_key = SymmetricKey([77u8; 32]);
    let test_secret_share = s!(99).mark_zero();
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(7777);

    let encrypted_share = EncryptedSecretShare {
        share_image,
        ciphertext: Ciphertext::encrypt(test_key, &test_secret_share, &mut rng),
    };

    // Create all mutation variants we want to test
    let mutations = vec![
        // Keygen mutations
        Mutation::Keygen(frostsnap_core::device::keys::KeyMutation::NewKey {
            key_id: frostsnap_core::KeyId([1u8; 32]),
            key_name: "test_key".to_string(),
            purpose: KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
        }),
        Mutation::Keygen(
            frostsnap_core::device::keys::KeyMutation::NewAccessStructure {
                access_structure_ref: frostsnap_core::AccessStructureRef {
                    key_id: frostsnap_core::KeyId([1u8; 32]),
                    access_structure_id: AccessStructureId([2u8; 32]),
                },
                threshold: 2,
                kind: AccessStructureKind::Master,
            },
        ),
        Mutation::Keygen(frostsnap_core::device::keys::KeyMutation::SaveShare(
            Box::new(SaveShareMutation {
                access_structure_ref: frostsnap_core::AccessStructureRef {
                    key_id: frostsnap_core::KeyId([1u8; 32]),
                    access_structure_id: AccessStructureId([2u8; 32]),
                },
                encrypted_secret_share: encrypted_share,
            }),
        )),
        // Restoration mutations
        Mutation::Restoration(RestorationMutation::Save(SavedBackup {
            share_backup,
            threshold: 2,
            purpose: KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
            key_name: "test_key".to_string(),
        })),
        Mutation::Restoration(RestorationMutation::UnSave(share_image)),
    ];

    // Test each mutation
    for mutation in mutations {
        match mutation.clone() {
            Mutation::Keygen(frostsnap_core::device::keys::KeyMutation::NewKey { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0000010101010101010101010101010101010101010101010101010101010101010108746573745f6b65790100"
                );
            }
            Mutation::Keygen(frostsnap_core::device::keys::KeyMutation::NewAccessStructure {
                ..
            }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0001010101010101010101010101010101010101010101010101010101010101010102020202020202020202020202020202020202020202020202020202020202020200"
                );
            }
            Mutation::Restoration(RestorationMutation::Save(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "01000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002aeb02010008746573745f6b6579"
                );
            }
            Mutation::Restoration(RestorationMutation::UnSave(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0101000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af"
                );
            }
            Mutation::Keygen(frostsnap_core::device::keys::KeyMutation::SaveShare(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "000201010101010101010101010101010101010101010101010101010101010101010202020202020202020202020202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1afb1dde6fd8607b05ecd33fcdf96eaef828be8955ad2af175f7b4f231e83dac8a2a89393d068530505297b93b9dc5b740d59a1ebee4d9a5924acda8cca"
                );
            }
        }
    }
}
