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
use frostsnap_core::{tweak::Account, Appkey};

pub fn multi_x_descriptor_for_account(
    approot: Appkey,
    account: Account,
    network: bitcoin::NetworkKind,
) -> Descriptor<DescriptorPublicKey> {
    let app_xpub = approot.to_xpub().to_bitcoin_xpub_with_lies(network);

    let account_xpub = app_xpub
        .derive_pub(&Secp256k1::verification_only(), &account.derivation_path())
        .unwrap();

    let multi_xpub = DescriptorPublicKey::MultiXPub(DescriptorMultiXKey {
        origin: Some((app_xpub.fingerprint(), account.derivation_path())),
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
