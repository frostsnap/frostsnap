use embedded_graphics::prelude::Point;
use embedded_graphics::pixelcolor::Rgb565;
use frostsnap_cst816s::{TouchEvent, TouchGesture, interrupt::TouchReceiver};
use frostsnap_widgets::{debug::OverlayDebug, DynWidget, Widget};

use crate::touch_calibration::adjust_touch_point;

/// Process touch events from the consumer and handle gestures
/// Returns true if the widget was switched (horizontal swipe)
pub fn process_touch_event<W>(
    touch_event: TouchEvent,
    widget: &mut OverlayDebug<W>,
    last_touch: &mut Option<Point>,
    current_widget_index: &mut usize,
    now_ms: frostsnap_widgets::Instant,
) -> bool 
where
    W: Widget<Color = Rgb565>,
{
    // Apply touch calibration adjustments
    let touch_point = adjust_touch_point(Point::new(touch_event.x, touch_event.y));
    let lift_up = touch_event.action == 1;
    let gesture = touch_event.gesture;

    let is_vertical_drag = matches!(gesture, TouchGesture::SlideUp | TouchGesture::SlideDown);
    let is_horizontal_swipe = matches!(gesture, TouchGesture::SlideLeft | TouchGesture::SlideRight);

    let mut widget_switched = false;

    // Handle horizontal swipes to switch between widgets
    if is_horizontal_swipe && lift_up {
        match gesture {
            TouchGesture::SlideLeft => {
                // Swipe left: show debug log
                if *current_widget_index == 0 {
                    *current_widget_index = 1;
                    widget.show_logs();
                    frostsnap_widgets::debug::log("Switched to debug log".into());
                    widget_switched = true;
                }
            }
            TouchGesture::SlideRight => {
                // Swipe right: show main widget
                if *current_widget_index == 1 {
                    *current_widget_index = 0;
                    widget.show_main();
                    frostsnap_widgets::debug::log("Switched to main widget".into());
                    widget_switched = true;
                }
            }
            _ => {}
        }
    }

    // Handle vertical drag for widgets that support it
    if is_vertical_drag {
        widget.handle_vertical_drag(
            last_touch.map(|point| point.y as u32),
            touch_point.y as u32,
            lift_up,
        );
    }

    if !is_vertical_drag || lift_up {
        // Always handle touch events (for both press and release)
        // This is important so that lift_up is processed after drag
        widget.handle_touch(touch_point, now_ms, lift_up);
    }

    // Store last touch for drag calculations
    if lift_up {
        *last_touch = None;
    } else {
        *last_touch = Some(touch_point);
    }

    widget_switched
}

/// Check if touch event has valid coordinates
#[inline]
pub fn is_valid_touch(touch_event: &TouchEvent) -> bool {
    touch_event.x > 0 || touch_event.y > 0
}

/// Process all pending touch events from the receiver
pub fn process_all_touch_events<W>(
    touch_receiver: &mut TouchReceiver,
    widget: &mut OverlayDebug<W>,
    last_touch: &mut Option<Point>,
    current_widget_index: &mut usize,
    now_ms: frostsnap_widgets::Instant,
) where
    W: Widget<Color = Rgb565>,
{
    while let Some(touch_event) = touch_receiver.dequeue() {
        // Only process if we have valid coordinates
        if is_valid_touch(&touch_event) {
            process_touch_event(
                touch_event,
                widget,
                last_touch,
                current_widget_index,
                now_ms,
            );
        }
    }
}