/// A fixed length string that doesn't require allocations.
///
/// Useful in panic handlers. If more text is written than it can fit it just silently overflows.
pub struct PanicBuffer<const N: usize> {
    buffer: [u8; N],
    buf_len: usize,
}

impl<const N: usize> Default for PanicBuffer<N> {
    fn default() -> Self {
        Self {
            buffer: [0u8; N],
            buf_len: 0,
        }
    }
}

impl<const N: usize> PanicBuffer<N> {
    pub fn as_str(&self) -> &str {
        match core::str::from_utf8(&self.buffer[..self.buf_len]) {
            Ok(string) => string,
            Err(_) => "failed to render panic",
        }
    }

    fn head(&mut self) -> &mut [u8] {
        &mut self.buffer[self.buf_len..]
    }
}

impl<const N: usize> core::fmt::Write for PanicBuffer<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let len = self.head().len().min(s.len());
        self.head()[..len].copy_from_slice(&s.as_bytes()[..len]);
        self.buf_len += len;
        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        let head = self.head();
        if !head.is_empty() {
            head[0] = c as u8;
            self.buf_len += 1;
        }
        Ok(())
    }
}

/// Display an error message on the screen with a red header
pub fn error_print<DT>(display: &mut DT, error: impl AsRef<str>)
where
    DT: embedded_graphics::draw_target::DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>
        + embedded_graphics::prelude::OriginDimensions,
{
    use embedded_graphics::{
        geometry::{Point, Size},
        pixelcolor::Rgb565,
        prelude::*,
        primitives::{PrimitiveStyleBuilder, Rectangle},
        text::{Alignment, Text},
    };
    use embedded_text::{alignment::HorizontalAlignment, style::TextBoxStyleBuilder, TextBox};
    use frostsnap_embedded_widgets::palette::PALETTE;

    let y = 25;
    let header_area = Rectangle::new(Point::zero(), Size::new(display.size().width, y));
    let _ = header_area
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(PALETTE.error)
                .build(),
        )
        .draw(display);

    let error_header_text = "Oh no, panic!";
    let _ = Text::with_alignment(
        error_header_text,
        Point::new((display.size().width / 2) as i32, 17),
        embedded_graphics::mono_font::MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_9X15,
            Rgb565::WHITE,
        ),
        Alignment::Center,
    )
    .draw(display);

    let textbox_style = TextBoxStyleBuilder::new()
        .alignment(HorizontalAlignment::Justified)
        .build();

    let body_area = Rectangle::new(
        Point::new(10, y as i32 + 5),
        Size::new(display.size().width - 20, display.size().height - y - 10),
    );

    let _ = TextBox::with_textbox_style(
        error.as_ref(),
        body_area,
        embedded_graphics::mono_font::MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_6X10,
            PALETTE.on_background,
        ),
        textbox_style,
    )
    .draw(display);
}
