use bitcoin::{Amount, ScriptBuf, TxOut};
use frostsnap_core::bitcoin_transaction::{LocalSpk, TransactionTemplate};
use frostsnap_core::tweak::BitcoinBip32Path;
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
    let local_spk = LocalSpk {
        master_appkey,
        bip32_path: BitcoinBip32Path::external(0),
    };

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
    let template_feerate = template.feerate().expect("should calculate feerate");
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
