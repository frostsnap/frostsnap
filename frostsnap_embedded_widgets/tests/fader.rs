use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    primitives::{Primitive, PrimitiveStyle, Rectangle},
    Drawable,
};
use frostsnap_embedded_widgets::{DynWidget, Fader, Frac, Instant, SuperDrawTarget, Widget};
use proptest::prelude::*;

/// A simple widget that draws a single pixel of a specific color
#[derive(Clone)]
struct SinglePixelWidget {
    color: Rgb565,
}

impl SinglePixelWidget {
    fn new(color: Rgb565) -> Self {
        Self { color }
    }
}

impl DynWidget for SinglePixelWidget {
    fn set_constraints(&mut self, _max_size: Size) {
        // Single pixel widget has fixed size
    }

    fn sizing(&self) -> frostsnap_embedded_widgets::Sizing {
        frostsnap_embedded_widgets::Sizing {
            width: 1,
            height: 1,
        }
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _is_release: bool,
    ) -> Option<frostsnap_embedded_widgets::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}

    fn force_full_redraw(&mut self) {}
}

impl Widget for SinglePixelWidget {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: Instant,
    ) -> Result<(), D::Error> {
        // Draw a single pixel at (0, 0)
        Rectangle::new(Point::new(0, 0), Size::new(1, 1))
            .into_styled(PrimitiveStyle::with_fill(self.color))
            .draw(target)
    }
}

/// A custom DrawTarget that captures the color of the pixel at (0, 0)
struct SinglePixelCapture {
    captured_color: Option<Rgb565>,
}

impl SinglePixelCapture {
    fn new() -> Self {
        Self {
            captured_color: None,
        }
    }

    fn get_captured_color(&self) -> Option<Rgb565> {
        self.captured_color
    }
}

impl DrawTarget for SinglePixelCapture {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            if pixel.0 == Point::new(0, 0) {
                self.captured_color = Some(pixel.1);
            }
        }
        Ok(())
    }
}

impl embedded_graphics::geometry::OriginDimensions for SinglePixelCapture {
    fn size(&self) -> Size {
        Size::new(240, 280)
    }
}

proptest! {
    #[test]
    fn test_fader_transitions(
        bg_color_raw: u16,
        color_a_raw: u16,
        f_val in 0u32..=10000u32,
        fade_duration_ms in 100u64..=5000u64,
        redraw_interval_ms in 10u64..=100u64,
    ) {
        // Convert raw values to colors
        let bg_color = Rgb565::from(embedded_graphics::pixelcolor::raw::RawU16::new(bg_color_raw));
        let color_a = Rgb565::from(embedded_graphics::pixelcolor::raw::RawU16::new(color_a_raw));

        // Convert f_val to Frac using from_ratio
        let f = Frac::from_ratio(f_val, 10000);

        // Create the widget to fade
        let widget = SinglePixelWidget::new(color_a);

        // Create the fader and start it fading in from bg_color
        let mut fader = Fader::new(widget);
        fader.start_fade_in(fade_duration_ms, redraw_interval_ms, bg_color);

        // Test at t=0 (should draw bg_color exclusively when fading in from it)
        let capture_t0 = SinglePixelCapture::new();
        let mut target_t0 = SuperDrawTarget::new(capture_t0, bg_color);
        let t0 = Instant::from_millis(0);
        fader.draw(&mut target_t0, t0).unwrap();
        let capture_t0 = target_t0.inner_mut().unwrap();

        // At t=0 when fading in from bg_color, we should see the background color
        let captured_t0 = capture_t0.get_captured_color().expect("Should have drawn a pixel at t=0");
        prop_assert_eq!(
            captured_t0,
            bg_color,
            "At t=0 when fading in, should draw from_color (background). Got {:?}, expected {:?}",
            captured_t0,
            bg_color
        );

        // Draw at t = f * fade_duration_ms
        // This is an intermediate point, no need to assert
        let capture_mid = SinglePixelCapture::new();
        let mut target_mid = SuperDrawTarget::new(capture_mid, bg_color);
        let t_mid_ms = (f * (fade_duration_ms as u32)).round() as u64;
        let t_mid = Instant::from_millis(t_mid_ms);
        fader.draw(&mut target_mid, t_mid).unwrap();

        // Draw at t = fade_duration_ms (should be fully color_a and idle)
        // But we need to respect redraw_interval_ms, so keep drawing until we get a pixel
        let mut captured_end = None;
        let mut t = fade_duration_ms;
        for _ in 0..10 {  // Try up to 10 times
            let capture_end = SinglePixelCapture::new();
            let mut target_end = SuperDrawTarget::new(capture_end, bg_color);
            fader.draw(&mut target_end, Instant::from_millis(t)).unwrap();
            let capture_end = target_end.inner_mut().unwrap();
            if let Some(color) = capture_end.get_captured_color() {
                captured_end = Some(color);
                break;
            }
            t += redraw_interval_ms;  // Advance by redraw interval
        }

        // At the end, we should see color_a (the widget's color)
        let captured_end = captured_end.expect("Should have drawn a pixel at or after fade end");
        prop_assert_eq!(
            captured_end,
            color_a,
            "At fade complete, should draw widget color. Got {:?}, expected {:?}",
            captured_end,
            color_a
        );

        // Verify fader is idle (draw again at a later time, should still be color_a)
        let capture_idle = SinglePixelCapture::new();
        let mut target_idle = SuperDrawTarget::new(capture_idle, bg_color);
        let t_idle = Instant::from_millis(fade_duration_ms + 1000);
        fader.draw(&mut target_idle, t_idle).unwrap();
        let capture_idle = target_idle.inner_mut().unwrap();

        let captured_idle = capture_idle.get_captured_color().expect("Should have drawn a pixel when idle");
        prop_assert_eq!(
            captured_idle,
            color_a,
            "When idle, should continue drawing widget color. Got {:?}, expected {:?}",
            captured_idle,
            color_a
        );

        // Verify is_fade_complete returns true
        prop_assert!(
            fader.is_fade_complete(),
            "After fade duration, is_fade_complete() should return true"
        );

        // Now test fading out back to bg_color
        fader.start_fade(fade_duration_ms, redraw_interval_ms, bg_color);

        // Draw at t=0 relative to fade out start (should still show color_a)
        let fade_out_start = fade_duration_ms + 1000;
        let capture_fade_out_t0 = SinglePixelCapture::new();
        let mut target_fade_out_t0 = SuperDrawTarget::new(capture_fade_out_t0, bg_color);
        fader.draw(&mut target_fade_out_t0, Instant::from_millis(fade_out_start)).unwrap();
        let capture_fade_out_t0 = target_fade_out_t0.inner_mut().unwrap();

        let captured_fade_out_t0 = capture_fade_out_t0.get_captured_color().expect("Should have drawn at fade out start");
        prop_assert_eq!(
            captured_fade_out_t0,
            color_a,
            "At fade out start, should still show widget color. Got {:?}, expected {:?}",
            captured_fade_out_t0,
            color_a
        );

        // Draw at fade out complete (should be fully bg_color and FadedOut)
        let mut captured_fade_out_end = None;
        let mut t_fade_out = fade_out_start + fade_duration_ms;
        for _ in 0..10 {  // Try up to 10 times to respect redraw interval
            let capture_fade_out_end = SinglePixelCapture::new();
            let mut target_fade_out_end = SuperDrawTarget::new(capture_fade_out_end, bg_color);
            fader.draw(&mut target_fade_out_end, Instant::from_millis(t_fade_out)).unwrap();
            let capture_fade_out_end = target_fade_out_end.inner_mut().unwrap();
            if let Some(color) = capture_fade_out_end.get_captured_color() {
                captured_fade_out_end = Some(color);
                break;
            }
            t_fade_out += redraw_interval_ms;
        }

        let captured_fade_out_end = captured_fade_out_end.expect("Should have drawn at fade out end");
        prop_assert_eq!(
            captured_fade_out_end,
            bg_color,
            "After fade out complete, should show target color. Got {:?}, expected {:?}",
            captured_fade_out_end,
            bg_color
        );

        // Verify fader is in FadedOut state
        prop_assert!(
            fader.is_fade_complete(),
            "After fade out, is_fade_complete() should return true"
        );
        prop_assert!(
            fader.is_faded_out(),
            "After fade out, is_faded_out() should return true"
        );

        // Try to draw again - should draw nothing (FadedOut state)
        let capture_faded_out = SinglePixelCapture::new();
        let mut target_faded_out = SuperDrawTarget::new(capture_faded_out, bg_color);
        fader.draw(&mut target_faded_out, Instant::from_millis(t_fade_out + 1000)).unwrap();
        let capture_faded_out = target_faded_out.inner_mut().unwrap();
        prop_assert_eq!(
            capture_faded_out.get_captured_color(),
            None,
            "In FadedOut state, should not draw anything"
        );
    }
}
