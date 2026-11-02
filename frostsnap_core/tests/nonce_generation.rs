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

    assert_eq!(nonces.len(), 3);

    let expected_nonces = ["03005eca3d7b9a01e2989ccf8b666b4b3d8297af037af170ede68388538ae986b202c28f0cb16318d50da0679a4afe2b4e54680eecf68a2991ead616dbbe01cd3d80",
        "02c1521a461debe8bc9f10b665d8fa100153d4add26d43f8128bd91b0ed10f21450200460b3b9fd84447aa24dbf31163c1229c5ebb4af24ba599a735a5c0577e7390",
        "030a443cdae604e48cc5b8e58f98e2d900cbabbc0dd3c4e27c85fabbafa575d48d0233725812523fc144c1f2879fba1f87c0857fc38480cd0adfcce13e41fa949d40"];

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
