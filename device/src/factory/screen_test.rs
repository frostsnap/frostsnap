use crate::calibrate_point;
use cst816s::CST816S;
use embedded_graphics::{
    mono_font::{iso_8859_1::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Baseline, Text},
};
use embedded_hal as hal;

// Constant for the "lift up" action.
const ACTION_LIFT_UP: u8 = 1;

// Updated target cells with a color for each cell.
const TARGET_CELLS: [(i32, i32, Rgb565); 5] = [
    (1, 1, Rgb565::RED),
    (7, 1, Rgb565::GREEN), // Top row
    (4, 4, Rgb565::BLUE),  // Middle row
    (1, 7, Rgb565::YELLOW),
    (7, 7, Rgb565::CSS_PURPLE), // Bottom row
];

/// Returns the target rectangle (in pixel coordinates) for a given cell.
/// Each target square is drawn with a 1‑pixel margin within its grid cell.
fn target_rect(cell: (i32, i32), grid_spacing: i32) -> Rectangle {
    let (cell_x, cell_y) = cell;
    Rectangle::new(
        Point::new(cell_x * grid_spacing + 1, cell_y * grid_spacing + 1),
        Size::new((grid_spacing - 2) as u32, (grid_spacing - 2) as u32),
    )
}

/// Updates the failures display by drawing a filled white rectangle over the
/// area where the failures text is shown, then drawing the text in red on top.
fn update_failures<S>(display: &mut S, screen_width: i32, failures: i32)
where
    S: DrawTarget<Color = Rgb565>,
{
    let text = format!("Failures: {}", failures);
    // Using FONT_10X20: approx. 10 pixels per character and 20 pixels tall.
    let text_width = 10 * text.len() as i32;
    let text_height = 20;
    let text_x = (screen_width - text_width) / 2;
    let text_y = 2; // near the top
                    // Draw a filled white rectangle to cover the grid behind the text.
    let _ = Rectangle::new(
        Point::new(text_x, text_y),
        Size::new(text_width as u32, text_height as u32),
    )
    .into_styled(
        PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::WHITE)
            .build(),
    )
    .draw(display);
    // Draw the failures text in red.
    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::RED);
    let _ = Text::with_baseline(&text, Point::new(text_x, text_y), text_style, Baseline::Top)
        .draw(display);
}

pub fn run<S, I2C, PINT, RST>(display: &mut S, capsense: &mut CST816S<I2C, PINT, RST>)
where
    I2C: hal::i2c::I2c,
    PINT: hal::digital::InputPin,
    RST: hal::digital::StatefulOutputPin,
    S: DrawTarget<Color = Rgb565> + OriginDimensions,
{
    let grid_spacing: i32 = 30;
    let screen_width: i32 = 240;
    let screen_height: i32 = 280;

    // Outer loop: run the test repeatedly until the user chooses "Finished".
    loop {
        // --- Test Phase ---
        // Clear the display and draw grid lines (with no vertical offset).
        let _ = display.clear(Rgb565::BLACK);
        let grid_line_color = Rgb565::WHITE;
        for x in (0..screen_width).step_by(grid_spacing as usize) {
            let _ = Rectangle::new(Point::new(x, 0), Size::new(1, screen_height as u32))
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(grid_line_color)
                        .build(),
                )
                .draw(display);
        }
        for y in (0..screen_height).step_by(grid_spacing as usize) {
            let _ = Rectangle::new(Point::new(0, y), Size::new(screen_width as u32, 1))
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(grid_line_color)
                        .build(),
                )
                .draw(display);
        }

        // Draw target squares using our helper function.
        for &(cell_x, cell_y, color) in TARGET_CELLS.iter() {
            let target = target_rect((cell_x, cell_y), grid_spacing);
            let _ = target
                .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
                .draw(display);
        }

        // Initialize failure counter and track which target cells are still active.
        let mut failures: i32 = 0;
        let mut active = [true; TARGET_CELLS.len()];
        // For debouncing: store the previous event action.
        let mut prev_action: Option<u8> = None;

        // Initially update the failures display.
        update_failures(display, screen_width, failures);

        // Main test loop: read touch events and handle targets.
        loop {
            if let Some(touch_event) = capsense.read_one_touch_event(true) {
                // Debounce: if the current event is lift-up and the previous event was also lift-up, skip.
                if touch_event.action == ACTION_LIFT_UP && prev_action == Some(ACTION_LIFT_UP) {
                    continue;
                }
                prev_action = Some(touch_event.action);
                if touch_event.action != ACTION_LIFT_UP {
                    continue;
                }

                // Calibrate the touch point.
                let touch_point = calibrate_point(Point::new(touch_event.x, touch_event.y));

                // Draw a small red square at the calibrated touch location.
                let red_square_size = Size::new(2, 2);
                let _ = Rectangle::new(touch_point, red_square_size)
                    .into_styled(PrimitiveStyleBuilder::new().fill_color(Rgb565::RED).build())
                    .draw(display);

                // Check if the touch hits any active target.
                let mut hit_target = false;
                for (i, &(cell_x, cell_y, _)) in TARGET_CELLS.iter().enumerate() {
                    let target = target_rect((cell_x, cell_y), grid_spacing);
                    if active[i] && target.contains(touch_point) {
                        active[i] = false;
                        hit_target = true;
                        // Clear the target by drawing it in black.
                        let _ = target
                            .into_styled(
                                PrimitiveStyleBuilder::new()
                                    .fill_color(Rgb565::BLACK)
                                    .build(),
                            )
                            .draw(display);
                    }
                }

                // Only count a failure if no active target was hit.
                if !hit_target {
                    failures += 1;
                    update_failures(display, screen_width, failures);
                }

                // If all target squares have been cleared, break out of the test loop.
                if active.iter().all(|&a| !a) {
                    break;
                }
            }
        }

        // --- Menu Phase ---
        // Clear the display and show two buttons plus the final failures count.
        let _ = display.clear(Rgb565::BLACK);

        // Compute button dimensions:
        //   - 80% of the screen width (240 * 0.8 = 192 pixels)
        //   - Centered horizontally (left margin of 24 pixels)
        //   - Buttons are 60 pixels tall.
        let button_width: i32 = (screen_width * 80) / 100; // 192 pixels
        let button_x: i32 = (screen_width - button_width) / 2; // 24 pixels
        let button_height: i32 = 60;

        // Define the top button ("Start Again") and bottom button ("Finished").
        let start_again_rect = Rectangle::new(
            Point::new(button_x, 20),
            Size::new(button_width as u32, button_height as u32),
        );
        let finished_rect = Rectangle::new(
            Point::new(button_x, screen_height - 20 - button_height),
            Size::new(button_width as u32, button_height as u32),
        );

        // Draw the buttons with high contrast: white fill with black text.
        let _ = start_again_rect
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::WHITE)
                    .build(),
            )
            .draw(display);
        let _ = finished_rect
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::WHITE)
                    .build(),
            )
            .draw(display);

        // Create text styles.
        let button_text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::BLACK);
        let failures_text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::RED);

        // Draw labels on the buttons.
        let start_text = "Start Again";
        let finished_text = "Finished";
        let start_text_width = 10 * start_text.len() as i32;
        let finished_text_width = 10 * finished_text.len() as i32;
        let start_text_x = button_x + (button_width - start_text_width) / 2;
        let start_text_y = 20 + (button_height - 20) / 2;
        let finished_text_x = button_x + (button_width - finished_text_width) / 2;
        let finished_text_y = (screen_height - 20 - button_height) + (button_height - 20) / 2;
        let _ = Text::with_baseline(
            start_text,
            Point::new(start_text_x, start_text_y),
            button_text_style,
            Baseline::Top,
        )
        .draw(display);
        let _ = Text::with_baseline(
            finished_text,
            Point::new(finished_text_x, finished_text_y),
            button_text_style,
            Baseline::Top,
        )
        .draw(display);

        // Draw the final failures count between the two buttons.
        let final_text = format!("Failures: {}", failures);
        let final_text_width = 10 * final_text.len() as i32;
        let final_text_x = (screen_width - final_text_width) / 2;
        let final_text_y =
            20 + button_height + ((screen_height - 20 - button_height - 20 - button_height) / 2);
        let _ = Text::with_baseline(
            &final_text,
            Point::new(final_text_x, final_text_y),
            failures_text_style,
            Baseline::Top,
        )
        .draw(display);

        // For the menu loop, use separate debounce logic.
        let mut prev_menu_action: Option<u8> = None;
        loop {
            if let Some(touch_event) = capsense.read_one_touch_event(true) {
                if touch_event.action == ACTION_LIFT_UP && prev_menu_action == Some(ACTION_LIFT_UP)
                {
                    continue;
                }
                prev_menu_action = Some(touch_event.action);
                if touch_event.action != ACTION_LIFT_UP {
                    continue;
                }
                let touch_point = calibrate_point(Point::new(touch_event.x, touch_event.y));
                if start_again_rect.contains(touch_point) {
                    // "Start Again" tapped: break out to re-run the test.
                    break;
                } else if finished_rect.contains(touch_point) {
                    // "Finished" tapped: exit the function.
                    return;
                }
            }
        }
    }
}
