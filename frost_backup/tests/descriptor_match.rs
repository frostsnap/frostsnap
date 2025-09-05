use bitcoin::bip32::DerivationPath;
use bitcoin::secp256k1;
use bitcoin::{Address, Network};
use frost_backup::generate_xpriv;
use frostsnap_coordinator::bitcoin::{descriptor_for_account_keychain, wallet::KeychainId};
use frostsnap_core::{
    tweak::{AccountKind, BitcoinAccount, BitcoinAccountKeychain, Keychain},
    MasterAppkey,
};
use schnorr_fun::fun::prelude::*;
use std::str::FromStr;

#[test]
fn test_addresses_match() {
    // Generate a test secret
    let secret = Scalar::random(&mut rand::thread_rng());
    let network = bitcoin::NetworkKind::Test;

    // Path 1: Generate xpriv using frost_backup
    let xpriv = generate_xpriv(&secret, network);
    let secp = secp256k1::Secp256k1::new();

    // Derive the frost_backup address at the expected path
    // The full path is /0/0/0/0/0/0 (6 zeros total)
    let path = DerivationPath::from_str("m/0/0/0/0/0/0").unwrap();
    let derived = xpriv.derive_priv(&secp, &path).unwrap();
    let pubkey = derived.to_keypair(&secp).x_only_public_key().0;
    let frost_backup_address = Address::p2tr(&secp, pubkey, None, Network::Testnet);

    // Path 2: Generate descriptor using frostsnap_core
    // Convert secret to Point
    let root_key: Point = g!(secret * G).normalize();

    // Derive master appkey from rootkey
    let master_appkey = MasterAppkey::derive_from_rootkey(root_key);

    // Create a bitcoin account (0 hardened)
    let account = BitcoinAccount {
        kind: AccountKind::Segwitv1,
        index: 0,
    };

    // Create keychain id for external chain (0)
    let keychain_id: KeychainId = (
        master_appkey,
        BitcoinAccountKeychain {
            account,
            keychain: Keychain::External,
        },
    );

    // Get descriptor from frostsnap_coordinator
    let frostsnap_descriptor = descriptor_for_account_keychain(keychain_id, network);

    // Get address from frostsnap descriptor
    let frostsnap_address = frostsnap_descriptor
        .at_derivation_index(0)
        .expect("Valid derivation")
        .address(Network::Testnet)
        .expect("Valid address");

    // Compare addresses
    assert_eq!(
        frost_backup_address, frostsnap_address,
        "First address from frost_backup xpriv should match frostsnap_core descriptor"
    );

    // Also verify addresses at index 1
    let path1 = DerivationPath::from_str("m/0/0/0/0/0/1").unwrap();
    let derived1 = xpriv.derive_priv(&secp, &path1).unwrap();
    let pubkey1 = derived1.to_keypair(&secp).x_only_public_key().0;
    let frost_backup_address_1 = Address::p2tr(&secp, pubkey1, None, Network::Testnet);

    let frostsnap_address_1 = frostsnap_descriptor
        .at_derivation_index(1)
        .expect("Valid derivation")
        .address(Network::Testnet)
        .expect("Valid address");

    assert_eq!(
        frost_backup_address_1, frostsnap_address_1,
        "Second address from frost_backup xpriv should match frostsnap_core descriptor"
    );
}
