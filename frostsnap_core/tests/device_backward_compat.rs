/// Tests to ensure backward compatibility of device mutations
mod common;

use frost_backup::ShareBackup;
use frostsnap_core::device::{
    card::{CardMutation, ShareOrigin, VerdictReason},
    restoration::*,
    EncryptedSecretShare, KeyPurpose, Mutation, SaveShareMutation,
};
use frostsnap_core::message::card::{CardRefusal, CardSealed};
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
            share_backup: share_backup.clone(),
            threshold: 2,
            purpose: KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
            key_name: "test_key".to_string(),
        })),
        Mutation::Restoration(RestorationMutation::Save2(SavedBackup2 {
            share_backup,
            threshold: Some(2),
            purpose: Some(KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin)),
            key_name: Some("test_key".to_string()),
        })),
        Mutation::Restoration(RestorationMutation::_UnSave(share_image)),
        Mutation::Restoration(RestorationMutation::Save3(SavedBackup3 {
            backup: PhysicalBackup::Card {
                share: WipedShare(secret_share),
                threshold: Some(2),
            },
            threshold: Some(2),
            purpose: Some(KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin)),
            key_name: Some("test_key".to_string()),
        })),
        // Card mutations
        Mutation::Card(CardMutation::Origin {
            share_image,
            origin: ShareOrigin::Keygen { card: true },
        }),
        Mutation::Card(CardMutation::CardChecked { share_image }),
        Mutation::Card(CardMutation::PhoneRound {
            share_image,
            n_tag: [5u8; 32],
            d: [1, 2, 3, 4, 5],
        }),
        Mutation::Card(CardMutation::OpenRun { share_image, j: 3 }),
        Mutation::Card(CardMutation::Released { share_image, j: 3 }),
        Mutation::Card(CardMutation::Verdict {
            share_image,
            passed: true,
            reason: VerdictReason::PhoneRoundPassed,
        }),
        Mutation::Card(CardMutation::Verdict {
            share_image,
            passed: false,
            reason: VerdictReason::DeviceRoundFailed,
        }),
        Mutation::Card(CardMutation::DeviceRoundStarted {
            share_image,
            e: [0, 12, 31, 7, 20],
        }),
        Mutation::Card(CardMutation::DeviceRoundStep {
            share_image,
            k: 2,
            attempt: 1,
        }),
        Mutation::Card(CardMutation::DeviceRoundResult {
            share_image,
            pass: false,
        }),
        Mutation::Card(CardMutation::SplitWriteStarted { share_image }),
        Mutation::Card(CardMutation::DeviceHalfWritten { share_image }),
        Mutation::Card(CardMutation::SealedC {
            share_image,
            sealed: CardSealed {
                format_id: 3,
                h_c: [5u8; 32],
                coin_commit: [6u8; 32],
            },
            words_hash: [7u8; 32],
        }),
        Mutation::Card(CardMutation::ProvingStarted { share_image }),
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
            Mutation::Restoration(RestorationMutation::Save2(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "01020000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002aeb01020101000108746573745f6b6579"
                );
            }
            Mutation::Restoration(RestorationMutation::_UnSave(_)) => {
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
            Mutation::Restoration(RestorationMutation::Save3(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0103010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002a010201020101000108746573745f6b6579"
                );
            }
            Mutation::Card(CardMutation::Origin { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0200000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af0001"
                );
            }
            Mutation::Card(CardMutation::CardChecked { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0201000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af"
                );
            }
            Mutation::Card(CardMutation::PhoneRound { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0202000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af05050505050505050505050505050505050505050505050505050505050505050102030405"
                );
            }
            Mutation::Card(CardMutation::OpenRun { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0203000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af03"
                );
            }
            Mutation::Card(CardMutation::Released { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0204000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af03"
                );
            }
            Mutation::Card(CardMutation::Verdict {
                reason: VerdictReason::DeviceRoundFailed,
                ..
            }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0205000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af0004"
                );
            }
            Mutation::Card(CardMutation::Verdict { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0205000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af0100"
                );
            }
            Mutation::Card(CardMutation::DeviceRoundStarted { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0206000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af000c1f0714"
                );
            }
            Mutation::Card(CardMutation::DeviceRoundStep { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0207000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af0201"
                );
            }
            Mutation::Card(CardMutation::DeviceRoundResult { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0208000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af00"
                );
            }
            Mutation::Card(CardMutation::SplitWriteStarted { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0209000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af"
                );
            }
            Mutation::Card(CardMutation::DeviceHalfWritten { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "020a000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af"
                );
            }
            Mutation::Card(CardMutation::SealedC { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "020b000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af03050505050505050505050505050505050505050505050505050505050505050506060606060606060606060606060606060606060606060606060606060606060707070707070707070707070707070707070707070707070707070707070707"
                );
            }
            Mutation::Card(CardMutation::ProvingStarted { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "020c000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af"
                );
            }
        }
    }
}

/// Spec PK.6 (2), PK.3: a C′ record is sealed with `SealedC`. A `SealedC` of another format
/// replays as `OtherFormat`; a `SealedC` of format 3 seals the record with its words hash.
#[test]
fn seals_of_other_formats_replay_as_other_format() {
    use frostsnap_core::device::{card::CardState, FrostSigner};
    use rand::{rngs::StdRng, SeedableRng};
    let share_image = ShareImage {
        index: s!(1).public(),
        image: g!(42 * G).normalize().mark_zero(),
    };
    let sealed = |format_id| CardSealed {
        format_id,
        h_c: [1; 32],
        coin_commit: [2; 32],
    };
    let replayed = |mutations: Vec<CardMutation>| {
        let mut device =
            FrostSigner::new_random_with_nonce_batch_size(&mut StdRng::seed_from_u64(1), 8, 10);
        device.apply_mutation(Mutation::Card(CardMutation::Origin {
            share_image,
            origin: ShareOrigin::Keygen { card: true },
        }));
        for mutation in mutations {
            device.apply_mutation(Mutation::Card(mutation));
        }
        device.card_record(&share_image).cloned().unwrap()
    };
    for format_id in [1, 2, 4] {
        let rec = replayed(vec![CardMutation::SealedC {
            share_image,
            sealed: sealed(format_id),
            words_hash: [3; 32],
        }]);
        assert_eq!(
            rec.state,
            CardState::OtherFormat,
            "SealedC of format {format_id}"
        );
    }
    let rec = replayed(vec![CardMutation::SealedC {
        share_image,
        sealed: sealed(3),
        words_hash: [3; 32],
    }]);
    assert_eq!(rec.state, CardState::Sealed);
    assert_eq!(rec.sealed, Some(sealed(3)));
    assert_eq!(rec.words_hash, Some([3; 32]));
}

/// `CardRefusal` is persisted on the coordinator (inside its `CardFailReason`).
#[test]
fn test_split_card_refusals() {
    for (refusal, hex) in [
        (CardRefusal::NotInPhoneHalf, "0f"),
        (CardRefusal::NoRecord, "10"),
    ] {
        let bytes = frostsnap_core::hex::decode(hex).unwrap();
        let (decoded, _): (CardRefusal, _) =
            bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
        assert_eq!(decoded, refusal);
    }
}
