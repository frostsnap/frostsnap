use bitcoin::{Amount, ScriptBuf, TxOut};
use frostsnap_core::bitcoin_transaction::{LocalSpk, PromptSignBitcoinTx, TransactionTemplate};
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_core::MasterAppkey;
use schnorr_fun::fun::G;

fn local_spk(bip32_path: BitcoinBip32Path) -> LocalSpk {
    LocalSpk {
        master_appkey: MasterAppkey::derive_from_rootkey(G.normalize()),
        bip32_path,
    }
}

fn template_with_input(input_sats: u64) -> TransactionTemplate {
    let mut template = TransactionTemplate::new();
    template.push_imaginary_owned_input(
        local_spk(BitcoinBip32Path::external(0)),
        Amount::from_sat(input_sats),
    );
    template
}

fn push_foreign(template: &mut TransactionTemplate, sats: u64) {
    let spk =
        ScriptBuf::from_hex("5120a62baa9e7c1aeda63492f2129cc8226a39db1bc05a9c11e45a61cb751a11061d")
            .unwrap();
    template.push_foreign_output(TxOut {
        value: Amount::from_sat(sats),
        script_pubkey: spk,
    });
}

fn push_to_self(template: &mut TransactionTemplate, sats: u64, index: u32) {
    template.push_owned_output(
        Amount::from_sat(sats),
        local_spk(BitcoinBip32Path::external(index)),
    );
}

fn push_change(template: &mut TransactionTemplate, sats: u64, index: u32) {
    template.push_owned_output(
        Amount::from_sat(sats),
        local_spk(BitcoinBip32Path::internal(index)),
    );
}

fn prompt(template: &TransactionTemplate) -> PromptSignBitcoinTx {
    template.user_prompt(bitcoin::Network::Bitcoin)
}

fn summary(prompt: &PromptSignBitcoinTx) -> Vec<(u64, Option<BitcoinBip32Path>)> {
    prompt
        .recipients
        .iter()
        .map(|r| (r.amount.to_sat(), r.owned))
        .collect()
}

#[test]
fn ordinary_send_hides_the_single_change_output() {
    let mut template = template_with_input(100_000);
    push_foreign(&mut template, 60_000);
    push_change(&mut template, 39_000, 0);

    let prompt = prompt(&template);
    assert_eq!(summary(&prompt), vec![(60_000, None)]);
    assert_eq!(prompt.value_moved(), Amount::from_sat(60_000));
}

#[test]
fn foreign_only_has_no_owned_recipients() {
    let mut template = template_with_input(100_000);
    push_foreign(&mut template, 99_000);

    let prompt = prompt(&template);
    assert_eq!(summary(&prompt), vec![(99_000, None)]);
    assert_eq!(prompt.value_moved(), Amount::from_sat(99_000));
}

#[test]
fn send_that_also_pays_our_own_external_address_discloses_it_in_order() {
    let mut template = template_with_input(100_000);
    push_foreign(&mut template, 50_000);
    push_to_self(&mut template, 30_000, 1);
    push_change(&mut template, 19_000, 0);

    let prompt = prompt(&template);
    assert_eq!(
        summary(&prompt),
        vec![
            (50_000, None),
            (30_000, Some(BitcoinBip32Path::external(1))),
        ]
    );
    assert_eq!(prompt.value_moved(), Amount::from_sat(80_000));
}

#[test]
fn pure_self_spend_discloses_everything_including_change() {
    let mut template = template_with_input(100_000);
    push_to_self(&mut template, 70_000, 1);
    push_change(&mut template, 29_000, 0);

    let prompt = prompt(&template);
    assert_eq!(
        summary(&prompt),
        vec![
            (70_000, Some(BitcoinBip32Path::external(1))),
            (29_000, Some(BitcoinBip32Path::internal(0))),
        ]
    );
    assert_eq!(prompt.value_moved(), Amount::from_sat(99_000));
}

#[test]
fn consolidation_to_change_only_is_still_disclosed() {
    let mut template = template_with_input(100_000);
    push_change(&mut template, 99_000, 0);

    let prompt = prompt(&template);
    assert_eq!(
        summary(&prompt),
        vec![(99_000, Some(BitcoinBip32Path::internal(0)))]
    );
}

#[test]
fn two_change_outputs_with_a_foreign_recipient_are_disclosed() {
    let mut template = template_with_input(100_000);
    push_foreign(&mut template, 50_000);
    push_change(&mut template, 25_000, 0);
    push_change(&mut template, 24_000, 1);

    let prompt = prompt(&template);
    assert_eq!(
        summary(&prompt),
        vec![
            (50_000, None),
            (25_000, Some(BitcoinBip32Path::internal(0))),
            (24_000, Some(BitcoinBip32Path::internal(1))),
        ]
    );
    assert_eq!(prompt.value_moved(), Amount::from_sat(99_000));
}
