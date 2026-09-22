mod common;

use common::TestDeviceKeyGen;
use frostsnap_core::device_nonces::{NonceJobBatch, RatchetSeedMaterial, SecretNonceSlot};
use frostsnap_core::nonce_stream::NonceStreamId;

#[test]
fn test_nonce_generation_deterministic() {
    // Fixed test data
    let nonce_stream_id = NonceStreamId([
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32,
        0x10,
    ]);
    let ratchet_prg_seed_material: RatchetSeedMaterial = [
        0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x00,
    ];

    let slot_value = SecretNonceSlot {
        index: 0,
        nonce_stream_id,
        ratchet_prg_seed_material,
        last_used: 0,
        signing_state: None,
    };

    let mut device_hmac = TestDeviceKeyGen;

    // Generate 3 nonces using a NonceJobBatch with a single job
    let task = slot_value.nonce_task(None, 3);
    let mut batch = NonceJobBatch::new(vec![task]);
    batch.run_until_finished(&mut device_hmac);
    let segments = batch.into_segments();
    assert_eq!(segments.len(), 1, "Should have exactly one segment");
    let nonces: Vec<String> = segments[0]
        .nonces
        .iter()
        .map(|nonce| nonce.to_string())
        .collect();

    // These are the expected nonce values captured from the original iterator implementation
    // Using TestDeviceKeyGen which uses HMAC with TEST_ENCRYPTION_KEY
    assert_eq!(nonces.len(), 3);

    let expected_nonces = ["03db824cdfb550fdfa8064aa69dc0cc9c8274c820fb8281b2d2443ad8d93e1d5fc02384fa6f82d96f2b31c871c05c19fef10a4885eb719a83b513f7b584932f3c960",
        "02a96b24205745a07255455672b47dc3a3ed8a5ad1e35ddfdb674d52b17b63da8a0357a55f16e389d4559c4d869443caa6c959eef34643d29878f3fc4567ed110334",
        "032dd9784b0f54987dd14a65cb81a7a9bbfbd91cbf30b72b40ba31dcc70b764e70027aea249a118f651a1da7861095604ccb2258aa78f10a34e751c65fb9c59a17c3"];

    for (i, (actual, expected)) in nonces.iter().zip(expected_nonces.iter()).enumerate() {
        assert_eq!(actual, expected, "Nonce {} mismatch", i);
    }
}

/// Re-opening a nonce stream whose slot was evicted must give fresh nonces, never a replay of the
/// ones handed out before eviction.
#[test]
fn reopening_evicted_nonce_stream_gives_fresh_nonces() {
    use frostsnap_core::device::{DeviceToUserMessage, FrostSigner};
    use frostsnap_core::message::{signing, CoordinatorToDeviceMessage, DeviceSend};
    use frostsnap_core::nonce_stream::CoordNonceStreamState;

    const N_SLOTS: usize = 4;
    const BATCH_SIZE: u32 = 4;

    fn open_stream(
        device: &mut FrostSigner,
        stream_id: NonceStreamId,
        rng: &mut impl rand::RngCore,
    ) -> Vec<String> {
        let message = CoordinatorToDeviceMessage::Signing(
            signing::CoordinatorSigning::OpenNonceStreams(signing::OpenNonceStreams {
                streams: vec![CoordNonceStreamState {
                    stream_id,
                    index: 0,
                    remaining: 0,
                }],
            }),
        );
        let sends = device.recv_coordinator_message(message, rng).unwrap();
        let mut batch = match sends.into_iter().next().unwrap() {
            DeviceSend::ToUser(msg) => match *msg {
                DeviceToUserMessage::NonceJobs(batch) => batch,
                other => panic!("expected nonce jobs, got {other:?}"),
            },
            other => panic!("expected a message to the user, got {other:?}"),
        };
        batch.run_until_finished(&mut TestDeviceKeyGen);
        let segments = batch.into_segments();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].stream_id, stream_id);
        segments[0]
            .nonces
            .iter()
            .map(|nonce| nonce.to_string())
            .collect()
    }

    let mut rng = rand::thread_rng();
    let mut device = FrostSigner::new_random_with_nonce_batch_size(&mut rng, N_SLOTS, BATCH_SIZE);

    let evicted_stream = NonceStreamId::random(&mut rng);
    let before_eviction = open_stream(&mut device, evicted_stream, &mut rng);
    assert_eq!(before_eviction.len(), BATCH_SIZE as usize);

    // Fill every slot with other streams, evicting the first one (least recently used).
    let other_streams: Vec<_> = (0..N_SLOTS)
        .map(|_| NonceStreamId::random(&mut rng))
        .collect();
    let survivor = other_streams[0];
    let all_opened: Vec<Vec<String>> = other_streams
        .iter()
        .map(|stream_id| open_stream(&mut device, *stream_id, &mut rng))
        .collect();
    let survivor_before = all_opened[0].clone();
    assert!(
        device.nonce_slots().get(evicted_stream).is_none(),
        "the first stream should have been evicted"
    );

    // Positive control: a stream that was not evicted still hands out the same nonces.
    assert_eq!(
        open_stream(&mut device, survivor, &mut rng),
        survivor_before,
        "a stream still in a slot must be reproducible"
    );

    let after_eviction = open_stream(&mut device, evicted_stream, &mut rng);
    assert_ne!(
        after_eviction[0], before_eviction[0],
        "re-opened evicted stream reused its first nonce"
    );
    for nonce in &after_eviction {
        assert!(
            !before_eviction.contains(nonce),
            "re-opened evicted stream reused a nonce it had already handed out"
        );
    }
}
