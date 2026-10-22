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
