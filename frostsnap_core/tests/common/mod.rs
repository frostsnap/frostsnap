//! Shared helpers for integration tests that don't belong in the public
//! `frostsnap_core::test` module (which is for downstream crates).

/// Macro for testing backward compatibility of bincode serialization.
#[macro_export]
macro_rules! assert_bincode_hex_eq {
    ($mutation:expr, $expected_hex:expr) => {
        let expected_bytes = frostsnap_core::hex::decode($expected_hex)
            .expect(&format!("Failed to parse hex for {:?}", $mutation.kind()));
        let (decoded, _) = bincode::decode_from_slice(&expected_bytes, bincode::config::standard())
            .expect(&format!("Failed to decode hex for {:?}", $mutation.kind()));
        assert_eq!($mutation, decoded, "Mismatch for {:?}", $mutation.kind());
    };
}
