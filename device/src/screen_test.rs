use embedded_graphics::prelude::*;
use esp_hal::timer::Timer as _;
use frostsnap_cst816s::interrupt::TouchReceiver;
use frostsnap_widgets::{
    palette::PALETTE, Instant as WidgetInstant, ScreenTest, SuperDrawTarget, Widget,
};

use crate::touch_calibration::adjust_touch_point;

pub fn run<S, T>(display: &mut S, touch_receiver: &mut TouchReceiver, timer: &T)
where
    S: DrawTarget<Color = embedded_graphics::pixelcolor::Rgb565>,
    T: esp_hal::timer::Timer,
{
    let mut super_display = SuperDrawTarget::new(display, PALETTE.background);
    let mut screen_test_widget = ScreenTest::new();
    screen_test_widget.set_constraints(Size::new(240, 280));

    let mut last_redraw_time = timer.now();
    let _ = super_display.clear(PALETTE.background);

    loop {
        let now = timer.now();
        let now_ms = WidgetInstant::from_millis(now.duration_since_epoch().to_millis());

        while let Some(touch_event) = touch_receiver.dequeue() {
            let touch_point = adjust_touch_point(Point::new(touch_event.x, touch_event.y));
            let is_release = touch_event.action == 1;
            screen_test_widget.handle_touch(touch_point, now_ms, is_release);
        }

        let elapsed_ms = (now - last_redraw_time).to_millis();
        if elapsed_ms >= 5 {
            let _ = screen_test_widget.draw(&mut super_display, now_ms);
            last_redraw_time = now;
        }

        if screen_test_widget.is_completed() {
            break;
        }
    }
}
