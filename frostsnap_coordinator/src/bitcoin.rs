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
    let secp = Secp256k1::verification_only();
    let bitcoin_app_xpub = master_appkey.derive_appkey(&secp, AppTweakKind::Bitcoin, network);
    let account_xpub = bitcoin_app_xpub
        .derive_pub(&secp, &account.derivation_path())
        .unwrap();

    let keychains = [Keychain::External, Keychain::Internal];

    let multi_xpub = DescriptorPublicKey::MultiXPub(DescriptorMultiXKey {
        origin: Some((bitcoin_app_xpub.fingerprint(), account.derivation_path())),
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
