use crate::HOLD_TO_CONFIRM_TIME_SHORT_MS;
use crate::{
    palette::PALETTE, prelude::*, scrollable_text::ScrollableText, string_ext::StringWrap,
    HoldToConfirm, Padding, FONT_MED, FONT_SMALL,
};
use alloc::string::String;
use embedded_graphics::{geometry::Size, pixelcolor::Rgb565, text::Alignment};
use u8g2_fonts::U8g2TextStyle;

/// Hold to confirm widget for signing messages
#[derive(frostsnap_macros::Widget)]
pub struct SignMessageConfirm {
    #[widget_delegate]
    hold_to_confirm: HoldToConfirm<
        Column<(
            Text<U8g2TextStyle<Rgb565>>,
            Container<Padding<ScrollableText<U8g2TextStyle<Rgb565>>>>,
        )>,
    >,
}

impl SignMessageConfirm {
    pub fn new(message: String) -> Self {
        let title = Text::new(
            "Sign message?",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);

        let wrapped_message = StringWrap::from_str(&message, 23);
        let scrollable_message = ScrollableText::new(
            wrapped_message.into(),
            U8g2TextStyle::new(FONT_SMALL, PALETTE.on_surface),
        )
        .with_alignment(Alignment::Center);

        let message_with_padding = Padding::all(8, scrollable_message);
        let message_container = Container::new(message_with_padding)
            .with_border(PALETTE.outline, 2)
            .with_fill(PALETTE.surface)
            .with_corner_radius(Size::new(8, 8))
            .with_expanded();

        let content = Column::new((title, message_container))
            .with_main_axis_alignment(MainAxisAlignment::SpaceEvenly);

        let hold_to_confirm = HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_SHORT_MS, content);

        Self { hold_to_confirm }
    }

    /// Check if the confirmation is complete
    pub fn is_confirmed(&self) -> bool {
        self.hold_to_confirm.is_completed()
    }
}
