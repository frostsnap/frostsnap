use crate::{
    address_display::{pick_highlights, AddressDisplay},
    any_of::AnyOf,
    bitcoin_amount_display::BitcoinAmountDisplay,
    gray4_style::Gray4TextStyle,
    page_slider::PageSlider,
    palette::PALETTE,
    prelude::*,
    widget_list::{WidgetList, WidgetListItem},
    GrayToAlpha, HoldToConfirm, Image,
};
use alloc::{boxed::Box, format, string::String, string::ToString, vec::Vec};
use embedded_graphics::{
    geometry::Size,
    pixelcolor::{Gray8, Rgb565},
};
use frostsnap_core::bitcoin_transaction::PromptSignBitcoinTx;
use frostsnap_fonts::{
    Gray4Font, NOTO_SANS_17_REGULAR, NOTO_SANS_18_LIGHT, NOTO_SANS_18_MEDIUM, NOTO_SANS_24_BOLD,
    NOTO_SANS_MONO_15_REGULAR,
};
use tinybmp::Bmp;

mod recipient_list;
mod summary;

use recipient_list::{RecipientItem, RecipientListPage};
use summary::SummaryPage;

const FONT_PAGE_HEADER: &Gray4Font = &NOTO_SANS_18_LIGHT;
const FONT_CONFIRM_TITLE: &Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_CONFIRM_TEXT: &Gray4Font = &NOTO_SANS_17_REGULAR;
const FONT_CAUTION_NOTE: &Gray4Font = &NOTO_SANS_18_MEDIUM;
const FONT_CAUTION_TITLE: &Gray4Font = &NOTO_SANS_24_BOLD;
const FONT_CAUTION_TEXT: &Gray4Font = &NOTO_SANS_17_REGULAR;
const FONT_SILENT_PAYMENT_ADDRESS: &Gray4Font = &NOTO_SANS_MONO_15_REGULAR;
const SP_ADDRESS_HORIZONTAL_SPACING: u32 = 15;

/// Default number of recipients above which the review switches to a list.
pub const DEFAULT_LIST_REVIEW_THRESHOLD: usize = 3;

const HIGH_FEE_ABSOLUTE_THRESHOLD_SATS: u64 = 100_000;
const HIGH_FEE_PERCENTAGE_THRESHOLD: u64 = 5;
const HOLD_TO_SIGN_TIME_MS: u32 = 3000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SignPromptKind {
    Sign,
    SpEcdh,
}

/// Widget list that generates sign prompt pages
#[derive(Clone)]
pub struct SignPromptPageList {
    prompt: PromptSignBitcoinTx,
    list_threshold: usize,
    total_pages: usize,
    rand_seed: u32,
    kind: SignPromptKind,
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
        )>,
    >,
}

impl AmountPage {
    #[inline(never)]
    pub fn new(index: usize, amount_sats: u64) -> Self {
        let title = Text::new(
            format!("Send Amount #{}", index + 1),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let amount_display = BitcoinAmountDisplay::new(amount_sats);

        let btc_text = Text::new(
            "BTC".to_string(),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let mut column = Column::new((title, amount_display, btc_text))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_uniform_gap(10);

        Self {
            center: Center::new(column),
        }
    }
}

/// Page widget for displaying recipient address
#[derive(Clone, frostsnap_macros::Widget)]
pub struct AddressPage {
    #[widget_delegate]
    center: Center<Padding<Column<(Text<Gray4TextStyle>, AddressDisplay)>>>,
}

impl AddressPage {
    #[inline(never)]
    pub fn new_with_seed(index: usize, address: &bitcoin::Address, rand_seed: u32) -> Self {
        let title = Text::new(
            format!("To Address #{}", index + 1),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let mixed_seed = rand_seed.wrapping_add((index as u32).wrapping_mul(0x9e3779b9));
        let address_display = AddressDisplay::new_with_seed(address.clone(), mixed_seed);

        let mut column = Column::new((title, address_display))
            .with_main_axis_alignment(MainAxisAlignment::Start);
        column.set_gap(0, 10);
        let padded = Padding::only(column).bottom(40).build();

        Self {
            center: Center::new(padded),
        }
    }
}

fn chunk_silent_payment_address(address: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut pos = 0;
    while pos < address.len() {
        let end = (pos + 4).min(address.len());
        let chunk = &address[pos..end];
        chunks.push(alloc::format!("{:4}", chunk));
        pos = end;
    }
    chunks
}

pub type SilentPaymentAddressDisplay = Column<Vec<Row<Vec<Text<Gray4TextStyle>>>>>;

/// Page widget for displaying silent payment recipient's address and amount
#[derive(Clone, frostsnap_macros::Widget)]
pub struct SilentPaymentPage {
    #[widget_delegate]
    center: Center<
        Padding<
            Column<(
                Text<Gray4TextStyle>,
                Text<Gray4TextStyle>,
                SilentPaymentAddressDisplay,
            )>,
        >,
    >,
}

impl SilentPaymentPage {
    #[inline(never)]
    pub fn new(index: usize, address: &str, amount: bitcoin::Amount) -> Self {
        Self::new_with_seed(index, address, amount, 0)
    }

    #[inline(never)]
    pub fn new_with_seed(
        index: usize,
        address: &str,
        amount: bitcoin::Amount,
        rand_seed: u32,
    ) -> Self {
        let title = Text::new(
            format!("Send to silent payment #{}", index + 1),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let amount_text = Text::new(
            format!("{}", amount),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.primary),
        );

        let chunks = chunk_silent_payment_address(address);
        let mixed_seed = rand_seed.wrapping_add((index as u32).wrapping_mul(0x9e3779b9));
        let highlighted = pick_highlights(mixed_seed, chunks.len());

        let mut rows = Vec::new();
        for (row_idx, chunk_slice) in chunks.chunks(3).enumerate() {
            let mut row_texts = Vec::new();
            for (col_idx, chunk_str) in chunk_slice.iter().enumerate() {
                let global_idx = row_idx * 3 + col_idx;
                let color = if highlighted.contains(&global_idx) {
                    PALETTE.on_background
                } else {
                    PALETTE.primary
                };
                row_texts.push(Text::new(
                    chunk_str.clone(),
                    Gray4TextStyle::new(FONT_SILENT_PAYMENT_ADDRESS, color),
                ));
            }
            let mut row = Row::new(row_texts).with_main_axis_alignment(MainAxisAlignment::Center);
            row.set_uniform_gap(SP_ADDRESS_HORIZONTAL_SPACING);
            rows.push(row);
        }

        let mut address_col = Column::new(rows);
        address_col.set_uniform_gap(1);

        let mut column = Column::new((title, amount_text, address_col))
            .with_main_axis_alignment(MainAxisAlignment::Start)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_gap(0, 4);
        column.set_gap(1, 6);
        let padded = Padding::only(column).bottom(30).build();

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

const WARNING_ICON_DATA: &[u8] = include_bytes!("../../assets/warning-icon-24x24.bmp");

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

impl WarningPage {
    #[inline(never)]
    fn new(fee_sats: u64, _total_sent: u64) -> Self {
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

        let (line1, line2) = if fee_sats > HIGH_FEE_ABSOLUTE_THRESHOLD_SATS {
            ("Fee is greater".to_string(), "than 0.001 BTC".to_string())
        } else {
            (
                "Fee exceeds 5% of the".to_string(),
                "amount being sent".to_string(),
            )
        };

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

/// Splits the eight groups of a check code onto two lines of four groups each.
fn check_code_two_lines(code: &str) -> String {
    match code.match_indices(' ').nth(3) {
        Some((i, _)) => format!("{}\n{}", &code[..i], &code[i + 1..]),
        None => code.to_string(),
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
            Text<Gray4TextStyle>,
        )>,
    >,
}

impl ConfirmationPage {
    #[inline(never)]
    fn new(kind: SignPromptKind, fingerprint_code: &str) -> Self {
        let (sign_title, press_line1, press_line2) = match kind {
            SignPromptKind::Sign => (
                "Hold to Sign".to_string(),
                "Press and hold".to_string(),
                "for 3 seconds".to_string(),
            ),
            SignPromptKind::SpEcdh => (
                "Hold to Contribute".to_string(),
                "Adds your key share,".to_string(),
                "not signing yet".to_string(),
            ),
        };

        let sign_text = Text::new(
            sign_title,
            Gray4TextStyle::new(FONT_CONFIRM_TITLE, PALETTE.on_background),
        );

        let press_text = Column::new((
            Text::new(
                press_line1,
                Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
            ),
            Text::new(
                press_line2,
                Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
            ),
        ));

        let code_text = Text::new(
            format!("Check code\n{}", check_code_two_lines(fingerprint_code)),
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        )
        .with_alignment(embedded_graphics::text::Alignment::Center);

        let mut confirm_content = Column::new((sign_text, press_text, code_text))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(CrossAxisAlignment::Center);
        confirm_content.set_gap(0, 10);
        confirm_content.set_gap(1, 10);

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
    SilentPaymentPage,
    FeePage,
    WarningPage,
    ConfirmationPage,
    SummaryPage,
    RecipientListPage,
)>;

impl SignPromptPageList {
    fn new_with_seed_and_kind(
        prompt: PromptSignBitcoinTx,
        rand_seed: u32,
        kind: SignPromptKind,
        list_threshold: usize,
    ) -> Self {
        let num_recipients =
            prompt.foreign_recipients.len() + prompt.silent_payment_recipients.len();
        let has_warning = Self::has_high_fee(&prompt);

        // Signing reviews every output, so the ECDH request only asks to confirm the check code.
        let total_pages = if kind == SignPromptKind::SpEcdh {
            1
        } else if num_recipients > list_threshold {
            2 + if has_warning { 1 } else { 0 } + 1
        } else {
            num_recipients * 2 + if has_warning { 1 } else { 0 } + 2
        };
        Self {
            prompt,
            list_threshold,
            total_pages,
            rand_seed,
            kind,
        }
    }

    fn has_high_fee(prompt: &PromptSignBitcoinTx) -> bool {
        let fee_sats = prompt.fee.to_sat();

        if fee_sats > HIGH_FEE_ABSOLUTE_THRESHOLD_SATS {
            return true;
        }

        let total_sent = prompt.total_sent().to_sat();
        if total_sent > 0 && fee_sats > total_sent * HIGH_FEE_PERCENTAGE_THRESHOLD / 100 {
            return true;
        }

        false
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

        if self.kind == SignPromptKind::SpEcdh {
            return Some(WidgetListItem::new(SignPromptPage::new(
                ConfirmationPage::new(self.kind, &self.prompt.fingerprint_code()),
            )));
        }

        let num_recipients =
            self.prompt.foreign_recipients.len() + self.prompt.silent_payment_recipients.len();
        let recipient_pages = num_recipients * 2;
        let has_warning = Self::has_high_fee(&self.prompt);

        if num_recipients > self.list_threshold {
            let page = if index == 0 {
                let summary = SummaryPage::new(
                    num_recipients,
                    self.prompt.total_sent().to_sat(),
                    self.prompt.fee.to_sat(),
                    self.prompt.fee_rate_sats_per_vbyte,
                    self.prompt.fingerprint_code(),
                );
                SignPromptPage::new(summary)
            } else if index == 1 {
                let mut recipients = Vec::with_capacity(num_recipients);
                for (i, (addr, amt)) in self.prompt.foreign_recipients.iter().enumerate() {
                    recipients.push(RecipientItem::Foreign {
                        index: i,
                        address: addr.clone(),
                        amount: *amt,
                    });
                }
                let foreign_len = self.prompt.foreign_recipients.len();
                for (i, (addr, amt)) in self.prompt.silent_payment_recipients.iter().enumerate() {
                    recipients.push(RecipientItem::SilentPayment {
                        index: foreign_len + i,
                        address: addr.clone(),
                        amount: *amt,
                    });
                }
                let list_page = RecipientListPage::new(recipients, self.rand_seed);
                SignPromptPage::new(list_page)
            } else if has_warning && index == 2 {
                let total_sent = self.prompt.total_sent().to_sat();
                SignPromptPage::new(WarningPage::new(self.prompt.fee.to_sat(), total_sent))
            } else {
                SignPromptPage::new(ConfirmationPage::new(
                    self.kind,
                    &self.prompt.fingerprint_code(),
                ))
            };
            return Some(WidgetListItem::new(page));
        }

        let (page, use_fb) = if index < recipient_pages {
            let recipient_idx = index / 2;
            let is_amount = index.is_multiple_of(2);

            if is_amount {
                let amount = self
                    .prompt
                    .foreign_recipients
                    .as_slice()
                    .get(recipient_idx)
                    .map(|(_, amount)| amount)
                    .unwrap_or_else(|| {
                        &self.prompt.silent_payment_recipients
                            [recipient_idx - self.prompt.foreign_recipients.len()]
                        .1
                    });
                (
                    SignPromptPage::new(AmountPage::new(recipient_idx, amount.to_sat())),
                    false,
                )
            } else {
                if let Some((address, _)) =
                    self.prompt.foreign_recipients.as_slice().get(recipient_idx)
                {
                    (
                        SignPromptPage::new(AddressPage::new_with_seed(
                            recipient_idx,
                            address,
                            self.rand_seed,
                        )),
                        true,
                    )
                } else {
                    let (address, amount) = &self.prompt.silent_payment_recipients
                        [recipient_idx - self.prompt.foreign_recipients.len()];
                    (
                        SignPromptPage::new(SilentPaymentPage::new_with_seed(
                            recipient_idx,
                            address,
                            *amount,
                            self.rand_seed,
                        )),
                        true,
                    )
                }
            }
        } else if has_warning && index == recipient_pages {
            let total_sent = self.prompt.total_sent().to_sat();
            (
                SignPromptPage::new(WarningPage::new(self.prompt.fee.to_sat(), total_sent)),
                false,
            )
        } else if (has_warning && index == recipient_pages + 1)
            || (!has_warning && index == recipient_pages)
        {
            (
                SignPromptPage::new(FeePage::new(
                    self.prompt.fee.to_sat(),
                    self.prompt.fee_rate_sats_per_vbyte,
                )),
                false,
            )
        } else {
            (
                SignPromptPage::new(ConfirmationPage::new(
                    self.kind,
                    &self.prompt.fingerprint_code(),
                )),
                false,
            )
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
        if let Some(list_page) = current_widget.downcast_ref::<RecipientListPage>() {
            return list_page.can_go_prev();
        }
        true
    }

    fn can_go_next(&self, from_index: usize, current_widget: &SignPromptPage) -> bool {
        if from_index + 1 >= self.total_pages {
            return false;
        }
        if let Some(list_page) = current_widget.downcast_ref::<RecipientListPage>() {
            return list_page.can_go_next();
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
        Self::new_with_seed_and_kind(
            prompt,
            rand_seed,
            SignPromptKind::Sign,
            DEFAULT_LIST_REVIEW_THRESHOLD,
        )
    }

    pub fn new_sp_ecdh(prompt: PromptSignBitcoinTx, rand_seed: u32) -> Self {
        Self::new_with_seed_and_kind(
            prompt,
            rand_seed,
            SignPromptKind::SpEcdh,
            DEFAULT_LIST_REVIEW_THRESHOLD,
        )
    }

    /// `list_threshold` is the number of recipients above which the review is a summary page and
    /// a list of recipients instead of one page per recipient. 0 always uses the list, 1 uses it
    /// when there is more than one recipient.
    pub fn new_with_seed_and_kind(
        prompt: PromptSignBitcoinTx,
        rand_seed: u32,
        kind: SignPromptKind,
        list_threshold: usize,
    ) -> Self {
        let page_list =
            SignPromptPageList::new_with_seed_and_kind(prompt, rand_seed, kind, list_threshold);
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
    use super::*;
    use alloc::vec;
    use embedded_graphics::geometry::Size;

    fn prompt(num_sp_recipients: usize) -> PromptSignBitcoinTx {
        PromptSignBitcoinTx {
            foreign_recipients: vec![],
            silent_payment_recipients: (0..num_sp_recipients)
                .map(|i| {
                    (
                        format!("tsp1recipient{i}"),
                        bitcoin::Amount::from_sat(1_000),
                    )
                })
                .collect(),
            fee: bitcoin::Amount::from_sat(10),
            fee_rate_sats_per_vbyte: Some(1.0),
            outputs_fingerprint: [0; 32],
        }
    }

    fn pages(num_recipients: usize, list_threshold: usize) -> usize {
        SignPromptPageList::new_with_seed_and_kind(
            prompt(num_recipients),
            0,
            SignPromptKind::Sign,
            list_threshold,
        )
        .len()
    }

    #[test]
    fn sp_ecdh_only_confirms() {
        let list = SignPromptPageList::new_with_seed_and_kind(
            prompt(2),
            0,
            SignPromptKind::SpEcdh,
            DEFAULT_LIST_REVIEW_THRESHOLD,
        );
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn list_review_starts_above_the_threshold() {
        // Summary, list, confirm.
        const LIST: usize = 3;
        // Two pages per recipient, then fee and confirm.
        let per_recipient = |n: usize| n * 2 + 2;

        // Threshold 0 always uses the list, even for one recipient.
        assert_eq!(pages(1, 0), LIST);
        // Threshold 1 uses the list from two recipients.
        assert_eq!(pages(1, 1), per_recipient(1));
        assert_eq!(pages(2, 1), LIST);
        // The default keeps one page per recipient up to three.
        assert_eq!(pages(3, DEFAULT_LIST_REVIEW_THRESHOLD), per_recipient(3));
        assert_eq!(pages(4, DEFAULT_LIST_REVIEW_THRESHOLD), LIST);
    }

    #[test]
    fn confirmation_with_check_code_fits_the_device_screen() {
        let mut page = ConfirmationPage::new(
            SignPromptKind::Sign,
            "1a50 e39a 473d 614f ffd1 dade fd42 85bb",
        );
        page.set_constraints(Size::new(240, 280));
        let content = page.sizing().dirty_rect();
        let bottom_right = content.bottom_right().unwrap();
        assert!(bottom_right.x < 240 && bottom_right.y < 280, "{content:?}");
    }
}
