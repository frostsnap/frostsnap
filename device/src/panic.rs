use crate::init_display;
use embedded_graphics::{prelude::*, text::Alignment};
use frostsnap_widgets::string_ext::StringFixed;

const FONT_WIDTH: u32 = 6;

/// Unified panic handler for Frostsnap devices
///
/// This function handles panics by displaying the panic information on the screen
/// with a green background and white text.
pub fn handle_panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    use embedded_graphics::pixelcolor::Rgb565;
    use esp_hal::{
        delay::Delay,
        gpio::{Level, Output},
        peripherals::Peripherals,
    };

    let peripherals = unsafe { Peripherals::steal() };

    let mut bl = Output::new(peripherals.GPIO1, Level::Low);

    let mut delay = Delay::new();

    let mut display = init_display!(peripherals: peripherals, delay: &mut delay);

    let _ = display.clear(Rgb565::CSS_DARK_BLUE);

    // Draw red ERROR header
    use embedded_graphics::mono_font::{ascii::FONT_10X20, MonoTextStyle};
    use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
    use embedded_graphics::text::{Text, TextStyle};

    let error_rect = Rectangle::new(Point::new(0, 0), Size::new(240, 40));
    let _ = error_rect
        .into_styled(PrimitiveStyleBuilder::new().fill_color(Rgb565::RED).build())
        .draw(&mut display);

    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
    let _ = Text::with_text_style(
        "ERROR",
        Point::new(95, 25), // Centered horizontally
        text_style,
        TextStyle::default(),
    )
    .draw(&mut display);

    let mut panic_buf = StringFixed::<512>::with_wrap((240 / FONT_WIDTH) as usize);

    let _ = match info.location() {
        Some(location) => write!(
            &mut panic_buf,
            "{}:{} {}",
            location.file().split('/').next_back().unwrap_or(""),
            location.line(),
            info
        ),
        None => write!(&mut panic_buf, "{}", info),
    };

    // Draw panic text
    let _ = embedded_graphics::text::Text::with_alignment(
        panic_buf.as_str(),
        embedded_graphics::geometry::Point::new(0, 50), // Move panic text below header
        embedded_graphics::mono_font::MonoTextStyle::new(
            &embedded_graphics::mono_font::ascii::FONT_6X10,
            Rgb565::CSS_WHITE,
        ),
        Alignment::Left,
    )
    .draw(&mut display);

    // Draw contact info at bottom
    let contact_style = MonoTextStyle::new(&FONT_10X20, Rgb565::CSS_WHITE);
    let _ = Text::with_text_style(
        "Contact\nsupport@frostsnap.com",
        Point::new(120, 240), // Centered horizontally
        contact_style,
        TextStyle::with_alignment(Alignment::Center),
    )
    .draw(&mut display);

    bl.set_high();
    #[allow(clippy::empty_loop)]
    loop {}
}
