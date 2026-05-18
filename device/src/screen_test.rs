use alloc::rc::Rc;
use core::cell::RefCell;
use embedded_graphics::prelude::*;
use esp_hal::time::Instant;
use frostsnap_cst816s::interrupt::TouchReceiver;
use frostsnap_widgets::{
    palette::PALETTE, DynWidget, Instant as WidgetInstant, ScreenTest, SuperDrawTarget, Widget,
};

use crate::DISPLAY_REFRESH_MS;

pub fn run<'a>(
    display: crate::peripherals::Display<'a>,
    touch_receiver: &mut TouchReceiver,
) -> crate::peripherals::Display<'a> {
    let display_rc = Rc::new(RefCell::new(display));
    let mut super_display =
        SuperDrawTarget::from_shared(Rc::clone(&display_rc), PALETTE.background);

    let mut screen_test_widget = ScreenTest::new();
    screen_test_widget.set_constraints(Size::new(240, 280));

    let mut last_redraw_time = Instant::now();
    let _ = super_display.clear(PALETTE.background);
    crate::peripherals::flush_display(&mut display_rc.borrow_mut());

    loop {
        let now = Instant::now();
        let now_ms = WidgetInstant::from_millis(now.duration_since_epoch().as_millis());

        crate::peripherals::poll_touch_input();

        while let Some(touch_event) = touch_receiver.dequeue() {
            let touch_point =
                crate::peripherals::adjust_touch_point(Point::new(touch_event.x, touch_event.y));
            let is_release = touch_event.action == 1;
            screen_test_widget.handle_touch(touch_point, now_ms, is_release);
        }

        let elapsed_ms = (now - last_redraw_time).as_millis();
        if elapsed_ms >= DISPLAY_REFRESH_MS {
            let _ = screen_test_widget.draw(&mut super_display, now_ms);
            crate::peripherals::flush_display(&mut display_rc.borrow_mut());
            last_redraw_time = now;
        }

        if screen_test_widget.is_completed() {
            break;
        }
    }

    drop(super_display);
    Rc::try_unwrap(display_rc)
        .unwrap_or_else(|_| panic!("should be only holder"))
        .into_inner()
}
