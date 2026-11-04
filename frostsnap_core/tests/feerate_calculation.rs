use bitcoin::{Amount, ScriptBuf, TxOut};
use frostsnap_core::bitcoin_transaction::{LocalSpk, TransactionTemplate};
use frostsnap_core::tweak::{
    AppTweak, BitcoinBip32Path, BitcoinTweak, BitcoinTweakKind, NormalIndex,
};
use frostsnap_core::MasterAppkey;
use schnorr_fun::fun::G;

/// Test that TransactionTemplate.feerate() correctly estimates the feerate
/// of a signed transaction based on real signet transaction:
/// https://mempool.space/signet/tx/0c6f19d7f0544543df6ccb0f853a2518e60edd505acbe3111a098900d9b3033d
///
/// This transaction has:
/// - 2 taproot keyspend inputs (5,000 + 11,000 sats)
/// - 1 output (15,831 sats)
/// - Fee: 169 sats
/// - Weight: 674 WU
/// - vSize: 168.5 bytes
#[test]
fn test_feerate_estimation_accuracy() {
    let mut template = TransactionTemplate::new();

    // Create dummy master appkey (doesn't matter for weight calculation)
    let master_appkey = MasterAppkey::derive_from_rootkey(G.normalize());

    // Create dummy local SPK for the inputs
    let local_spk =
        LocalSpk::key_spend(master_appkey, BitcoinBip32Path::external(NormalIndex::ZERO));

    // Add 2 owned inputs matching the real transaction amounts
    template.push_imaginary_owned_input(local_spk.clone(), Amount::from_sat(5_000));
    template.push_imaginary_owned_input(local_spk, Amount::from_sat(11_000));

    // Use the actual output scriptPubKey from the real transaction
    let output_script =
        ScriptBuf::from_hex("5120a62baa9e7c1aeda63492f2129cc8226a39db1bc05a9c11e45a61cb751a11061d")
            .unwrap();

    // Add output matching the real transaction
    template.push_foreign_output(TxOut {
        value: Amount::from_sat(15_831),
        script_pubkey: output_script,
    });

    // Get the estimated feerate from template
    let template_feerate = template
        .as_seen_by(master_appkey)
        .feerate()
        .expect("should calculate feerate");
    let template_fee = template.fee().expect("should calculate fee");

    // The real transaction has these metrics
    const EXPECTED_VSIZE: f64 = 168.5;
    const EXPECTED_FEE: u64 = 169;
    const EXPECTED_FEERATE: f64 = EXPECTED_FEE as f64 / EXPECTED_VSIZE;

    // The fee should match exactly
    assert_eq!(
        template_fee, EXPECTED_FEE,
        "Fee mismatch: template calculated {}, expected {}",
        template_fee, EXPECTED_FEE
    );

    // The feerate should match within 0.1 sat/vB
    let difference = (template_feerate - EXPECTED_FEERATE).abs();
    assert!(
        difference < 0.1,
        "Feerate estimation is off by {:.2} sat/vB. Expected {:.2}, got {:.2}",
        difference,
        EXPECTED_FEERATE,
        template_feerate
    );
}

/// A script-path input's witness is its signature, leaf script and control block. The feerate
/// must weigh all three, and agree with rust-bitcoin's weight for the transaction witnessed that
/// way independently of our own witness code.
#[test]
fn feerate_weighs_a_script_path_witness() {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let master_appkey = MasterAppkey::derive_from_rootkey(G.normalize());
    let leaf_path = BitcoinBip32Path::external(NormalIndex::new(1).unwrap());
    let leaf_key = AppTweak::Bitcoin(BitcoinTweak {
        bip32_path: leaf_path,
        kind: BitcoinTweakKind::ScriptSpend,
    })
    .derive_xonly_key(&master_appkey.to_xpub());
    let leaf_script = bitcoin::script::Builder::new()
        .push_x_only_key(&leaf_key.into())
        .push_opcode(bitcoin::opcodes::all::OP_CHECKSIG)
        .into_script();
    // A second leaf so the control block carries a merkle path, not just the internal key.
    let other_leaf = bitcoin::script::Builder::new()
        .push_int(144)
        .push_opcode(bitcoin::opcodes::all::OP_CSV)
        .into_script();
    let internal_key = bitcoin::secp256k1::Keypair::from_seckey_slice(&secp, &[9u8; 32])
        .unwrap()
        .x_only_public_key()
        .0;
    let spend_info = bitcoin::taproot::TaprootBuilder::new()
        .add_leaf(1, leaf_script.clone())
        .unwrap()
        .add_leaf(1, other_leaf)
        .unwrap()
        .finalize(&secp, internal_key)
        .unwrap();
    let control_block = spend_info
        .control_block(&(
            leaf_script.clone(),
            bitcoin::taproot::LeafVersion::TapScript,
        ))
        .unwrap();

    let mut template = TransactionTemplate::new();
    template.push_imaginary_owned_input(
        LocalSpk::key_spend(master_appkey, BitcoinBip32Path::external(NormalIndex::ZERO)),
        Amount::from_sat(50_000),
    );
    template.push_imaginary_owned_input(
        LocalSpk::script_spend(
            master_appkey,
            ScriptBuf::new_p2tr(&secp, internal_key, spend_info.merkle_root()),
            vec![frostsnap_core::bitcoin_transaction::LeafSpend::new(
                leaf_path,
                leaf_script.clone(),
                bitcoin::taproot::LeafVersion::TapScript,
                control_block.serialize(),
            )],
        ),
        Amount::from_sat(50_000),
    );
    template.push_foreign_output(TxOut {
        value: Amount::from_sat(99_000),
        script_pubkey: ScriptBuf::from_hex(
            "5120a62baa9e7c1aeda63492f2129cc8226a39db1bc05a9c11e45a61cb751a11061d",
        )
        .unwrap(),
    });

    let mut witnessed = template.to_rust_bitcoin_tx();
    let signature = [7u8; 64];
    witnessed.input[0].witness = bitcoin::Witness::from_slice(&[signature]);
    witnessed.input[1].witness = bitcoin::Witness::from_slice(&[
        signature.to_vec(),
        leaf_script.to_bytes(),
        control_block.serialize(),
    ]);
    let expected = template.fee().unwrap() as f64 / witnessed.weight().to_vbytes_ceil() as f64;

    assert_eq!(template.as_seen_by(master_appkey).feerate(), Some(expected));
}

/// With our keys in two leaves, whichever leaf is finally satisfied is not ours to know, so the
/// feerate assumes the larger witness — never overstating the rate the user pays.
#[test]
fn feerate_assumes_the_largest_of_several_leaves() {
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let master_appkey = MasterAppkey::derive_from_rootkey(G.normalize());
    let key_at = |index: u32| -> bitcoin::XOnlyPublicKey {
        AppTweak::Bitcoin(BitcoinTweak {
            bip32_path: BitcoinBip32Path::external(NormalIndex::new(index).unwrap()),
            kind: BitcoinTweakKind::ScriptSpend,
        })
        .derive_xonly_key(&master_appkey.to_xpub())
        .into()
    };
    let small_leaf = bitcoin::script::Builder::new()
        .push_x_only_key(&key_at(1))
        .push_opcode(bitcoin::opcodes::all::OP_CHECKSIG)
        .into_script();
    let large_leaf = bitcoin::script::Builder::new()
        .push_x_only_key(&key_at(2))
        .push_opcode(bitcoin::opcodes::all::OP_CHECKSIGVERIFY)
        .push_int(52_560)
        .push_opcode(bitcoin::opcodes::all::OP_CSV)
        .into_script();
    let internal_key = bitcoin::secp256k1::Keypair::from_seckey_slice(&secp, &[9u8; 32])
        .unwrap()
        .x_only_public_key()
        .0;
    let spend_info = bitcoin::taproot::TaprootBuilder::new()
        .add_leaf(1, small_leaf.clone())
        .unwrap()
        .add_leaf(1, large_leaf.clone())
        .unwrap()
        .finalize(&secp, internal_key)
        .unwrap();
    let control_block = |leaf: &ScriptBuf| {
        spend_info
            .control_block(&(leaf.clone(), bitcoin::taproot::LeafVersion::TapScript))
            .unwrap()
            .serialize()
    };
    let leaf_spend = |index: u32, leaf: &ScriptBuf| {
        frostsnap_core::bitcoin_transaction::LeafSpend::new(
            BitcoinBip32Path::external(NormalIndex::new(index).unwrap()),
            leaf.clone(),
            bitcoin::taproot::LeafVersion::TapScript,
            control_block(leaf),
        )
    };

    let mut template = TransactionTemplate::new();
    template.push_imaginary_owned_input(
        LocalSpk::script_spend(
            master_appkey,
            ScriptBuf::new_p2tr(&secp, internal_key, spend_info.merkle_root()),
            vec![leaf_spend(1, &small_leaf), leaf_spend(2, &large_leaf)],
        ),
        Amount::from_sat(50_000),
    );
    template.push_foreign_output(TxOut {
        value: Amount::from_sat(49_000),
        script_pubkey: ScriptBuf::from_hex(
            "5120a62baa9e7c1aeda63492f2129cc8226a39db1bc05a9c11e45a61cb751a11061d",
        )
        .unwrap(),
    });

    let feerate_witnessed_with = |leaf: &ScriptBuf| {
        let mut tx = template.to_rust_bitcoin_tx();
        tx.input[0].witness =
            bitcoin::Witness::from_slice(&[vec![7u8; 64], leaf.to_bytes(), control_block(leaf)]);
        template.fee().unwrap() as f64 / tx.weight().to_vbytes_ceil() as f64
    };
    let feerate = template.as_seen_by(master_appkey).feerate();

    assert_eq!(feerate, Some(feerate_witnessed_with(&large_leaf)));
    assert_ne!(feerate, Some(feerate_witnessed_with(&small_leaf)));
}
