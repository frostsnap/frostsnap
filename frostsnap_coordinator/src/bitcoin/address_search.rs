use bdk_chain::bitcoin::{
    bip32::{ChildNumber, Xpub},
    key::TapTweak,
    secp256k1::Secp256k1,
    NetworkKind, Script, ScriptBuf,
};
use frostsnap_core::{
    tweak::{AppTweakKind, BitcoinAccountKeychain, BitcoinBip32Path, NormalIndex},
    MasterAppkey,
};
/// How many indices one step of the search covers, on each of the receive and change keychains:
/// 10,000 addresses in all.
pub const ADDRESS_SEARCH_STEP: u32 = 5_000;

/// Looks for the wallet address paying to `target` among the receive and change addresses from
/// index `start`, for one [`ADDRESS_SEARCH_STEP`].
pub fn search_for_address(
    master_appkey: MasterAppkey,
    target: &Script,
    start: u32,
) -> Option<BitcoinBip32Path> {
    let bitcoin_appkey = master_appkey.derive_appkey(AppTweakKind::Bitcoin);
    // Deriving each keychain once leaves one child derivation per address instead of the whole
    // path. That child derivation and the taproot tweak go through libsecp256k1 rather than
    // frostsnap_core's own derivation, which measured about three times slower.
    let secp = Secp256k1::verification_only();
    let keychains: [(BitcoinAccountKeychain, Xpub); 2] = [
        BitcoinAccountKeychain::external(),
        BitcoinAccountKeychain::internal(),
    ]
    .map(|keychain| {
        (
            keychain,
            bitcoin_appkey
                .derive_bip32(keychain.path_segments_from_bitcoin_appkey())
                .to_bitcoin_xpub_with_lies(NetworkKind::Main),
        )
    });

    (start..start.saturating_add(ADDRESS_SEARCH_STEP))
        .map_while(NormalIndex::new)
        .find_map(|index| {
            keychains.iter().find_map(|(account_keychain, xpub)| {
                let internal_key = xpub
                    .ckd_pub(&secp, ChildNumber::from(index))
                    .expect("computationally unreachable")
                    .to_x_only_pub();
                let (output_key, _) = internal_key.tap_tweak(&secp, None);
                (ScriptBuf::new_p2tr_tweaked(output_key).as_script() == target).then_some(
                    BitcoinBip32Path {
                        account_keychain: *account_keychain,
                        index,
                    },
                )
            })
        })
}

#[cfg(test)]
mod test {
    use super::*;
    use frostsnap_core::bitcoin_transaction::LocalSpk;
    use std::str::FromStr;

    fn master_appkey() -> MasterAppkey {
        MasterAppkey::from_str("0325b0d1cda060241998916f45d02e227db436bdd708a55cf1dc67f3f534e332186fd6543fbfc5dd07094e93543fa05120f12d3a80876aa011a4897b7a0770d1fb").unwrap()
    }

    fn path(external: bool, index: u32) -> BitcoinBip32Path {
        let index = NormalIndex::new(index).unwrap();
        if external {
            BitcoinBip32Path::external(index)
        } else {
            BitcoinBip32Path::internal(index)
        }
    }

    fn spk(path: BitcoinBip32Path) -> ScriptBuf {
        LocalSpk {
            master_appkey: master_appkey(),
            bip32_path: path,
        }
        .spk()
    }

    #[test]
    fn finds_receive_and_change_addresses_at_their_index() {
        for target in [
            path(true, 0),
            path(false, 0),
            path(true, 7),
            path(false, 4_999),
        ] {
            assert_eq!(
                search_for_address(master_appkey(), &spk(target), 0),
                Some(target)
            );
        }
    }

    #[test]
    fn searches_one_step_from_start() {
        let target = path(true, ADDRESS_SEARCH_STEP);
        let spk = spk(target);
        assert_eq!(search_for_address(master_appkey(), &spk, 0), None);
        assert_eq!(
            search_for_address(master_appkey(), &spk, ADDRESS_SEARCH_STEP),
            Some(target)
        );
    }
}
