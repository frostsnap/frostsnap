use core::str::FromStr;
use frostsnap_core::bitcoin_transaction::{PromptRecipient, PromptSignBitcoinTx};
use frostsnap_core::tweak::BitcoinBip32Path;
use frostsnap_widgets::sign_prompt::{
    AddressPage, AmountPage, ConfirmationPage, FeePage, SignPromptPageList, WarningPage,
};
use frostsnap_widgets::widget_list::WidgetList;

fn prompt(recipients: &[(u64, Option<BitcoinBip32Path>)], fee: u64) -> PromptSignBitcoinTx {
    let address = bitcoin::Address::from_str(
        "bc1p5d7rjq7g6rdk2yhzks9smlaqtedr4dekq08ge8ztwac72sfr9rusxg3297",
    )
    .unwrap()
    .assume_checked();
    PromptSignBitcoinTx {
        recipients: recipients
            .iter()
            .map(|&(sats, owned)| PromptRecipient {
                address: address.clone(),
                amount: bitcoin::Amount::from_sat(sats),
                owned,
            })
            .collect(),
        fee: bitcoin::Amount::from_sat(fee),
        fee_rate_sats_per_vbyte: Some(1.0),
    }
}

fn pages(prompt: PromptSignBitcoinTx) -> SignPromptPageList {
    SignPromptPageList::new_with_seed(prompt, 0)
}

fn page_is<W: 'static>(list: &SignPromptPageList, index: usize) -> bool {
    list.get(index)
        .expect("page exists")
        .widget
        .downcast_ref::<W>()
        .is_some()
}

#[test]
fn owned_recipient_gets_ordinary_amount_and_address_pages() {
    let list = pages(prompt(
        &[(1_000_000, Some(BitcoinBip32Path::external(0)))],
        10_000,
    ));
    assert_eq!(list.len(), 4);
    assert!(page_is::<AmountPage>(&list, 0));
    assert!(page_is::<AddressPage>(&list, 1));
    assert!(page_is::<FeePage>(&list, 2));
    assert!(page_is::<ConfirmationPage>(&list, 3));
}

#[test]
fn pure_self_spend_can_trigger_the_proportional_fee_warning() {
    let list = pages(prompt(
        &[(1_000_000, Some(BitcoinBip32Path::external(0)))],
        60_000,
    ));
    assert_eq!(list.len(), 5);
    assert!(page_is::<AmountPage>(&list, 0));
    assert!(page_is::<AddressPage>(&list, 1));
    assert!(page_is::<WarningPage>(&list, 2));
    assert!(page_is::<FeePage>(&list, 3));
    assert!(page_is::<ConfirmationPage>(&list, 4));
}

#[test]
fn ordinary_send_pages_are_unchanged() {
    let list = pages(prompt(&[(500_000, None)], 10_000));
    assert_eq!(list.len(), 4);
    assert!(page_is::<AmountPage>(&list, 0));
    assert!(page_is::<AddressPage>(&list, 1));
    assert!(page_is::<FeePage>(&list, 2));
    assert!(page_is::<ConfirmationPage>(&list, 3));
}

#[test]
fn owned_recipient_rides_in_the_normal_page_sequence() {
    let list = pages(prompt(
        &[
            (500_000, None),
            (300_000, Some(BitcoinBip32Path::external(1))),
        ],
        10_000,
    ));
    assert_eq!(list.len(), 6);
    assert!(page_is::<AmountPage>(&list, 0));
    assert!(page_is::<AddressPage>(&list, 1));
    assert!(page_is::<AmountPage>(&list, 2));
    assert!(page_is::<AddressPage>(&list, 3));
    assert!(page_is::<FeePage>(&list, 4));
    assert!(page_is::<ConfirmationPage>(&list, 5));
}

/// The warning depends on what leaves the wallet, so adding an output that comes back to us must
/// not change whether it fires. Same foreign value, same fee, same verdict.
#[test]
fn owned_recipients_do_not_change_the_fee_warning() {
    let foreign_only = pages(prompt(&[(1_000_000, None)], 60_000));
    assert_eq!(foreign_only.len(), 5);
    assert!(page_is::<WarningPage>(&foreign_only, 2));

    let with_owned = pages(prompt(
        &[
            (1_000_000, None),
            (1_000_000, Some(BitcoinBip32Path::external(0))),
        ],
        60_000,
    ));
    assert_eq!(with_owned.len(), 7);
    assert!(page_is::<WarningPage>(&with_owned, 4));
}

#[test]
fn absolute_fee_threshold_fires_even_when_little_moves() {
    let list = pages(prompt(
        &[(10_000, Some(BitcoinBip32Path::external(0)))],
        150_000,
    ));
    assert_eq!(list.len(), 5);
    assert!(page_is::<WarningPage>(&list, 2));
}

/// A self-payment must not dilute the value the proportional warning measures against. The fee
/// here is 30% of the 200_000 actually leaving and sits below the absolute threshold, so the
/// proportional rule is the only thing that can catch it.
#[test]
fn an_owned_output_does_not_dilute_the_foreign_value_the_warning_measures() {
    let list = pages(prompt(
        &[
            (200_000, None),
            (1_000_000, Some(BitcoinBip32Path::external(0))),
        ],
        60_000,
    ));
    assert_eq!(list.len(), 7);
    assert!(page_is::<WarningPage>(&list, 4));
}
