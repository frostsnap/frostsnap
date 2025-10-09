use frostsnap_coordinator::{firmware::VersionNumber, FirmwareBin};
use frostsnap_core::hex;

#[test]
fn test_v0_0_1_firmware_digests() {
    let firmware_signed_bytes = include_bytes!("v0.0.1-firmware-signed.bin");
    let firmware_unsigned_bytes = include_bytes!("v0.0.1-firmware-unsigned.bin");

    let firmware_signed = FirmwareBin::new(firmware_signed_bytes)
        .validate()
        .expect("Failed to validate signed firmware");
    let firmware_unsigned = FirmwareBin::new(firmware_unsigned_bytes)
        .validate()
        .expect("Failed to validate unsigned firmware");

    // Expected digests from the release
    const EXPECTED_SIGNED_DIGEST: &str =
        "57161f80b41413b1053e272f9c3da8d16ecfce44793345be69f7fe03d93f4eb0";
    const EXPECTED_UNSIGNED_DIGEST: &str =
        "8f45ae6b72c241a20798acbd3c6d3e54071cae73e335df1785f2d485a915da4c";

    // Test 1: Hash of entire signed firmware (with signature block)
    let signed_hex = hex::encode(&firmware_signed.digest_with_signature().0);

    assert_eq!(
        signed_hex, EXPECTED_SIGNED_DIGEST,
        "Signed firmware digest doesn't match"
    );

    // Test 2: Hash of unsigned firmware (deterministic build, firmware-only)
    let unsigned_hex = hex::encode(&firmware_unsigned.digest().0);

    assert_eq!(
        unsigned_hex, EXPECTED_UNSIGNED_DIGEST,
        "Unsigned firmware digest doesn't match"
    );

    // Test 3: Signed firmware should have total_size > firmware_size
    assert_eq!(
        firmware_signed.total_size(),
        firmware_signed_bytes.len() as u32,
        "Signed firmware total_size should match actual bytes"
    );
    assert!(
        firmware_signed.firmware_size() < firmware_signed.total_size(),
        "Signed firmware should have signature block (firmware_size < total_size)"
    );

    // Test 4: Unsigned firmware should have firmware_size == total_size
    assert_eq!(
        firmware_unsigned.firmware_size(),
        firmware_unsigned.total_size(),
        "Unsigned firmware should have firmware_size == total_size"
    );
    assert_eq!(
        firmware_unsigned.total_size(),
        firmware_unsigned_bytes.len() as u32,
        "Unsigned firmware total_size should match actual bytes"
    );

    // Test 5: For unsigned firmware, digest() should equal the deterministic build digest
    let unsigned_firmware_only_hex = hex::encode(&firmware_unsigned.digest().0);
    assert_eq!(
        unsigned_firmware_only_hex, EXPECTED_UNSIGNED_DIGEST,
        "digest() should return firmware-only digest for unsigned firmware"
    );

    // Test 6: For signed firmware, digest() should return firmware-only (deterministic) digest
    let firmware_only_hex = hex::encode(&firmware_signed.digest().0);
    assert_eq!(
        firmware_only_hex, EXPECTED_UNSIGNED_DIGEST,
        "digest() should return firmware-only digest (excluding signature) for signed firmware"
    );

    // Test 7: Old firmware (v0.0.1) should report upgrade_digest_no_sig capability as false
    let signed_version = VersionNumber::from_digest(&firmware_signed.digest_with_signature())
        .expect("Should find v0.0.1 signed firmware");
    let unsigned_version = VersionNumber::from_digest(&firmware_unsigned.digest())
        .expect("Should find v0.0.1 unsigned firmware");

    assert!(
        !signed_version.capabilities().upgrade_digest_no_sig,
        "v0.0.1 signed firmware should not support upgrade_digest_no_sig"
    );

    assert!(
        !unsigned_version.capabilities().upgrade_digest_no_sig,
        "v0.0.1 unsigned firmware should not support upgrade_digest_no_sig"
    );

    // Test 8: Signed firmware should have is_signed() == true
    assert!(
        firmware_signed.is_signed(),
        "Signed firmware should have is_signed() == true"
    );

    // Test 9: Unsigned firmware should have is_signed() == false
    assert!(
        !firmware_unsigned.is_signed(),
        "Unsigned firmware should have is_signed() == false"
    );

    // Test 10: Both firmwares should have version information
    assert!(
        firmware_signed.version().is_some(),
        "Signed firmware should have version information"
    );
    assert!(
        firmware_unsigned.version().is_some(),
        "Unsigned firmware should have version information"
    );

    // Test 11: Version information should be v0.0.1
    assert_eq!(firmware_signed.version().unwrap().to_string(), "0.0.1");
    assert_eq!(firmware_unsigned.version().unwrap().to_string(), "0.0.1");

    println!("✓ All firmware digest tests passed!");
    println!(
        "  - Signed firmware digest (with sig): {}",
        EXPECTED_SIGNED_DIGEST
    );
    println!(
        "  - Firmware-only digest (deterministic): {}",
        EXPECTED_UNSIGNED_DIGEST
    );
    println!("  - digest() now returns firmware-only digest by default");
    println!("  - v0.0.1 firmware correctly reports upgrade_digest_no_sig capability as false");
    println!("  - is_signed() correctly identifies signed vs unsigned firmware");
    println!("  - version() correctly returns v0.0.1 for both signed and unsigned");
}
