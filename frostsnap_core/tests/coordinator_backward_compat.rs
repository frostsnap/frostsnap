/// Tests to ensure backward compatibility of coordinator mutations
mod common;

use frostsnap_core::coordinator::{
    keys::KeyMutation,
    restoration::{PendingConsolidation, RestorationMutation},
    signing::SigningMutation,
    ActiveSignSession, CompleteKey, CoordAccessStructure, Mutation, SignSessionProgress, StartSign,
};
use frostsnap_core::device::KeyPurpose;
use frostsnap_core::message::{EncodedSignature, GroupSignReq};
use frostsnap_core::nonce_stream::{CoordNonceStreamState, NonceStreamId, NonceStreamSegment};
use frostsnap_core::tweak::AppTweak;
use frostsnap_core::tweak::Xpub;
use frostsnap_core::{
    AccessStructureId, AccessStructureKind, AccessStructureRef, KeyId, Kind, MasterAppkey,
    SignItem, WireSignTask,
};
use frostsnap_core::{DeviceId, RestorationId, SignSessionId};
use schnorr_fun::frost::{ShareImage, SharedKey};
use schnorr_fun::{binonce, fun::prelude::*};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

#[test]
fn test_all_coordinator_mutations() {
    // Create test data
    let share_image = ShareImage {
        index: s!(1).public(),
        image: g!(42 * G).normalize().mark_zero(),
    };

    let access_structure_ref = AccessStructureRef {
        key_id: KeyId([3u8; 32]),
        access_structure_id: AccessStructureId([4u8; 32]),
    };

    let pending_consolidation = PendingConsolidation {
        device_id: DeviceId([5u8; 33]),
        access_structure_ref,
        share_index: s!(1).public(),
    };

    // Create test data for Keygen mutations
    let master_appkey = MasterAppkey::derive_from_rootkey(g!(42 * G).normalize());

    // Create encrypted rootkey for CompleteKey using proper encryption
    use frostsnap_core::{Ciphertext, SymmetricKey};
    let test_key = SymmetricKey([42u8; 32]);
    let test_rootkey = g!(7 * G).normalize();
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(1337);
    let encrypted_rootkey = Ciphertext::encrypt(test_key, &test_rootkey, &mut rng);

    // Create SharedKey for Xpub
    let poly = vec![
        g!(42 * G).normalize().mark_zero(),
        g!(7 * G).normalize().mark_zero(),
    ];
    let shared_key = SharedKey::from_poly(poly).non_zero().unwrap();

    let xpub_shared_key = Xpub {
        key: shared_key,
        chaincode: [2u8; 32],
    };

    // Create CoordAccessStructure for CompleteKey using proper constructor
    let device_to_share_index: BTreeMap<DeviceId, schnorr_fun::frost::ShareIndex> =
        [(DeviceId([5u8; 33]), s!(1).public())]
            .into_iter()
            .collect();

    let coord_access_structure = CoordAccessStructure::new(
        xpub_shared_key.clone(),
        device_to_share_index,
        AccessStructureKind::Master,
    );

    let mut access_structures = HashMap::new();
    access_structures.insert(AccessStructureId([4u8; 32]), coord_access_structure);

    let complete_key = CompleteKey {
        master_appkey,
        encrypted_rootkey,
        access_structures,
    };

    // Create test data for complex Signing mutations
    // Create NonceStreamSegment with actual binonce::Nonce
    let nonce_segment = NonceStreamSegment {
        stream_id: NonceStreamId([7u8; 16]),
        nonces: {
            let mut nonces = VecDeque::new();
            // Create a binonce::Nonce from two Points
            let nonce = binonce::Nonce([g!(10 * G).normalize(), g!(11 * G).normalize()]);
            nonces.push_back(nonce);
            nonces
        },
        index: 1,
    };

    // Create SignSessionProgress using its new function with deterministic RNG
    let sign_session_progress = {
        let frost = schnorr_fun::frost::Frost::<
            sha2::Sha256,
            schnorr_fun::nonce::Deterministic<sha2::Sha256>,
        >::default();
        let app_shared_key = Xpub {
            key: SharedKey::from_poly(vec![
                g!(27 * G).normalize().mark_zero(),
                g!(28 * G).normalize().mark_zero(),
            ])
            .non_zero()
            .unwrap(),
            chaincode: [29u8; 32],
        };
        let sign_item = SignItem {
            message: b"test message to sign".to_vec(),
            app_tweak: AppTweak::TestMessage,
        };
        let mut nonces = BTreeMap::new();
        nonces.insert(
            s!(1).public(),
            binonce::Nonce([g!(30 * G).normalize(), g!(31 * G).normalize()]),
        );
        nonces.insert(
            s!(2).public(),
            binonce::Nonce([g!(32 * G).normalize(), g!(33 * G).normalize()]),
        );
        // Use a deterministic RNG for consistent test results
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        SignSessionProgress::new(&frost, app_shared_key, sign_item, nonces, &mut rng)
    };

    // Create ActiveSignSession with proper GroupSignReq
    let active_sign_session = ActiveSignSession {
        progress: vec![sign_session_progress],
        init: StartSign {
            nonces: {
                let mut nonces = BTreeMap::new();
                // Add some device nonce states
                nonces.insert(
                    DeviceId([16u8; 33]),
                    CoordNonceStreamState {
                        stream_id: NonceStreamId([17u8; 16]),
                        index: 2,
                        remaining: 10,
                    },
                );
                nonces.insert(
                    DeviceId([18u8; 33]),
                    CoordNonceStreamState {
                        stream_id: NonceStreamId([19u8; 16]),
                        index: 3,
                        remaining: 15,
                    },
                );
                nonces
            },
            group_request: GroupSignReq {
                parties: {
                    let mut parties = BTreeSet::new();
                    parties.insert(s!(1).public());
                    parties.insert(s!(2).public());
                    parties.insert(s!(3).public());
                    parties
                },
                agg_nonces: vec![
                    // Add some aggregate nonces
                    binonce::Nonce([
                        g!(20 * G).normalize().mark_zero(),
                        g!(21 * G).normalize().mark_zero(),
                    ]),
                    binonce::Nonce([
                        g!(22 * G).normalize().mark_zero(),
                        g!(23 * G).normalize().mark_zero(),
                    ]),
                ],
                sign_task: WireSignTask::Test {
                    message: "test message for signing".to_string(),
                },
                access_structure_id: AccessStructureId([14u8; 32]),
            },
        },
        key_id: KeyId([13u8; 32]),
        sent_req_to_device: {
            let mut devices = HashSet::new();
            devices.insert(DeviceId([16u8; 33]));
            devices.insert(DeviceId([18u8; 33]));
            devices
        },
    };

    // Create SignatureShare - it's just a Scalar<Public, Zero>
    let signature_shares = vec![s!(15).public().mark_zero()];

    // Create EncodedSignature
    let encoded_sig = EncodedSignature([14u8; 64]);

    // Create all mutation variants we want to test
    let mutations = vec![
        // Restoration mutations
        Mutation::Restoration(RestorationMutation::NewRestoration {
            restoration_id: RestorationId([1u8; 16]),
            key_name: "test_key".to_string(),
            threshold: 2,
            key_purpose: KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
        }),
        Mutation::Restoration(RestorationMutation::RestorationProgress {
            restoration_id: RestorationId([1u8; 16]),
            device_id: DeviceId([2u8; 33]),
            share_image,
            access_structure_ref: None,
        }),
        Mutation::Restoration(RestorationMutation::RestorationProgress {
            restoration_id: RestorationId([1u8; 16]),
            device_id: DeviceId([2u8; 33]),
            share_image,
            access_structure_ref: Some(access_structure_ref),
        }),
        Mutation::Restoration(RestorationMutation::DeleteRestorationShare {
            restoration_id: RestorationId([1u8; 16]),
            device_id: DeviceId([2u8; 33]),
            share_image,
        }),
        Mutation::Restoration(RestorationMutation::DeleteRestoration {
            restoration_id: RestorationId([1u8; 16]),
        }),
        Mutation::Restoration(RestorationMutation::DeviceNeedsConsolidation(
            pending_consolidation,
        )),
        Mutation::Restoration(RestorationMutation::DeviceFinishedConsolidation(
            pending_consolidation,
        )),
        // Keygen mutations
        Mutation::Keygen(KeyMutation::NewKey {
            key_name: "test_key".to_string(),
            purpose: KeyPurpose::Bitcoin(bitcoin::Network::Bitcoin),
            complete_key,
        }),
        Mutation::Keygen(KeyMutation::NewAccessStructure {
            shared_key: xpub_shared_key,
            kind: AccessStructureKind::Master,
        }),
        Mutation::Keygen(KeyMutation::NewShare {
            access_structure_ref: AccessStructureRef {
                key_id: KeyId([3u8; 32]),
                access_structure_id: AccessStructureId([4u8; 32]),
            },
            device_id: DeviceId([5u8; 33]),
            share_index: s!(1).public(),
        }),
        Mutation::Keygen(KeyMutation::DeleteKey(KeyId([3u8; 32]))),
        // Signing mutations
        Mutation::Signing(SigningMutation::NewNonces {
            device_id: DeviceId([6u8; 33]),
            nonce_segment,
        }),
        Mutation::Signing(SigningMutation::NewSigningSession(active_sign_session)),
        Mutation::Signing(SigningMutation::SentSignReq {
            session_id: SignSessionId([12u8; 32]),
            device_id: DeviceId([6u8; 33]),
        }),
        Mutation::Signing(SigningMutation::GotSignatureSharesFromDevice {
            session_id: SignSessionId([12u8; 32]),
            device_id: DeviceId([6u8; 33]),
            signature_shares,
        }),
        Mutation::Signing(SigningMutation::CloseSignSession {
            session_id: SignSessionId([12u8; 32]),
            finished: Some(vec![encoded_sig]),
        }),
        Mutation::Signing(SigningMutation::ForgetFinishedSignSession {
            session_id: SignSessionId([12u8; 32]),
        }),
    ];

    // Test each mutation
    for mutation in mutations {
        match mutation.clone() {
            Mutation::Restoration(RestorationMutation::NewRestoration { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "02000101010101010101010101010101010108746573745f6b6579020100"
                );
            }
            Mutation::Restoration(RestorationMutation::RestorationProgress {
                access_structure_ref: None,
                ..
            }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "020101010101010101010101010101010101020202020202020202020202020202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af00"
                );
            }
            Mutation::Restoration(RestorationMutation::RestorationProgress {
                access_structure_ref: Some(_),
                ..
            }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "020101010101010101010101010101010101020202020202020202020202020202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af0103030303030303030303030303030303030303030303030303030303030303030404040404040404040404040404040404040404040404040404040404040404"
                );
            }
            Mutation::Restoration(RestorationMutation::DeleteRestorationShare { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "020201010101010101010101010101010101020202020202020202020202020202020202020202020202020202020202020202000000000000000000000000000000000000000000000000000000000000000102fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af"
                );
            }
            Mutation::Restoration(RestorationMutation::DeleteRestoration { .. }) => {
                assert_bincode_hex_eq!(mutation, "020301010101010101010101010101010101");
            }
            Mutation::Restoration(RestorationMutation::DeviceNeedsConsolidation(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0204050505050505050505050505050505050505050505050505050505050505050505030303030303030303030303030303030303030303030303030303030303030304040404040404040404040404040404040404040404040404040404040404040000000000000000000000000000000000000000000000000000000000000001"
                );
            }
            Mutation::Restoration(RestorationMutation::DeviceFinishedConsolidation(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0205050505050505050505050505050505050505050505050505050505050505050505030303030303030303030303030303030303030303030303030303030303030304040404040404040404040404040404040404040404040404040404040404040000000000000000000000000000000000000000000000000000000000000001"
                );
            }
            Mutation::Keygen(KeyMutation::NewKey { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "000008746573745f6b65790100036a73f983b472e3ef6a6336bd6600ac2d49ef96ec284fc9da3c378156316baeb8941593f4651c7a0062624f959ecc292a412ebf57e7c497b9b007e87d5816e879b3cf6b65cd04d3f661e9227ecb1352668d1ae0b4d2b74aad3d5e30cc35ad8cfeaccaf41fde30f8caa72c5dad232e80495aeb47d4fe7e72e343252f8b1b0104040404040404040404040404040404040404040404040404040404040404040202fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af025cbdf0646e5db4eaa398f365f2ea7a0e3d419b7e0330e39ce92bddedcac4f9bc020202020202020202020202020202020202020202020202020202020202020201050505050505050505050505050505050505050505050505050505050505050505000000000000000000000000000000000000000000000000000000000000000100"
                );
            }
            Mutation::Keygen(KeyMutation::NewAccessStructure { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "00010202fe8d1eb1bcb3432b1db5833ff5f2226d9cb5e65cee430558c18ed3a3c86ce1af025cbdf0646e5db4eaa398f365f2ea7a0e3d419b7e0330e39ce92bddedcac4f9bc020202020202020202020202020202020202020202020202020202020202020200"
                );
            }
            Mutation::Keygen(KeyMutation::NewShare { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0002030303030303030303030303030303030303030303030303030303030303030304040404040404040404040404040404040404040404040404040404040404040505050505050505050505050505050505050505050505050505050505050505050000000000000000000000000000000000000000000000000000000000000001"
                );
            }
            Mutation::Keygen(KeyMutation::DeleteKey(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "00030303030303030303030303030303030303030303030303030303030303030303"
                );
            }
            Mutation::Signing(SigningMutation::NewNonces { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0100060606060606060606060606060606060606060606060606060606060606060606070707070707070707070707070707070103a0434d9e47f3c86235477c7b1ae6ae5d3442d49b1943c2b752a68e2a47e247c703774ae7f858a9411e5ef4246b70c65aac5649980be5c17891bbec17895da008cb01"
                );
            }
            Mutation::Signing(SigningMutation::NewSigningSession(_)) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "0101011474657374206d65737361676520746f207369676e00e4c0f99327353b433529c3866724204c4cc4537ff70fb1d9e63d19f33a2acfe4ae0e79e032ed91b6dd96d578e3f57a6794fc2a90bc4a33f94887b73f50484176279e7da397bd000278831221e2af872cca129bc310d612e77de05d1c459bfe69f0f15eb113f0ae254fe2a32d1b5c9653b3531ebd2169ca38d3667bac0c3be8e20329d39ee36b67586040d955065276b1eb4c75e0a06d7fd7fe7e1693aedbce9e1a03bf23c1542d16eab70b1051eaf832823cfc4c6f1dcdbafd81e37918e6f874ef8b020000000000000000000000000000000000000000000000000000000000000001026d2b085e9e382ed10b69fc311a03f8641ccfff21574de0927513a49d9a688a00036a245bf6dc698504c89a20cfded60853152b695336c28063b61c65cbd269e6b4000000000000000000000000000000000000000000000000000000000000000202d30199d74fb5a22d47b6e054e2f378cedacffcb89904a61d75d0dbd407143e65031697ffa6fd9de627c077e3d2fe541084ce13300b0bec1146f95ae57f0d0bd6a55ddbd8dd9c88337982ae52c0ecf50f73e97a8b083de7b0990da1e78f580857da000203daed4f2be3a8bf278e70132fb0beb7522f570e144bf615c07e996d443dee87290255eb67d7b7238a70a7fa6f64d5dc3c826b31536da6eb344dc39a66f904f979681d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d0210101010101010101010101010101010101010101010101010101010101010101011111111111111111111111111111111020a12121212121212121212121212121212121212121212121212121212121212121213131313131313131313131313131313030f0300000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000302024ce119c96e2fa357200b559b2f7dd5a5f02d5290aff74b03f3e471b273211c9702352bbf4a4cdd12564f93fa332ce333301d9ad40271f8107181340aef25be59d503421f5fc9a21065445c96fdb91c0c1e2f2431741c72713b4b99ddcb316f31e9fc032fa2104d6b38d11b0230010559879124e42ab8dfeff5ff29dc9cdadd4ecacc3f001874657374206d65737361676520666f72207369676e696e670e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d0d02101010101010101010101010101010101010101010101010101010101010101010121212121212121212121212121212121212121212121212121212121212121212"
                );
            }
            Mutation::Signing(SigningMutation::SentSignReq { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "01020c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c060606060606060606060606060606060606060606060606060606060606060606"
                );
            }
            Mutation::Signing(SigningMutation::GotSignatureSharesFromDevice { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "01030c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c06060606060606060606060606060606060606060606060606060606060606060601000000000000000000000000000000000000000000000000000000000000000f"
                );
            }
            Mutation::Signing(SigningMutation::CloseSignSession { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "01040c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c01010e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e"
                );
            }
            Mutation::Signing(SigningMutation::ForgetFinishedSignSession { .. }) => {
                assert_bincode_hex_eq!(
                    mutation,
                    "01050c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c"
                );
            }
        }
    }
}
