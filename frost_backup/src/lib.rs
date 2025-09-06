#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[allow(unused_imports)]
#[macro_use]
extern crate alloc;

pub mod bip39_words;
pub mod recovery;
pub mod share_backup;

mod error;
pub use error::*;
pub use schnorr_fun::frost::Fingerprint;
pub use share_backup::*;

/// The default fingerprint used for share generation in production
pub const FINGERPRINT: Fingerprint = Fingerprint::FROST_V0;

/// Generate an xpriv from a secret scalar
///
/// This creates an xpriv with the standard initial values that can be used
/// for BIP32 derivation.
#[cfg(feature = "std")]
pub fn generate_xpriv(
    secret: &schnorr_fun::fun::Scalar,
    network: bitcoin::NetworkKind,
) -> bitcoin::bip32::Xpriv {
    let secret_key = bitcoin::secp256k1::SecretKey::from_slice(&secret.to_bytes()).unwrap();
    let chaincode = [0u8; 32];
    bitcoin::bip32::Xpriv {
        network,
        depth: 0,
        parent_fingerprint: [0u8; 4].into(),
        child_number: bitcoin::bip32::ChildNumber::from_normal_idx(0).unwrap(),
        private_key: secret_key,
        chain_code: chaincode.into(),
    }
}

/// Generate a descriptor string from a secret scalar
///
/// This creates a taproot descriptor with the standard derivation path
/// that can be imported into a wallet. The path includes an extra /0
/// at the beginning to match frostsnap_core's rootkey_to_master_appkey
/// derivation.
#[cfg(feature = "std")]
pub fn generate_descriptor(
    secret: &schnorr_fun::fun::Scalar,
    network: bitcoin::NetworkKind,
) -> alloc::string::String {
    use alloc::format;
    let xpriv = generate_xpriv(secret, network);
    // Include the rootkey_to_master_appkey [0] derivation in the path
    format!("tr({}/0/0/0/0/<0;1>/*)", xpriv)
}
