use crate::{rat::FatRat, row::Row, sized_box::SizedBox, text::Text, Instant, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::BinaryColor,
};
use u8g2_fonts::U8g2TextStyle;
use alloc::{string::{ToString}};

/// A widget that displays a Bitcoin amount with proper formatting and coloring
/// Displays in format: X.XX XXX XXX with half-width spaces between segments
pub struct BitcoinAmountDisplay {
    /// The row containing all elements
    row: Row<(
        Text<U8g2TextStyle<BinaryColor>>,  // Whole part + decimal point
        Text<U8g2TextStyle<BinaryColor>>,  // First decimal digit
        Text<U8g2TextStyle<BinaryColor>>,  // Second decimal digit
        SizedBox<BinaryColor>,             // Half-width space
        Text<U8g2TextStyle<BinaryColor>>,  // Third decimal digit
        Text<U8g2TextStyle<BinaryColor>>,  // Fourth decimal digit
        Text<U8g2TextStyle<BinaryColor>>,  // Fifth decimal digit
        SizedBox<BinaryColor>,             // Half-width space
        Text<U8g2TextStyle<BinaryColor>>,  // Sixth decimal digit
        Text<U8g2TextStyle<BinaryColor>>,  // Seventh decimal digit
        Text<U8g2TextStyle<BinaryColor>>,  // Eighth decimal digit
    ), BinaryColor>,
    /// Amount in satoshis (for reference)
    satoshis: u64,
}

impl BitcoinAmountDisplay {
    pub fn new(satoshis: u64) -> Self {
        let btc = FatRat::from_ratio(satoshis, 100_000_000);
        let amount_str = format!("{}.", btc.whole_part());
        let mut color = BinaryColor::Off;
        if btc.whole_part() > 0 {
            color = BinaryColor::On;
        }
        let whole_text = Text::new(amount_str, U8g2TextStyle::new(crate::FONT_LARGE, color));

        // Get decimal digits iterator (take only 8 for Bitcoin)
        let mut after_decimal = btc.decimal_digits().take(8).map(|digit| {
            if digit > 0 {
                color = BinaryColor::On;
            }
            Text::new(digit.to_string(), U8g2TextStyle::new(crate::FONT_LARGE, color))
        });

        // Half-width spacers (approximately half the width of a digit)
        let spacer1 = SizedBox::<BinaryColor>::new(Size::new(4, 1));
        let spacer2 = SizedBox::<BinaryColor>::new(Size::new(4, 1));

        // Create row with all elements
        let row = Row::new((
            whole_text,
            after_decimal.next().unwrap(),
            after_decimal.next().unwrap(),
            spacer1,
            after_decimal.next().unwrap(),
            after_decimal.next().unwrap(),
            after_decimal.next().unwrap(),
            spacer2,
            after_decimal.next().unwrap(),
            after_decimal.next().unwrap(),
            after_decimal.next().unwrap(),
        ));

        Self {
            row,
            satoshis,
        }
    }

    pub fn satoshis(&self) -> u64 {
        self.satoshis
    }
}

impl crate::DynWidget for BitcoinAmountDisplay {
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.row.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.row.handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn size_hint(&self) -> Option<Size> {
        self.row.size_hint()
    }

    fn force_full_redraw(&mut self) {
        self.row.force_full_redraw()
    }
}

impl Widget for BitcoinAmountDisplay {
    type Color = BinaryColor;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.row.draw(target, current_time)
    }
}
