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
    let pairs = template.signatures_by_input_index(&sigs).unwrap();

    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0].0, 1,
        "the signature belongs to input 1, not input 0"
    );

    let tx = template.to_signed_rust_bitcoin_tx(&[signature(7)]).unwrap();
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
    let pairs = template.signatures_by_input_index(&sigs).unwrap();

    assert_eq!(
        pairs.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(pairs[0].1 .0[0], 1);
    assert_eq!(pairs[1].1 .0[0], 2);

    let tx = template
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
    let pairs = template.signatures_by_input_index(&sigs).unwrap();

    assert_eq!(
        pairs.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn too_few_signatures_is_rejected_rather_than_truncated() {
    let template = template_with(&[1, 2], 4);

    let sigs = [signature(1)];
    let err = template.signatures_by_input_index(&sigs).unwrap_err();

    assert_eq!((err.expected, err.got), (2, 1));
    assert!(template.to_signed_rust_bitcoin_tx(&[signature(1)]).is_err());
}

#[test]
fn too_many_signatures_is_rejected() {
    let template = template_with(&[1], 2);

    let sigs = [signature(1), signature(2)];
    let err = template.signatures_by_input_index(&sigs).unwrap_err();

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
    let pairs = template.signatures_by_input_index(&sigs).unwrap();
    assert_eq!(
        pairs.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![1],
        "input 0 pays a script we own but the template never owned that input"
    );
}

/// `is_mine` in the app is built from this. Sourcing it from a wallet index instead answers
/// "has the wallet derived this spk yet", which is bounded by lookahead.
#[test]
fn owned_spks_covers_both_sides_at_any_depth() {
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

    let owned = template.owned_spks(key);

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

/// A template can carry scripts belonging to more than one key at the same path. Asking for
/// one key must not hand back the other's script — which, paired with the asking key, is how
/// a PSBT output once got attributed to whichever wallet happened to be signing.
#[test]
fn owned_spks_excludes_scripts_belonging_to_another_key() {
    let ours = key();
    let sibling = MasterAppkey::derive_from_rootkey(g!(3 * G).normalize());
    let our_path = BitcoinBip32Path::external(idx(1));
    let sibling_path = BitcoinBip32Path::external(idx(1));

    let our_spk = LocalSpk {
        master_appkey: ours,
        bip32_path: our_path,
    };
    let sibling_spk = LocalSpk {
        master_appkey: sibling,
        bip32_path: sibling_path,
    };

    let mut template = TransactionTemplate::new();
    let txout = TxOut {
        value: Amount::from_sat(100_000),
        script_pubkey: our_spk.spk(),
    };
    template
        .push_owned_input(
            PushInput::spend_outpoint(&txout, outpoint(0)),
            our_spk.clone(),
        )
        .unwrap();
    template
        .push_owned_output_checked(
            &TxOut {
                value: Amount::from_sat(90_000),
                script_pubkey: sibling_spk.spk(),
            },
            sibling_spk.clone(),
        )
        .unwrap();

    let ours_only = template.owned_spks(ours);
    assert_eq!(ours_only.get(&our_spk.spk()).copied(), Some(our_path));
    assert!(
        !ours_only.contains_key(&sibling_spk.spk()),
        "the sibling's script is not ours to claim"
    );

    let theirs_only = template.owned_spks(sibling);
    assert_eq!(
        theirs_only.get(&sibling_spk.spk()).copied(),
        Some(sibling_path)
    );
    assert!(!theirs_only.contains_key(&our_spk.spk()));
}
