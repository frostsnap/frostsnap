pub mod chain_sync;
pub mod wallet;
mod wallet_persist;

use bdk_chain::{
    bitcoin::{
        self,
        bip32::{ChildNumber, DerivationPath},
        key::Secp256k1,
    },
    miniscript::{
        descriptor::{DerivPaths, DescriptorMultiXKey, Wildcard},
        Descriptor, DescriptorPublicKey,
    },
};
use frostsnap_core::{
    schnorr_fun::fun::Point,
    tweak::{Account, TweakableKey},
};

pub fn multi_x_descriptor_for_account(
    root_key: Point,
    account: Account,
    network: bitcoin::Network,
) -> Descriptor<DescriptorPublicKey> {
    let root_bitcoin_xpub = root_key.bitcoin_app_xpub().xpub(network);
    let account_xpub = root_bitcoin_xpub
        .derive_pub(&Secp256k1::verification_only(), &account.derivation_path())
        .unwrap();

    let multi_xpub = DescriptorPublicKey::MultiXPub(DescriptorMultiXKey {
        origin: Some((root_bitcoin_xpub.fingerprint(), account.derivation_path())),
        xkey: account_xpub,
        derivation_paths: DerivPaths::new(
            [0, 1]
                .into_iter()
                .map(|i| DerivationPath::from(vec![ChildNumber::Normal { index: i }]))
                .collect(),
        )
        .unwrap(),
        wildcard: Wildcard::Unhardened,
    });
    let desc_key = multi_xpub;

    Descriptor::new_tr(desc_key, None).expect("well formed")
}
