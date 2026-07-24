use crate::device_hal::{TouchEvent, TouchGesture};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::Point;
use frostsnap_widgets::{debug::OverlayDebug, DynWidget, Widget};

/// Apply one decoded touch sample to the widget tree.
///
/// Portable half of the device touch handler — the CST816S dequeue, calibration,
/// and gesture mapping live in the esp `TouchSource` (which yields `TouchEvent`).
pub fn apply_touch_event<W>(
    ev: TouchEvent,
    widget: &mut OverlayDebug<W>,
    last_touch: &mut Option<Point>,
    current_widget_index: &mut usize,
    now_ms: frostsnap_widgets::Instant,
) where
    W: Widget<Color = Rgb565>,
{
    let touch_point = ev.point;
    let lift_up = ev.lift_up;

    let is_vertical_drag = matches!(ev.gesture, TouchGesture::SlideUp | TouchGesture::SlideDown);
    let is_horizontal_swipe = matches!(
        ev.gesture,
        TouchGesture::SlideLeft | TouchGesture::SlideRight
    );

    // Horizontal swipes switch between the main widget and the debug log.
    if is_horizontal_swipe && lift_up {
        match ev.gesture {
            TouchGesture::SlideLeft if *current_widget_index == 0 => {
                *current_widget_index = 1;
                widget.show_logs();
            }
            TouchGesture::SlideRight if *current_widget_index == 1 => {
                *current_widget_index = 0;
                widget.show_main();
            }
            _ => {}
        }
    }

    if is_vertical_drag {
        widget.handle_vertical_drag(
            last_touch.map(|point| point.y as u32),
            touch_point.y as u32,
            lift_up,
        );
    }

    // Always handle press/release (lift_up must be processed even after a drag).
    if !is_vertical_drag || lift_up {
        widget.handle_touch(touch_point, now_ms, lift_up);
    }

    if lift_up {
        *last_touch = None;
    } else {
        *last_touch = Some(touch_point);
    }
}
