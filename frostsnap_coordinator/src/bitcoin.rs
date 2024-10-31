pub mod chain_sync;
pub mod wallet;
mod wallet_persist;

use bdk_chain::{
    bitcoin::{self, key::Secp256k1},
    miniscript::{
        descriptor::{DerivPaths, DescriptorMultiXKey, Wildcard},
        Descriptor, DescriptorPublicKey,
    },
};
use frostsnap_core::{
    tweak::{AppTweakKind, BitcoinAccount, Keychain},
    MasterAppkey,
};

pub fn multi_x_descriptor_for_account(
    master_appkey: MasterAppkey,
    account: BitcoinAccount,
    network: bitcoin::NetworkKind,
) -> Descriptor<DescriptorPublicKey> {
    let master_appkey_xpub = master_appkey.to_xpub().to_bitcoin_xpub_with_lies(network);
    let secp = Secp256k1::verification_only();
    let derivation_path = AppTweakKind::Bitcoin
        .derivation_path()
        .extend(account.derivation_path());
    let account_xpub = master_appkey_xpub
        .derive_pub(&secp, &derivation_path)
        .unwrap();

    let keychains = [Keychain::External, Keychain::Internal];

    let multi_xpub = DescriptorPublicKey::MultiXPub(DescriptorMultiXKey {
        origin: Some((master_appkey_xpub.fingerprint(), derivation_path)),
        xkey: account_xpub,
        derivation_paths: DerivPaths::new(
            keychains
                .into_iter()
                .map(|keychain| keychain.derivation_path())
                .collect(),
        )
        .unwrap(),
        wildcard: Wildcard::Unhardened,
    });
    let desc_key = multi_xpub;

    Descriptor::new_tr(desc_key, None).expect("well formed")
}
