#![cfg(feature = "redact")]

use embedded_graphics::{geometry::Size, pixelcolor::Rgb565};
use frostsnap_fonts::NOTO_SANS_18_LIGHT;
use frostsnap_widgets::{
    gray4_style::Gray4TextStyle, palette::PALETTE, redact, vec_framebuffer::VecFramebuffer, Column,
    DynWidget, Instant, SuperDrawTarget, Text, Widget,
};

fn text(s: &str) -> Text<Gray4TextStyle> {
    Text::new(
        s.to_string(),
        Gray4TextStyle::new(&NOTO_SANS_18_LIGHT, PALETTE.on_background),
    )
}

fn render<W: Widget<Color = Rgb565>>(mut widget: W) -> VecFramebuffer<Rgb565> {
    widget.set_constraints(Size::new(240, 280));
    let mut fb = VecFramebuffer::new(240, 280);
    fb.clear(PALETTE.background);
    let mut target = SuperDrawTarget::new(&mut fb, PALETTE.background);
    widget.draw(&mut target, Instant::from_millis(0)).unwrap();
    drop(target);
    fb
}

#[test]
fn only_the_redacted_child_is_blurred() {
    let sharp = render(Column::new((text("Network Fee"), text("0.00 000 284"))));
    let blurred = render(Column::new((
        text("Network Fee"),
        redact::redact(text("0.00 000 284")),
    )));

    let mut title = text("Network Fee");
    title.set_constraints(Size::new(240, 280));
    let title_rows = title.sizing().height as usize;
    let rows = |fb: &VecFramebuffer<Rgb565>, range: core::ops::Range<usize>| {
        fb.data()[range.start * 240 * 2..range.end * 240 * 2].to_vec()
    };
    assert_eq!(rows(&sharp, 0..title_rows), rows(&blurred, 0..title_rows));
    assert_ne!(
        rows(&sharp, title_rows..280),
        rows(&blurred, title_rows..280)
    );
}
