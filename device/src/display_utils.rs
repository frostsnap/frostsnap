use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::OriginDimensions,
    mono_font::{ascii, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::*,
};
use embedded_text::{
    alignment::HorizontalAlignment,
    style::{TextBoxStyle, TextBoxStyleBuilder},
    TextBox,
};
use mipidsi::error::Error;

// Re-export the palette
pub use frostsnap_embedded_widgets::palette::PALETTE;

const TEXTBOX_STYLE: TextBoxStyle = TextBoxStyleBuilder::new().build();

/// Display an error message on the screen with a red header bar
pub fn error_print<DT>(display: &mut DT, error: impl AsRef<str>)
where
    DT: DrawTarget<Color = Rgb565, Error = Error> + OriginDimensions,
{
    let y = 25;
    let header_area = Rectangle::new(Point::zero(), Size::new(display.size().width, y));
    let _ = header_area
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(PALETTE.error)
                .build(),
        )
        .draw(display);

    let header_charstyle = MonoTextStyle::new(&ascii::FONT_7X14, PALETTE.primary);
    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Center)
        .build();
    let _ = TextBox::with_textbox_style(
        "ERROR",
        Rectangle::new(Point::new(1, 9), Size::new(display.size().width, y)),
        header_charstyle,
        textbox_style,
    )
    .draw(display);

    Line::new(
        Point::new(0, y as i32),
        Point::new(display.size().width as i32, y as i32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb565::CSS_DARK_GRAY, 1))
    .draw(display)
    .unwrap();

    let _ = Rectangle::new(
        Point::new(0, (y + 1) as i32),
        Size::new(display.size().width, display.size().height),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(PALETTE.background)
            .build(),
    )
    .draw(display);

    let character_style = MonoTextStyle::new(&ascii::FONT_7X14, PALETTE.primary);

    let _ = TextBox::with_textbox_style(
        error.as_ref(),
        Rectangle::new(Point::new(1, (y + 1) as i32), display.size()),
        character_style,
        TEXTBOX_STYLE,
    )
    .draw(display);
}
