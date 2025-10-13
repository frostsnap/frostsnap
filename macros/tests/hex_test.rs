use frostsnap_macros::hex;

#[test]
fn test_hex_basic() {
    let bytes: [u8; 4] = hex!("deadbeef");
    assert_eq!(bytes, [0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn test_hex_empty() {
    let bytes: [u8; 0] = hex!("");
    assert_eq!(bytes, []);
}

#[test]
fn test_hex_single_byte() {
    let bytes: [u8; 1] = hex!("ff");
    assert_eq!(bytes, [0xff]);
}

#[test]
fn test_hex_with_whitespace() {
    let bytes: [u8; 4] = hex!("  deadbeef  ");
    assert_eq!(bytes, [0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn test_hex_uppercase() {
    let bytes: [u8; 4] = hex!("DEADBEEF");
    assert_eq!(bytes, [0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn test_hex_mixed_case() {
    let bytes: [u8; 4] = hex!("DeAdBeEf");
    assert_eq!(bytes, [0xde, 0xad, 0xbe, 0xef]);
}

#[test]
fn test_hex_sha256() {
    let bytes: [u8; 32] = hex!("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(bytes.len(), 32);
    assert_eq!(bytes[0], 0xe3);
    assert_eq!(bytes[31], 0x55);
}
