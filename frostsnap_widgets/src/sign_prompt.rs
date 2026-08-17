use crate::{
    address_display::AddressDisplay,
    any_of::AnyOf,
    bitcoin_amount_display::BitcoinAmountDisplay,
    gray4_style::Gray4TextStyle,
    page_slider::PageSlider,
    palette::PALETTE,
    prelude::*,
    widget_list::{WidgetList, WidgetListItem},
    GrayToAlpha, HoldToConfirm, Image,
};
use alloc::{boxed::Box, format, string::ToString};
use embedded_graphics::{
    geometry::Size,
    pixelcolor::{Gray8, Rgb565},
};
use frostsnap_core::{bitcoin_transaction::PromptSignBitcoinTx, tweak::BitcoinBip32Path};
use frostsnap_fonts::{
    Gray4Font, NOTO_SANS_17_REGULAR, NOTO_SANS_18_LIGHT, NOTO_SANS_18_MEDIUM, NOTO_SANS_24_BOLD,
};
use tinybmp::Bmp;

const FONT_PAGE_HEADER: &Gray4Font = &NOTO_SANS_18_LIGHT;
const FONT_CONFIRM_TITLE: &Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_CONFIRM_TEXT: &Gray4Font = &NOTO_SANS_17_REGULAR;
const FONT_CAUTION_NOTE: &Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_CAUTION_TITLE: &Gray4Font = &NOTO_SANS_24_BOLD;
const FONT_CAUTION_TEXT: &Gray4Font = &NOTO_SANS_17_REGULAR;

const HIGH_FEE_ABSOLUTE_THRESHOLD_SATS: u64 = 100_000;
const HIGH_FEE_PERCENTAGE_THRESHOLD: u64 = 5;
const HOLD_TO_SIGN_TIME_MS: u32 = 3000;

const FONT_TO_SELF_FOOTNOTE: &Gray4Font = &NOTO_SANS_17_REGULAR;

/// Footnote marking a recipient the signing wallet itself derives, set small and in
/// secondary gray so it annotates the page without competing with the amount or
/// address. The keychain/index pair is the derivation the device attests.
fn to_self_footnote(path: &BitcoinBip32Path) -> Text<Gray4TextStyle> {
    Text::new(
        format!(
            "(to self: {}/{})",
            path.account_keychain.keychain as u32, path.index
        ),
        Gray4TextStyle::new(FONT_TO_SELF_FOOTNOTE, PALETTE.text_secondary),
    )
}

/// Widget list that generates sign prompt pages
#[derive(Clone)]
pub struct SignPromptPageList {
    prompt: PromptSignBitcoinTx,
    total_pages: usize,
    rand_seed: u32,
}

/// Page widget for displaying amount to send
#[derive(frostsnap_macros::Widget)]
pub struct AmountPage {
    #[widget_delegate]
    center: Center<
        Column<(
            Text<Gray4TextStyle>,
            BitcoinAmountDisplay,
            Text<Gray4TextStyle>,
            Option<Text<Gray4TextStyle>>,
        )>,
    >,
}

impl AmountPage {
    #[inline(never)]
    pub fn new(index: usize, amount_sats: u64, owned: Option<&BitcoinBip32Path>) -> Self {
        let title = Text::new(
            format!("Send Amount #{}", index + 1),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );
        let footnote = owned.map(to_self_footnote);

        let amount_display = BitcoinAmountDisplay::new(amount_sats);

        let btc_text = Text::new(
            "BTC".to_string(),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let mut column = Column::new((title, amount_display, btc_text, footnote))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        // Gaps keep an unowned page pixel-identical to the pre-footnote layout: the
        // empty slot contributes no height and no gap.
        column.set_gap(0, 10);
        column.set_gap(1, 10);
        column.set_gap(2, if owned.is_some() { 8 } else { 0 });

        Self {
            center: Center::new(column),
        }
    }
}

/// Page widget for displaying recipient address
#[derive(Clone, frostsnap_macros::Widget)]
pub struct AddressPage {
    #[widget_delegate]
    center: Center<
        Padding<
            Column<(
                Text<Gray4TextStyle>,
                AddressDisplay,
                Option<Text<Gray4TextStyle>>,
            )>,
        >,
    >,
}

impl AddressPage {
    #[inline(never)]
    pub fn new_with_seed(
        index: usize,
        address: &bitcoin::Address,
        rand_seed: u32,
        owned: Option<&BitcoinBip32Path>,
    ) -> Self {
        let title = Text::new(
            format!("To Address #{}", index + 1),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );
        let footnote = owned.map(to_self_footnote);

        let mixed_seed = rand_seed.wrapping_add((index as u32).wrapping_mul(0x9e3779b9));
        let address_display = AddressDisplay::new_with_seed(address.clone(), mixed_seed);

        let mut column = Column::new((title, address_display, footnote))
            .with_main_axis_alignment(MainAxisAlignment::Start);
        column.set_gap(0, 10);
        column.set_gap(1, if owned.is_some() { 8 } else { 0 });
        let padded = Padding::only(column).bottom(40).build();

        Self {
            center: Center::new(padded),
        }
    }
}

/// Page widget for displaying network fee
#[derive(frostsnap_macros::Widget)]
pub struct FeePage {
    #[widget_delegate]
    center: Center<
        Column<(
            Text<Gray4TextStyle>,
            BitcoinAmountDisplay,
            Text<Gray4TextStyle>,
        )>,
    >,
}

impl FeePage {
    #[inline(never)]
    fn new(fee_sats: u64, fee_rate_sats_per_vbyte: Option<f64>) -> Self {
        let title = Text::new(
            "Network Fee".to_string(),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let fee_amount = BitcoinAmountDisplay::new(fee_sats);

        let fee_rate_text = if let Some(rate) = fee_rate_sats_per_vbyte {
            Text::new(
                format!("{:.1} sats/vB", rate),
                Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
            )
        } else {
            Text::new(
                "BTC".to_string(),
                Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
            )
        };

        let mut column = Column::new((title, fee_amount, fee_rate_text))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_uniform_gap(10);

        Self {
            center: Center::new(column),
        }
    }
}

const WARNING_ICON_DATA: &[u8] = include_bytes!("../assets/warning-icon-24x24.bmp");

/// Page widget for high fee warning
#[derive(frostsnap_macros::Widget)]
pub struct WarningPage {
    #[widget_delegate]
    center: Center<
        Column<(
            Row<(
                Image<GrayToAlpha<Bmp<'static, Gray8>, Rgb565>>,
                SizedBox<Rgb565>,
                Column<(SizedBox<Rgb565>, Text<Gray4TextStyle>)>,
            )>,
            Text<Gray4TextStyle>,
            Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>,
        )>,
    >,
}

/// The two lines of the high-fee warning.
///
/// Pure and separately tested because the sentence must name the quantity the rule actually
/// divided by, and nothing about rendering a page can catch it naming a different one. In a mixed
/// send every output is titled "Send Amount #n", owned ones included, so "the amount sent" reads as
/// the total and is wrong: the rule uses only what leaves.
fn warning_lines(
    fee_sats: u64,
    leaves_wallet: bool,
) -> (alloc::string::String, alloc::string::String) {
    if fee_sats > HIGH_FEE_ABSOLUTE_THRESHOLD_SATS {
        ("Fee is greater".to_string(), "than 0.001 BTC".to_string())
    } else if leaves_wallet {
        (
            "Fee exceeds 5% of".to_string(),
            "the amount leaving".to_string(),
        )
    } else {
        (
            "Fee exceeds 5% of".to_string(),
            "the value moved".to_string(),
        )
    }
}

impl WarningPage {
    #[inline(never)]
    fn new(fee_sats: u64, leaves_wallet: bool) -> Self {
        let warning_bmp =
            Bmp::<Gray8>::from_slice(WARNING_ICON_DATA).expect("Failed to load warning icon BMP");
        let warning_icon = Image::new(GrayToAlpha::new(warning_bmp, PALETTE.warning));

        let icon_spacer = SizedBox::<Rgb565>::new(Size::new(5, 1));

        let caution_text = Text::new(
            "Caution".to_string(),
            Gray4TextStyle::new(FONT_CAUTION_NOTE, PALETTE.warning),
        );

        let text_with_spacer =
            Column::new((SizedBox::<Rgb565>::new(Size::new(1, 5)), caution_text));

        let caution_row = Row::new((warning_icon, icon_spacer, text_with_spacer))
            .with_main_axis_alignment(MainAxisAlignment::Center);

        let title_text = Text::new(
            "High Fee".to_string(),
            Gray4TextStyle::new(FONT_CAUTION_TITLE, PALETTE.on_background),
        );

        let (line1, line2) = warning_lines(fee_sats, leaves_wallet);

        let warning_text = Column::new((
            Text::new(
                line1,
                Gray4TextStyle::new(FONT_CAUTION_TEXT, PALETTE.text_secondary),
            ),
            Text::new(
                line2,
                Gray4TextStyle::new(FONT_CAUTION_TEXT, PALETTE.text_secondary),
            ),
        ))
        .with_cross_axis_alignment(CrossAxisAlignment::Center);

        let mut column = Column::new((caution_row, title_text, warning_text))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_uniform_gap(10);

        Self {
            center: Center::new(column),
        }
    }
}

/// Confirmation page with HoldToConfirm
#[derive(frostsnap_macros::Widget)]
pub struct ConfirmationPage {
    #[widget_delegate]
    pub hold_confirm: HoldToConfirm<
        Column<(
            Text<Gray4TextStyle>,
            Column<(Text<Gray4TextStyle>, Text<Gray4TextStyle>)>,
        )>,
    >,
}

impl ConfirmationPage {
    #[inline(never)]
    fn new() -> Self {
        let sign_text = Text::new(
            "Hold to Sign".to_string(),
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.on_background),
        );

        let press_text = Column::new((
            Text::new(
                "Press and hold".to_string(),
                Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
            ),
            Text::new(
                "for 3 seconds".to_string(),
                Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
            ),
        ));

        let mut confirm_content = Column::new((sign_text, press_text))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        confirm_content.set_gap(0, 15);

        let hold_confirm =
            HoldToConfirm::new(HOLD_TO_SIGN_TIME_MS, confirm_content).with_faded_out_button();

        Self { hold_confirm }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_confirm.is_confirmed()
    }

    pub fn is_finished(&self) -> bool {
        self.hold_confirm.is_finished()
    }
}

type SignPromptPage = AnyOf<(
    AmountPage,
    AddressPage,
    FeePage,
    WarningPage,
    ConfirmationPage,
)>;

impl SignPromptPageList {
    pub fn new_with_seed(prompt: PromptSignBitcoinTx, rand_seed: u32) -> Self {
        let num_recipients = prompt.recipients.len();
        let has_warning = Self::has_high_fee(&prompt);

        let total_pages = num_recipients * 2 + has_warning as usize + 2;

        Self {
            prompt,
            total_pages,
            rand_seed,
        }
    }

    fn has_high_fee(prompt: &PromptSignBitcoinTx) -> bool {
        let fee_sats = prompt.fee.to_sat();

        if fee_sats > HIGH_FEE_ABSOLUTE_THRESHOLD_SATS {
            return true;
        }

        let at_risk_sats = prompt.value_at_risk().to_sat();
        at_risk_sats > 0 && fee_sats > at_risk_sats * HIGH_FEE_PERCENTAGE_THRESHOLD / 100
    }
}

impl WidgetList for SignPromptPageList {
    type Widget = SignPromptPage;

    fn len(&self) -> usize {
        self.total_pages
    }

    fn get(&self, index: usize) -> Option<WidgetListItem<SignPromptPage>> {
        if index >= self.total_pages {
            return None;
        }

        let num_recipients = self.prompt.recipients.len();
        let recipient_pages = num_recipients * 2;
        let has_warning = Self::has_high_fee(&self.prompt);

        let warning_page = if has_warning {
            Some(recipient_pages)
        } else {
            None
        };
        let fee_page = recipient_pages + has_warning as usize;

        let (page, use_fb) = if index < recipient_pages {
            let recipient_idx = index / 2;
            let recipient = &self.prompt.recipients[recipient_idx];
            let is_amount = index.is_multiple_of(2);

            if is_amount {
                (
                    SignPromptPage::new(AmountPage::new(
                        recipient_idx,
                        recipient.amount.to_sat(),
                        recipient.owned.as_ref(),
                    )),
                    false,
                )
            } else {
                (
                    SignPromptPage::new(AddressPage::new_with_seed(
                        recipient_idx,
                        &recipient.address,
                        self.rand_seed,
                        recipient.owned.as_ref(),
                    )),
                    true,
                )
            }
        } else if Some(index) == warning_page {
            (
                SignPromptPage::new(WarningPage::new(
                    self.prompt.fee.to_sat(),
                    self.prompt.foreign_value().is_some(),
                )),
                false,
            )
        } else if index == fee_page {
            (
                SignPromptPage::new(FeePage::new(
                    self.prompt.fee.to_sat(),
                    self.prompt.fee_rate_sats_per_vbyte,
                )),
                false,
            )
        } else {
            (SignPromptPage::new(ConfirmationPage::new()), false)
        };

        let mut item = WidgetListItem::new(page);
        if use_fb {
            item = item.with_framebuffer_transitions(true);
        }
        Some(item)
    }

    fn can_go_prev(&self, from_index: usize, current_widget: &SignPromptPage) -> bool {
        if from_index == 0 {
            return false;
        }
        if from_index == self.total_pages - 1 {
            if let Some(confirmation_page) = current_widget.downcast_ref::<ConfirmationPage>() {
                return !confirmation_page.is_confirmed();
            }
        }
        true
    }
}

/// High-level widget that manages the complete sign prompt flow using PageSlider
#[derive(frostsnap_macros::Widget)]
pub struct SignTxPrompt {
    #[widget_delegate]
    page_slider: Box<PageSlider<SignPromptPageList>>,
}

impl SignTxPrompt {
    pub fn new(prompt: PromptSignBitcoinTx) -> Self {
        Self::new_with_seed(prompt, 0)
    }

    pub fn new_with_seed(prompt: PromptSignBitcoinTx, rand_seed: u32) -> Self {
        let page_list = SignPromptPageList::new_with_seed(prompt, rand_seed);
        let mut page_slider = Box::new(PageSlider::new(page_list));
        page_slider.set_on_page_ready(|page| {
            if let Some(confirmation_page) = page.downcast_mut::<ConfirmationPage>() {
                confirmation_page.hold_confirm.fade_in_button();
            }
        });
        page_slider.enable_swipe_up_chevron();

        Self { page_slider }
    }

    pub fn is_confirmed(&mut self) -> bool {
        if self.page_slider.current_index() == self.page_slider.total_pages() - 1 {
            let current_widget = self.page_slider.current_widget();
            if let Some(confirmation_page) = current_widget.downcast_ref::<ConfirmationPage>() {
                return confirmation_page.is_confirmed();
            }
        }
        false
    }

    pub fn is_finished(&mut self) -> bool {
        if self.page_slider.current_index() == self.page_slider.total_pages() - 1 {
            let current_widget = self.page_slider.current_widget();
            if let Some(confirmation_page) = current_widget.downcast_ref::<ConfirmationPage>() {
                return confirmation_page.is_finished();
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::warning_lines;

    /// The sentence must name the quantity the rule divided by. In a mixed send every output is
    /// titled "Send Amount #n", owned ones included, so wording that reads as the total is wrong:
    /// the proportional rule uses only what leaves the wallet.
    #[test]
    fn the_warning_names_the_quantity_it_measured() {
        assert_eq!(
            warning_lines(60_000, true),
            ("Fee exceeds 5% of".into(), "the amount leaving".into())
        );
        assert_eq!(
            warning_lines(60_000, false),
            ("Fee exceeds 5% of".into(), "the value moved".into())
        );
        assert_eq!(
            warning_lines(150_000, true),
            ("Fee is greater".into(), "than 0.001 BTC".into())
        );
    }
}
