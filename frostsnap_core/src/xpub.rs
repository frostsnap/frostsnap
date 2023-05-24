// use bitcoin::bip32::ExtendedPubKey;
// use schnorr_fun::fun::Point;

// pub fn new(frost_public_key: Point) -> bitcoin::bip32::ExtendedPubKey {
//     let xpub = bitcoin::bip32::ExtendedPubKey {
//         network: bitcoin::Network::Bitcoin,
//         depth: 0,
//         parent_fingerprint: bitcoin::bip32::Fingerprint::default(),
//         child_number: bitcoin::bip32::ChildNumber::Normal { index: 0 },
//         public_key: frost_public_key.into(),
//         chain_code: bitcoin::bip32::ChainCode::from([0u8; 32]),
//     };
//     xpub
// }
