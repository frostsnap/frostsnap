//! A signature is produced per *owned* input, so its position in the list is not an input
//! index. Every test here keeps a foreign input around, because that is the only thing that
//! tells the two orderings apart.

use bitcoin::{hashes::Hash, Amount, OutPoint, ScriptBuf, TxOut, Txid};
use frostsnap_core::{
    bitcoin_transaction::{LocalSpk, PushInput, TransactionTemplate},
    message::EncodedSignature,
    schnorr_fun::fun::{g, G},
    tweak::{BitcoinBip32Path, NormalIndex},
    MasterAppkey,
};
use std::collections::BTreeMap;

fn idx(n: u32) -> NormalIndex {
    NormalIndex::new(n).expect("test index is in range")
}

fn key() -> MasterAppkey {
    MasterAppkey::derive_from_rootkey(g!(2 * G).normalize())
}

fn outpoint(vout: u32) -> OutPoint {
    OutPoint {
        txid: Txid::from_byte_array([9u8; 32]),
        vout,
    }
}

fn foreign_spk(tag: u8) -> ScriptBuf {
    ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[tag; 32][..]].concat())
}

fn signature(tag: u8) -> EncodedSignature {
    // Any 64 bytes that parse as a schnorr signature; only their placement is under test.
    let mut bytes = [1u8; 64];
    bytes[0] = tag;
    EncodedSignature(bytes)
}

/// `owned` names which input positions belong to us; the rest are foreign.
fn template_with(owned: &[usize], total_inputs: usize) -> TransactionTemplate {
    let key = key();
    let mut template = TransactionTemplate::new();

    for i in 0..total_inputs {
        let value = Amount::from_sat(100_000);
        if let Some(nth) = owned.iter().position(|o| *o == i) {
            let path = BitcoinBip32Path::external(idx(nth as u32));
            let txout = TxOut {
                value,
                script_pubkey: LocalSpk {
                    master_appkey: key,
                    bip32_path: path,
                }
                .spk(),
            };
            template
                .push_owned_input(
                    PushInput::spend_outpoint(&txout, outpoint(i as u32)),
                    LocalSpk {
                        master_appkey: key,
                        bip32_path: path,
                    },
                )
                .unwrap();
        } else {
            let txout = TxOut {
                value,
                script_pubkey: foreign_spk(i as u8),
            };
            template.push_foreign_input(PushInput::spend_outpoint(&txout, outpoint(i as u32)));
        }
    }

    template.push_foreign_output(TxOut {
        value: Amount::from_sat(50_000),
        script_pubkey: foreign_spk(0xff),
    });
    template
}

#[test]
fn a_foreign_input_before_ours_does_not_take_our_signature() {
    let template = template_with(&[1], 2);

    let sigs = [signature(7)];
    let pairs = template
        .as_seen_by(key())
        .signatures_by_input_index(&sigs)
        .unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0].0, 1,
        "the signature belongs to input 1, not input 0"
    );

    let tx = template
        .as_seen_by(key())
        .to_signed_rust_bitcoin_tx(&[signature(7)])
        .unwrap();
    assert!(
        tx.input[0].witness.is_empty(),
        "the foreign input must be left alone"
    );
    assert!(!tx.input[1].witness.is_empty());
}

#[test]
fn foreign_inputs_on_both_sides_of_ours() {
    let template = template_with(&[1, 2], 4);

    let sigs = [signature(1), signature(2)];
    let pairs = template
        .as_seen_by(key())
        .signatures_by_input_index(&sigs)
        .unwrap();

    assert_eq!(
        pairs.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(pairs[0].1 .0[0], 1);
    assert_eq!(pairs[1].1 .0[0], 2);

    let tx = template
        .as_seen_by(key())
        .to_signed_rust_bitcoin_tx(&[signature(1), signature(2)])
        .unwrap();
    assert!(tx.input[0].witness.is_empty());
    assert!(!tx.input[1].witness.is_empty());
    assert!(!tx.input[2].witness.is_empty());
    assert!(tx.input[3].witness.is_empty());
}

#[test]
fn every_input_ours_is_unchanged() {
    let template = template_with(&[0, 1], 2);

    let sigs = [signature(1), signature(2)];
    let pairs = template
        .as_seen_by(key())
        .signatures_by_input_index(&sigs)
        .unwrap();

    assert_eq!(
        pairs.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn too_few_signatures_is_rejected_rather_than_truncated() {
    let template = template_with(&[1, 2], 4);

    let sigs = [signature(1)];
    let err = template
        .as_seen_by(key())
        .signatures_by_input_index(&sigs)
        .unwrap_err();

    assert_eq!((err.expected, err.got), (2, 1));
    assert!(template
        .as_seen_by(key())
        .to_signed_rust_bitcoin_tx(&[signature(1)])
        .is_err());
}

#[test]
fn too_many_signatures_is_rejected() {
    let template = template_with(&[1], 2);

    let sigs = [signature(1), signature(2)];
    let err = template
        .as_seen_by(key())
        .signatures_by_input_index(&sigs)
        .unwrap_err();

    assert_eq!((err.expected, err.got), (1, 2));
}

/// An input paying a script we also send to is still a foreign input. Asking "is this spk
/// one of ours" cannot tell the difference; asking the template can.
#[test]
fn a_foreign_input_at_one_of_our_own_output_scripts_is_still_foreign() {
    let key = key();
    let our_path = BitcoinBip32Path::internal(idx(3));
    let our_spk = LocalSpk {
        master_appkey: key,
        bip32_path: our_path,
    }
    .spk();

    let mut template = TransactionTemplate::new();

    let decoy = TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: our_spk.clone(),
    };
    template.push_foreign_input(PushInput::spend_outpoint(&decoy, outpoint(0)));

    let ours = TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: LocalSpk {
            master_appkey: key,
            bip32_path: BitcoinBip32Path::external(idx(0)),
        }
        .spk(),
    };
    template
        .push_owned_input(
            PushInput::spend_outpoint(&ours, outpoint(1)),
            LocalSpk {
                master_appkey: key,
                bip32_path: BitcoinBip32Path::external(idx(0)),
            },
        )
        .unwrap();

    template
        .push_owned_output_checked(
            &TxOut {
                value: Amount::from_sat(150_000),
                script_pubkey: our_spk,
            },
            LocalSpk {
                master_appkey: key,
                bip32_path: our_path,
            },
        )
        .unwrap();

    let sigs = [signature(7)];
    let pairs = template
        .as_seen_by(key)
        .signatures_by_input_index(&sigs)
        .unwrap();
    assert_eq!(
        pairs.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![1],
        "input 0 pays a script we own but the template never owned that input"
    );
}

/// `is_mine` in the app is built from this. Sourcing it from a wallet index instead answers
/// "has the wallet derived this spk yet", which is bounded by lookahead.
#[test]
fn our_spks_covers_both_sides_at_any_depth() {
    let key = key();
    let deep_input = BitcoinBip32Path::external(idx(500_000));
    let deep_output = BitcoinBip32Path::internal(idx(400_000));

    let mut template = TransactionTemplate::new();

    let foreign = TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: foreign_spk(0xaa),
    };
    template.push_foreign_input(PushInput::spend_outpoint(&foreign, outpoint(0)));

    let ours = TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: LocalSpk {
            master_appkey: key,
            bip32_path: deep_input,
        }
        .spk(),
    };
    template
        .push_owned_input(
            PushInput::spend_outpoint(&ours, outpoint(1)),
            LocalSpk {
                master_appkey: key,
                bip32_path: deep_input,
            },
        )
        .unwrap();

    template
        .push_owned_output_checked(
            &TxOut {
                value: Amount::from_sat(150_000),
                script_pubkey: LocalSpk {
                    master_appkey: key,
                    bip32_path: deep_output,
                }
                .spk(),
            },
            LocalSpk {
                master_appkey: key,
                bip32_path: deep_output,
            },
        )
        .unwrap();
    template.push_foreign_output(TxOut {
        value: Amount::from_sat(40_000),
        script_pubkey: foreign_spk(0xbb),
    });

    let owned = template.as_seen_by(key).our_spks();

    assert_eq!(
        owned.len(),
        2,
        "one input and one output, no foreign scripts"
    );
    assert_eq!(
        owned.get(&ours.script_pubkey).copied(),
        Some(deep_input),
        "an input far past any lookahead is still ours"
    );
    assert_eq!(
        owned
            .get(
                &LocalSpk {
                    master_appkey: key,
                    bip32_path: deep_output
                }
                .spk()
            )
            .copied(),
        Some(deep_output),
        "an output far past any lookahead is still ours"
    );
    assert!(!owned.contains_key(&foreign_spk(0xaa)));
    assert!(!owned.contains_key(&foreign_spk(0xbb)));
}

/// A template can carry another key's scripts. Seen by ours they are foreign: an input we
/// cannot sign and an output that is money leaving — which is what `Foreign` already means to
/// every reader, so none of them can be wrong about it.
#[test]
fn another_keys_scripts_become_foreign() {
    let ours = key();
    let sibling = MasterAppkey::derive_from_rootkey(g!(3 * G).normalize());
    let our_path = BitcoinBip32Path::external(idx(1));
    let their_path = BitcoinBip32Path::external(idx(1));

    let our_spk = LocalSpk {
        master_appkey: ours,
        bip32_path: our_path,
    };
    let their_spk = LocalSpk {
        master_appkey: sibling,
        bip32_path: their_path,
    };

    let mut template = TransactionTemplate::new();
    let our_input = TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: our_spk.spk(),
    };
    template
        .push_owned_input(
            PushInput::spend_outpoint(&our_input, outpoint(0)),
            our_spk.clone(),
        )
        .unwrap();
    let their_input = TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: their_spk.spk(),
    };
    template
        .push_owned_input(
            PushInput::spend_outpoint(&their_input, outpoint(1)),
            their_spk.clone(),
        )
        .unwrap();
    template
        .push_owned_output_checked(
            &TxOut {
                value: Amount::from_sat(150_000),
                script_pubkey: their_spk.spk(),
            },
            their_spk.clone(),
        )
        .unwrap();

    let mine = template.as_seen_by(ours);

    assert_eq!(
        mine.iter_our_inputs()
            .map(|(i, _, _)| i)
            .collect::<Vec<_>>(),
        vec![0],
        "their input is not ours to sign"
    );
    assert_eq!(
        mine.signatures_by_input_index(&[signature(7)])
            .unwrap()
            .into_iter()
            .map(|(i, _)| i)
            .collect::<Vec<_>>(),
        vec![0],
        "our one signature belongs on our one input"
    );
    assert!(!mine.our_spks().contains_key(&their_spk.spk()));
    assert_eq!(
        mine.foreign_recipients().collect::<Vec<_>>(),
        vec![(their_spk.spk().as_script(), 150_000)],
        "their output is a recipient, at their address and for its real value"
    );
    assert!(
        mine.feerate().is_none(),
        "we cannot witness their input, so the weight is unknown"
    );
    assert_eq!(
        mine.our_net_value(),
        -100_000,
        "our balance moves by our own input alone; their side is not ours to count"
    );
    assert_eq!(
        mine.foreign_net_values(),
        BTreeMap::from([(their_spk.spk(), 50_000)]),
        "netted against what they spend, where foreign_recipients reports the gross 150_000"
    );

    // The other direction is the same story with the keys swapped.
    let theirs = template.as_seen_by(sibling);
    assert_eq!(
        theirs
            .iter_our_inputs()
            .map(|(i, _, _)| i)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(theirs.foreign_recipients().count(), 0);
    assert_eq!(theirs.our_net_value(), 50_000);
    assert_eq!(
        theirs.foreign_net_values(),
        BTreeMap::from([(our_spk.spk(), -100_000)]),
        "we are the foreign party from their side, and we only spend"
    );

    // Nothing was mutated: the original still knows who owns what. It cannot be *asked*
    // whose they are without naming a key, which is the point, so read the owners directly.
    assert_eq!(
        template
            .inputs()
            .iter()
            .filter(|input| input.owner().local_owner().is_some())
            .count(),
        2
    );
}
