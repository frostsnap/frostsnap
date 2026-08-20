//! The template is bincode-encoded into `WireSignTask` and parsed by firmware, so its
//! encoding is a compatibility surface. This pins it against a fixture captured before the
//! scope type parameter existed.

use bitcoin::{hashes::Hash, Amount, OutPoint, ScriptBuf, TxOut, Txid};
use frostsnap_core::{
    bitcoin_transaction::{LocalSpk, PushInput, TransactionTemplate},
    schnorr_fun::fun::{g, G},
    tweak::{BitcoinBip32Path, NormalIndex},
    MasterAppkey,
};

fn idx(n: u32) -> NormalIndex {
    NormalIndex::new(n).unwrap()
}

/// Both sides, both owner kinds, and a non-default locktime.
///
/// The version is left at `new()`'s default deliberately. These bytes were captured from code
/// that predates the scope type parameter, which is what makes them evidence the encoding did
/// not move; regenerating them to exercise another field spends that for a field nothing here
/// touches. `version` is worth covering on its own terms — `TransactionTemplate::from_psbt`
/// copies it verbatim out of a third-party PSBT and it travels to the device — but under a
/// fixture of its own, which does not exist yet.
fn representative_template() -> TransactionTemplate {
    let key = MasterAppkey::derive_from_rootkey(g!(2 * G).normalize());
    let path = BitcoinBip32Path::external(idx(7));
    let mut template = TransactionTemplate::new();
    template.set_lock_time(bitcoin::absolute::LockTime::from_height(812_345).unwrap());

    let ours = TxOut {
        value: Amount::from_sat(123_456),
        script_pubkey: LocalSpk {
            master_appkey: key,
            bip32_path: path,
        }
        .spk(),
    };
    template
        .push_owned_input(
            PushInput::spend_outpoint(
                &ours,
                OutPoint {
                    txid: Txid::from_byte_array([3u8; 32]),
                    vout: 1,
                },
            ),
            LocalSpk {
                master_appkey: key,
                bip32_path: path,
            },
        )
        .unwrap();

    let theirs = TxOut {
        value: Amount::from_sat(99_000),
        script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xab; 32][..]].concat()),
    };
    template.push_foreign_input(PushInput::spend_outpoint(
        &theirs,
        OutPoint {
            txid: Txid::from_byte_array([4u8; 32]),
            vout: 0,
        },
    ));

    template.push_owned_output(
        Amount::from_sat(50_000),
        LocalSpk {
            master_appkey: key,
            bip32_path: BitcoinBip32Path::internal(idx(2)),
        },
    );
    template.push_foreign_output(TxOut {
        value: Amount::from_sat(150_000),
        script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xcd; 32][..]].concat()),
    });
    template
}

/// Captured before the scope type parameter existed. If this changes, firmware that has not
/// been upgraded can no longer parse a sign request.
const ENCODED: &str = "04fc39650c000220030303030303030303030303030303030303030303030303030303030303030301fc40e20100010262e5d80f30c313d69a0186b54c4a1ace22cce7376dea84b689f130b786eb29d1000f25db8615dd0663b2ad400f1c6607163ace291ca8ef284b8d319b54bff90500000007fcfdffffff20040404040404040404040404040404040404040404040404040404040404040400fcb882010000225120ababababababababababababababababababababababababababababababababfcfdffffff02fb50c3010262e5d80f30c313d69a0186b54c4a1ace22cce7376dea84b689f130b786eb29d1000f25db8615dd0663b2ad400f1c6607163ace291ca8ef284b8d319b54bff90500000102fcf049020000225120cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

#[test]
fn template_encoding_is_unchanged() {
    let encoded =
        bincode::encode_to_vec(representative_template(), bincode::config::standard()).unwrap();

    assert_eq!(
        frostsnap_core::schnorr_fun::fun::hex::encode(&encoded),
        ENCODED,
        "the template's encoding is what firmware parses; it must not move"
    );
}

#[test]
fn template_decodes_the_captured_encoding() {
    let bytes = frostsnap_core::schnorr_fun::fun::hex::decode(ENCODED).unwrap();
    let (decoded, _): (TransactionTemplate, _) =
        bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();

    assert_eq!(decoded, representative_template());
}
