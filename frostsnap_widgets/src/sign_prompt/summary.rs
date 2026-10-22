use super::{check_code_two_lines, FONT_CONFIRM_TEXT, FONT_PAGE_HEADER};
use crate::{
    bitcoin_amount_display::BitcoinAmountDisplay, gray4_style::Gray4TextStyle, palette::PALETTE,
    prelude::*,
};
use alloc::{
    format,
    string::{String, ToString},
};
use frostsnap_fonts::NOTO_SANS_MONO_17_REGULAR;

/// Summary page for transactions with many recipients
#[derive(frostsnap_macros::Widget)]
pub struct SummaryPage {
    #[widget_delegate]
    center: Center<
        Column<(
            Text<Gray4TextStyle>,
            BitcoinAmountDisplay,
            Text<Gray4TextStyle>,
            Text<Gray4TextStyle>,
            Text<Gray4TextStyle>,
            Text<Gray4TextStyle>,
        )>,
    >,
}

impl SummaryPage {
    #[inline(never)]
    pub fn new(
        num_recipients: usize,
        total_sent_sats: u64,
        fee_sats: u64,
        fee_rate: Option<f64>,
        fingerprint_code: String,
    ) -> Self {
        let recipients_text = Text::new(
            if num_recipients == 1 {
                "1 recipient".to_string()
            } else {
                format!("{} recipients", num_recipients)
            },
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let amount_display = BitcoinAmountDisplay::new(total_sent_sats);

        let btc_text = Text::new(
            "BTC".to_string(),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        let fee_str = if let Some(rate) = fee_rate {
            format!("Fee: {} sats ({:.1} sats/vB)", fee_sats, rate)
        } else {
            format!("Fee: {} sats", fee_sats)
        };
        let fee_text = Text::new(
            fee_str,
            Gray4TextStyle::new(FONT_CONFIRM_TEXT, PALETTE.text_secondary),
        );

        let check_code_label = Text::new(
            "Check code".to_string(),
            Gray4TextStyle::new(FONT_PAGE_HEADER, PALETTE.text_secondary),
        );

        // Two lines of four groups, since 19 characters at 17px fit the 240px screen.
        let code_text = Text::new(
            check_code_two_lines(&fingerprint_code),
            Gray4TextStyle::new(&NOTO_SANS_MONO_17_REGULAR, PALETTE.primary),
        )
        .with_alignment(embedded_graphics::text::Alignment::Center);

        let mut column = Column::new((
            recipients_text,
            amount_display,
            btc_text,
            fee_text,
            check_code_label,
            code_text,
        ))
        .with_main_axis_alignment(MainAxisAlignment::Center)
        .with_cross_axis_alignment(CrossAxisAlignment::Center);
        column.set_gap(0, 4);
        column.set_gap(1, 2);
        column.set_gap(2, 8);
        column.set_gap(3, 14);
        column.set_gap(4, 4);

        Self {
            center: Center::new(column),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::geometry::Size;

    #[test]
    fn summary_fits_the_device_screen() {
        let mut page = SummaryPage::new(
            4,
            1_000_000,
            500,
            Some(1.0),
            "1a50 e39a 473d 614f ffd1 dade fd42 85bb".to_string(),
        );
        page.set_constraints(Size::new(240, 280));
        let content = page.sizing().dirty_rect();
        let bottom_right = content.bottom_right().unwrap();
        assert!(bottom_right.x < 240 && bottom_right.y < 280, "{content:?}");
    }
}
