use crate::{
    palette::PALETTE, prelude::*, string_ext::StringWrap, DefaultTextStyle, HoldToConfirm, Padding,
    FONT_MED, HOLD_TO_CONFIRM_TIME_SHORT_MS, LEGACY_FONT_SMALL,
};
use alloc::string::String;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::{geometry::Size, text::Alignment};
use u8g2_fonts::U8g2TextStyle;

/// Hold-to-confirm widget for BIP-322 message signing.
///
/// Shows the human-readable message being signed and the address it is being
/// signed under, so the user can verify both on the device itself.
#[derive(frostsnap_macros::Widget)]
pub struct Bip322Confirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<
        Column<(
            Text,
            Container<Padding<Text<U8g2TextStyle<Rgb565>>>>,
            Text<U8g2TextStyle<Rgb565>>,
        )>,
    >,
}

impl Bip322Confirm {
    pub fn new(message: String, address: &bitcoin::Address, index: u32) -> Self {
        let title = Text::new(
            "Sign message?",
            DefaultTextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let wrapped_message = StringWrap::from_str(&message, 23);
        let message_text = Text::new(
            wrapped_message.as_str(),
            U8g2TextStyle::new(LEGACY_FONT_SMALL, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Center);

        let message_with_padding = Padding::all(8, message_text);
        let message_container = Container::new(message_with_padding)
            .with_border(PALETTE.outline, 2)
            .with_fill(PALETTE.surface)
            .with_corner_radius(Size::new(8, 8))
            .with_expanded();

        let caption = alloc::format!("with address #{index}\n{address}");
        let wrapped_caption = StringWrap::from_str(&caption, 23);
        let caption_text = Text::new(
            wrapped_caption.as_str(),
            U8g2TextStyle::new(LEGACY_FONT_SMALL, PALETTE.text_secondary),
        )
        .with_alignment(Alignment::Center);

        let content = Column::new((title, message_container, caption_text))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        let hold_to_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_SHORT_MS, content);

        Self { hold_to_confirm }
    }

    pub fn is_confirmed(&self) -> bool {
        self.hold_to_confirm.is_confirmed()
    }

    pub fn is_finished(&self) -> bool {
        self.hold_to_confirm.is_finished()
    }
}
