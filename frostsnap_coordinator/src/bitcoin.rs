pub mod chain_sync;
pub mod psbt;
pub mod status_tracker;
pub mod tofu;
pub mod wallet;
mod wallet_persist;

use bdk_chain::{
    bitcoin::{
        self,
        bip32::{ChildNumber, DerivationPath},
        ScriptBuf,
    },
    miniscript::{
        descriptor::{DerivPaths, DescriptorMultiXKey, Wildcard},
        Descriptor, DescriptorPublicKey,
    },
};
use frostsnap_core::{
    tweak::{AppTweakKind, BitcoinAccount, BitcoinBip32Path, DerivationPathExt, Keychain},
    MasterAppkey,
};
use wallet::KeychainId;

/// Descriptor for a key.
pub fn multi_x_descriptor_for_account(
    master_appkey: MasterAppkey,
    account: BitcoinAccount,
    network: bitcoin::NetworkKind,
) -> Descriptor<DescriptorPublicKey> {
    let bitcoin_app_xpub = master_appkey.derive_appkey(AppTweakKind::Bitcoin);
    let account_xpub = bitcoin_app_xpub.derive_bip32(account.path_segments_from_bitcoin_appkey());

    let keychains = [Keychain::External, Keychain::Internal];

    let multi_xpub = DescriptorPublicKey::MultiXPub(DescriptorMultiXKey {
        origin: Some((
            bitcoin_app_xpub.fingerprint(),
            DerivationPath::from_normal_path_segments(account.path_segments_from_bitcoin_appkey()),
        )),
        xkey: account_xpub.to_bitcoin_xpub_with_lies(network),
        derivation_paths: DerivPaths::new(
            keychains
                .into_iter()
                .map(|keychain| {
                    DerivationPath::master()
                        .child(ChildNumber::from_normal_idx(keychain as u32).unwrap())
                })
                .collect(),
        )
        .unwrap(),
        wildcard: Wildcard::Unhardened,
    });
    let desc_key = multi_xpub;

    Descriptor::new_tr(desc_key, None).expect("well formed")
}

pub fn descriptor_for_account_keychain(
    keychain: KeychainId,
    network: bitcoin::NetworkKind,
) -> Descriptor<DescriptorPublicKey> {
    let idx = keychain.1.keychain as usize;
    multi_x_descriptor_for_account(keychain.0, keychain.1.account, network)
        .into_single_descriptors()
        .expect("infallible")
        .remove(idx)
}

fn peek_spk(approot: MasterAppkey, path: BitcoinBip32Path) -> ScriptBuf {
    let descriptor = descriptor_for_account_keychain(
        (approot, path.account_keychain),
        bitcoin::NetworkKind::Main,
    );
    descriptor
        .at_derivation_index(path.index)
        .expect("infallible")
        .script_pubkey()
}

#[cfg(test)]
mod test {
    use bitcoin::Network;
    use core::str::FromStr;
    use frostsnap_core::tweak::{AccountKind, AppTweak, BitcoinAccountKeychain, BitcoinBip32Path};

    use super::*;

    #[test]
    fn descriptor_should_match_frostsnap_core() {
        let master_appkey = MasterAppkey::from_str("0325b0d1cda060241998916f45d02e227db436bdd708a55cf1dc67f3f534e332186fd6543fbfc5dd07094e93543fa05120f12d3a80876aa011a4897b7a0770d1fb").unwrap();
        let account = BitcoinAccount {
            kind: AccountKind::Segwitv1,
            index: 0,
        };

        let internal_tweak = AppTweak::Bitcoin(BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain {
                account,
                keychain: Keychain::Internal,
            },
            index: 84,
        });
        let external_tweak = AppTweak::Bitcoin(BitcoinBip32Path {
            account_keychain: BitcoinAccountKeychain {
                account,
                keychain: Keychain::External,
            },
            index: 42,
        });

        let multi_x_descriptor =
            multi_x_descriptor_for_account(master_appkey, account, bitcoin::NetworkKind::Main);

        let descriptors = multi_x_descriptor.into_single_descriptors().unwrap();

        let external_address = descriptors[0]
            .at_derivation_index(42)
            .unwrap()
            .address(Network::Bitcoin)
            .unwrap();

        let internal_address = descriptors[1]
            .at_derivation_index(84)
            .unwrap()
            .address(Network::Bitcoin)
            .unwrap();

        assert!(external_address.is_related_to_xonly_pubkey(
            &external_tweak
                .derive_xonly_key(&master_appkey.to_xpub())
                .into()
        ));
        assert!(internal_address.is_related_to_xonly_pubkey(
            &internal_tweak
                .derive_xonly_key(&master_appkey.to_xpub())
                .into(),
        ));

        assert_ne!(external_address, internal_address);
    }
}
