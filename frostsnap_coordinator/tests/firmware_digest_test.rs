use frostsnap_comms::{firmware_reader, Sha256Digest};
use frostsnap_coordinator::{firmware::FirmwareValidationError, FirmwareBin};
use frostsnap_core::sha2::{Digest, Sha256};

const V0_0_1_SIGNED: &[u8] = include_bytes!("v0.0.1-firmware-signed.bin");
const V0_0_1_UNSIGNED: &[u8] = include_bytes!("v0.0.1-firmware-unsigned.bin");

const EXPECTED_FULL_IMAGE_DIGEST: &str =
    "57161f80b41413b1053e272f9c3da8d16ecfce44793345be69f7fe03d93f4eb0";
const EXPECTED_BODY_DIGEST: &str =
    "8f45ae6b72c241a20798acbd3c6d3e54071cae73e335df1785f2d485a915da4c";

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest(Sha256::digest(bytes).into())
}

#[test]
fn v0_0_1_digests_and_sizes() {
    let (signed_body, signed_total) =
        firmware_reader::firmware_size(&FirmwareBin::new(V0_0_1_SIGNED)).unwrap();
    assert_eq!(signed_total, V0_0_1_SIGNED.len() as u32);
    assert!(signed_body < signed_total);
    assert_eq!(
        sha256(V0_0_1_SIGNED).to_string(),
        EXPECTED_FULL_IMAGE_DIGEST
    );
    assert_eq!(
        sha256(&V0_0_1_SIGNED[..signed_body as usize]).to_string(),
        EXPECTED_BODY_DIGEST
    );

    let (unsigned_body, unsigned_total) =
        firmware_reader::firmware_size(&FirmwareBin::new(V0_0_1_UNSIGNED)).unwrap();
    assert_eq!(unsigned_body, unsigned_total);
    assert_eq!(unsigned_total, V0_0_1_UNSIGNED.len() as u32);
    assert_eq!(sha256(V0_0_1_UNSIGNED).to_string(), EXPECTED_BODY_DIGEST);
}

#[test]
fn v0_0_1_embeds_no_version_and_is_not_bundleable() {
    for bin in [V0_0_1_SIGNED, V0_0_1_UNSIGNED] {
        assert_eq!(
            firmware_reader::embedded_version(&FirmwareBin::new(bin)).unwrap(),
            None
        );
        assert!(matches!(
            FirmwareBin::new(bin).validate(),
            Err(FirmwareValidationError::NoEmbeddedVersion)
        ));
    }
}
