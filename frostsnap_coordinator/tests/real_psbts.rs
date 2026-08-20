//! The unit tests build `Psbt` values in memory; these start from bytes a third party wrote,
//! so deserialization and the annotations a real producer emits are covered too.

use bdk_chain::bitcoin::{Amount, Psbt};
use frostsnap_core::{
    bitcoin_transaction::TransactionTemplate,
    bitcoin_transaction::{LocalSpk, SpkOwner},
    schnorr_fun::fun::{g, G},
    tweak::{BitcoinBip32Path, NormalIndex},
    MasterAppkey,
};

fn idx(n: u32) -> NormalIndex {
    NormalIndex::new(n).expect("test index is in range")
}

fn wallet_key() -> MasterAppkey {
    MasterAppkey::derive_from_rootkey(g!(2 * G).normalize())
}

fn owned(key: MasterAppkey, path: BitcoinBip32Path) -> Option<LocalSpk> {
    Some(LocalSpk {
        master_appkey: key,
        bip32_path: path,
    })
}

/// Core annotates an output paying the wallet's own *receive* address, not just its change.
/// Recognising it is what lets a self-send be shown as such rather than as money leaving.
#[test]
fn core_self_send_recognises_both_the_payment_and_the_change() {
    let key = wallet_key();
    let psbt = Psbt::deserialize(include_bytes!("fixtures/core_selfsend.psbt")).unwrap();

    let template = TransactionTemplate::from_psbt(&psbt, key).unwrap();

    assert_eq!(template.inputs().len(), 1);
    assert!(template.inputs()[0].owner().local_owner().is_some());

    assert_eq!(
        template.outputs()[0].owner().local_owner().cloned(),
        owned(key, BitcoinBip32Path::internal(idx(0)))
    );
    assert_eq!(
        template.outputs()[1].owner().local_owner().cloned(),
        owned(key, BitcoinBip32Path::external(idx(1))),
        "the payment goes to our own receive address"
    );
    assert_eq!(
        template.outputs()[1].value,
        Amount::from_btc(1.5).unwrap().to_sat()
    );
    assert!(
        template.as_seen_by(key).our_net_value() > -10_000,
        "a self-send moves nothing out beyond the fee"
    );
}

#[test]
fn core_payment_to_a_stranger_leaves_their_output_foreign() {
    let key = wallet_key();
    let psbt = Psbt::deserialize(include_bytes!("fixtures/core_stranger.psbt")).unwrap();

    let template = TransactionTemplate::from_psbt(&psbt, key).unwrap();

    let recipient_spk = psbt.unsigned_tx.output[0].script_pubkey.clone();
    assert_eq!(
        template.outputs()[0].owner(),
        &SpkOwner::Foreign(recipient_spk),
        "Core writes no derivation for an output it doesn't own"
    );
    assert_eq!(
        template.outputs()[1].owner().local_owner().cloned(),
        owned(key, BitcoinBip32Path::internal(idx(1))),
        "the change still comes back to us"
    );
}

/// Paying an address on our own change keychain: both outputs are internal, which is the
/// shape a real Sparrow self-send took. Nothing distinguishes the payment from the change
/// except which one the wallet chose, so both have to come back as ours.
#[test]
fn core_payment_to_our_own_change_address_owns_both_outputs() {
    let key = wallet_key();
    let psbt = Psbt::deserialize(include_bytes!("fixtures/core_to_internal.psbt")).unwrap();

    let template = TransactionTemplate::from_psbt(&psbt, key).unwrap();

    assert_eq!(
        template.outputs()[0].owner().local_owner().cloned(),
        owned(key, BitcoinBip32Path::internal(idx(2)))
    );
    assert_eq!(
        template.outputs()[1].owner().local_owner().cloned(),
        owned(key, BitcoinBip32Path::internal(idx(3)))
    );
    assert_eq!(
        template.as_seen_by(key).foreign_recipients().count(),
        0,
        "nothing here leaves the wallet"
    );
}

/// Core derived these scripts from the wallet descriptor; we re-derive them from the master
/// appkey and the path the PSBT claims. That the transform accepts them at all is the two
/// derivations agreeing — a mismatch is what `push_owned_output_checked` rejects.
#[test]
fn our_derivation_agrees_with_bitcoin_core() {
    let key = wallet_key();

    for bytes in [
        include_bytes!("fixtures/core_selfsend.psbt").as_slice(),
        include_bytes!("fixtures/core_stranger.psbt").as_slice(),
        include_bytes!("fixtures/core_to_internal.psbt").as_slice(),
    ] {
        let psbt = Psbt::deserialize(bytes).unwrap();
        let template = TransactionTemplate::from_psbt(&psbt, key).unwrap();

        assert_eq!(
            template.to_rust_bitcoin_tx().output,
            psbt.unsigned_tx.output,
            "the transaction we would sign must be the one the PSBT describes"
        );
    }
}

/// `Transaction::details` used to source prevouts from the wallet's own graph, which cannot
/// contain a PSBT's foreign inputs — and one missing prevout is enough to make the fee
/// unknowable. The template carries every prevout the PSBT declared, which is why building
/// from it fixes the fee display rather than papering over it.
#[test]
fn a_template_from_a_real_psbt_carries_every_prevout() {
    let key = wallet_key();

    for bytes in [
        include_bytes!("fixtures/core_selfsend.psbt").as_slice(),
        include_bytes!("fixtures/core_stranger.psbt").as_slice(),
        include_bytes!("fixtures/core_to_internal.psbt").as_slice(),
    ] {
        let psbt = Psbt::deserialize(bytes).unwrap();
        let template = TransactionTemplate::from_psbt(&psbt, key).unwrap();

        assert_eq!(
            template.inputs().len(),
            psbt.unsigned_tx.input.len(),
            "every input is represented"
        );
        assert!(
            template.fee().is_some(),
            "a template knows its own fee without consulting a wallet"
        );
    }
}
