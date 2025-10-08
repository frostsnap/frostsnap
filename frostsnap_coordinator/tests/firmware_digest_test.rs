use frostsnap_coordinator::FirmwareBin;
use frostsnap_core::hex;

#[test]
fn test_v0_0_1_firmware_digests() {
    let firmware_signed_bytes = include_bytes!("v0.0.1-firmware-signed.bin");
    let firmware_unsigned_bytes = include_bytes!("v0.0.1-firmware-unsigned.bin");

    let firmware_signed = FirmwareBin::new(firmware_signed_bytes);
    let firmware_unsigned = FirmwareBin::new(firmware_unsigned_bytes);

    // Expected digests from the release
    const EXPECTED_SIGNED_DIGEST: &str =
        "57161f80b41413b1053e272f9c3da8d16ecfce44793345be69f7fe03d93f4eb0";
    const EXPECTED_UNSIGNED_DIGEST: &str =
        "8f45ae6b72c241a20798acbd3c6d3e54071cae73e335df1785f2d485a915da4c";

    // Test 1: Hash of entire signed firmware
    let signed_hex = hex::encode(&firmware_signed.digest().0);

    assert_eq!(
        signed_hex, EXPECTED_SIGNED_DIGEST,
        "Signed firmware digest doesn't match"
    );

    // Test 2: Hash of unsigned firmware (deterministic build)
    let unsigned_hex = hex::encode(&firmware_unsigned.digest().0);

    assert_eq!(
        unsigned_hex, EXPECTED_UNSIGNED_DIGEST,
        "Unsigned firmware digest doesn't match"
    );

    // Test 3: find_signature_block should find signature at end of signed firmware
    let sig_block_start = firmware_signed
        .find_signature_block()
        .expect("Should find signature block in signed firmware");

    println!(
        "Signature block starts at: 0x{:x} ({} bytes)",
        sig_block_start, sig_block_start
    );
    println!(
        "Unsigned firmware size: {} bytes",
        firmware_unsigned_bytes.len()
    );
    println!(
        "Signed firmware size: {} bytes",
        firmware_signed_bytes.len()
    );

    // The signature should be at the last 4096 bytes
    assert_eq!(
        sig_block_start,
        firmware_signed_bytes.len() - 4096,
        "Signature block should be at the end"
    );

    // Test 4: Unsigned firmware should NOT have a signature block
    assert_eq!(
        firmware_unsigned.find_signature_block(),
        None,
        "Unsigned firmware should not have a signature block"
    );

    // Test 5: For unsigned firmware, firmware_only_digest() should equal digest()
    let unsigned_firmware_only_hex = hex::encode(&firmware_unsigned.firmware_only_digest().0);
    assert_eq!(
        unsigned_firmware_only_hex, EXPECTED_UNSIGNED_DIGEST,
        "firmware_only_digest() should return same digest for unsigned firmware"
    );

    // Test 6: firmware_only_digest() should return the unsigned digest for signed firmware
    let firmware_only_hex = hex::encode(&firmware_signed.firmware_only_digest().0);
    assert_eq!(
        firmware_only_hex, EXPECTED_UNSIGNED_DIGEST,
        "firmware_only_digest() should extract unsigned digest from signed firmware"
    );

    // Test 7: Old firmware (v0.0.1) should report upgrade_digest_no_sig capability as false
    let signed_capabilities = firmware_signed.digest().capabilities();
    let unsigned_capabilities = firmware_unsigned.digest().capabilities();

    assert!(
        !signed_capabilities.upgrade_digest_no_sig,
        "v0.0.1 signed firmware should not support upgrade_digest_no_sig"
    );

    assert!(
        !unsigned_capabilities.upgrade_digest_no_sig,
        "v0.0.1 unsigned firmware should not support upgrade_digest_no_sig"
    );

    println!("✓ All firmware digest tests passed!");
    println!("  - Signed firmware digest: {}", EXPECTED_SIGNED_DIGEST);
    println!("  - Unsigned firmware digest: {}", EXPECTED_UNSIGNED_DIGEST);
    println!("  - firmware_only_digest() correctly extracts unsigned digest from signed firmware");
    println!("  - v0.0.1 firmware correctly reports upgrade_digest_no_sig capability as false");
}
