use crate::{
    palette::PALETTE, DefaultTextStyle, DynWidget, HoldToConfirm, Instant, SuperDrawTarget, Text,
    Widget, FONT_MED, HOLD_TO_CONFIRM_TIME_SHORT_MS,
};
use alloc::{format, vec::Vec};
use embedded_graphics::{
    mono_font::{iso_8859_1::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Baseline, Text as EgText},
};

const ACTION_LIFT_UP: u8 = 1;

// Target cells with colors for each corner
const TARGET_CELLS: [(i32, i32, Rgb565); 4] = [
    (0, 0, Rgb565::RED),    // Top left
    (5, 0, Rgb565::GREEN),  // Top right
    (0, 6, Rgb565::YELLOW), // Bottom left
    (5, 6, Rgb565::BLUE),   // Bottom right
];

/// Current phase of the screen test
enum Phase {
    Testing {
        failures: i32,
        active: [bool; 4],
        prev_action: Option<u8>,
        needs_redraw: bool,
        touch_points: Vec<Point>, // Store all touch points to draw red dots
    },
    Menu {
        failures: i32,
        hold_to_confirm: HoldToConfirm<Text>,
        start_again_rect: Rectangle,
        prev_action: Option<u8>,
        needs_redraw: bool,
    },
}

pub struct ScreenTest {
    phase: Phase,
    grid_spacing: i32,
    screen_width: i32,
    screen_height: i32,
    max_size: Size,
}

impl Default for ScreenTest {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenTest {
    pub fn new() -> Self {
        Self {
            phase: Phase::Testing {
                failures: 0,
                active: [true; 4],
                prev_action: None,
                needs_redraw: true,
                touch_points: Vec::new(),
            },
            grid_spacing: 40,
            screen_width: 240,
            screen_height: 280,
            max_size: Size::zero(),
        }
    }

    fn target_rect(cell: (i32, i32), grid_spacing: i32) -> Rectangle {
        let (cell_x, cell_y) = cell;
        Rectangle::new(
            Point::new(cell_x * grid_spacing + 1, cell_y * grid_spacing + 1),
            Size::new((grid_spacing - 2) as u32, (grid_spacing - 2) as u32),
        )
    }

    fn draw_failures<D>(target: &mut D, failures: i32, screen_width: i32) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        let text = format!("Failures: {failures}");
        let text_width = 10 * text.len() as i32;
        let text_height = 20;
        let text_x = (screen_width - text_width) / 2;
        let text_y = 2;

        // Draw white background
        Rectangle::new(
            Point::new(text_x, text_y),
            Size::new(text_width as u32, text_height as u32),
        )
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(Rgb565::WHITE)
                .build(),
        )
        .draw(target)?;

        // Draw red text
        EgText::with_baseline(
            &text,
            Point::new(text_x, text_y),
            MonoTextStyle::new(&FONT_10X20, Rgb565::RED),
            Baseline::Top,
        )
        .draw(target)?;

        Ok(())
    }

    fn draw_testing<D>(
        target: &mut D,
        failures: i32,
        active: &[bool; 4],
        grid_spacing: i32,
        screen_width: i32,
        screen_height: i32,
        touch_points: &[Point],
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Rgb565>,
    {
        // Clear screen
        target.clear(Rgb565::BLACK)?;

        // Draw grid lines
        let grid_line_style = PrimitiveStyleBuilder::new()
            .fill_color(Rgb565::WHITE)
            .build();

        for x in (0..screen_width).step_by(grid_spacing as usize) {
            Rectangle::new(Point::new(x, 0), Size::new(1, screen_height as u32))
                .into_styled(grid_line_style)
                .draw(target)?;
        }
        for y in (0..screen_height).step_by(grid_spacing as usize) {
            Rectangle::new(Point::new(0, y), Size::new(screen_width as u32, 1))
                .into_styled(grid_line_style)
                .draw(target)?;
        }

        // Draw active target squares
        for (i, &(cell_x, cell_y, color)) in TARGET_CELLS.iter().enumerate() {
            if active[i] {
                Self::target_rect((cell_x, cell_y), grid_spacing)
                    .into_styled(PrimitiveStyleBuilder::new().fill_color(color).build())
                    .draw(target)?;
            }
        }

        // Draw failures counter
        Self::draw_failures(target, failures, screen_width)?;

        // Draw red dots at touch points
        for &touch_point in touch_points {
            Rectangle::new(touch_point, Size::new(2, 2))
                .into_styled(PrimitiveStyleBuilder::new().fill_color(Rgb565::RED).build())
                .draw(target)?;
        }

        Ok(())
    }

    pub fn is_completed(&self) -> bool {
        matches!(&self.phase, Phase::Menu { hold_to_confirm, .. } if hold_to_confirm.is_completed())
    }

    pub fn get_failures(&self) -> i32 {
        match &self.phase {
            Phase::Testing { failures, .. } | Phase::Menu { failures, .. } => *failures,
        }
    }
}

impl DynWidget for ScreenTest {
    fn set_constraints(&mut self, max_size: Size) {
        self.max_size = max_size;
    }

    fn sizing(&self) -> crate::Sizing {
        self.max_size.into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        let action = if is_release { ACTION_LIFT_UP } else { 0 };
        let grid_spacing = self.grid_spacing;
        let screen_width = self.screen_width;
        let screen_height = self.screen_height;

        match &mut self.phase {
            Phase::Testing {
                failures,
                active,
                prev_action,
                needs_redraw,
                touch_points,
            } => {
                if action == ACTION_LIFT_UP && *prev_action == Some(ACTION_LIFT_UP) {
                    return None;
                }
                *prev_action = Some(action);

                if action != ACTION_LIFT_UP {
                    return None;
                }

                touch_points.push(point);

                let mut hit_target = false;
                for (i, &(cell_x, cell_y, _)) in TARGET_CELLS.iter().enumerate() {
                    if active[i]
                        && Self::target_rect((cell_x, cell_y), grid_spacing).contains(point)
                    {
                        active[i] = false;
                        hit_target = true;
                        break;
                    }
                }

                if !hit_target {
                    *failures += 1;
                }
                *needs_redraw = true;

                // Check if all targets are cleared
                if active.iter().all(|&a| !a) {
                    // Transition to menu phase
                    let final_failures = *failures;
                    let test_complete_text = format!("Test Complete\n\nFailures: {final_failures}");
                    let text_widget = Text::new(
                        test_complete_text,
                        DefaultTextStyle::new(FONT_MED, PALETTE.on_background),
                    )
                    .with_alignment(embedded_graphics::text::Alignment::Center);

                    let mut hold_to_confirm =
                        HoldToConfirm::new(HOLD_TO_CONFIRM_TIME_SHORT_MS, text_widget);

                    // Set constraints for the hold_to_confirm widget (below start again button)
                    let button_y = 20;
                    let button_height = 60;
                    let button_spacing = 10;
                    let widget_y = button_y + button_height + button_spacing;
                    let widget_height = screen_height - widget_y;
                    hold_to_confirm
                        .set_constraints(Size::new(screen_width as u32, widget_height as u32));

                    let button_width = (screen_width * 80) / 100;
                    let button_x = (screen_width - button_width) / 2;
                    let start_again_rect = Rectangle::new(
                        Point::new(button_x, button_y),
                        Size::new(button_width as u32, button_height as u32),
                    );

                    self.phase = Phase::Menu {
                        failures: final_failures,
                        hold_to_confirm,
                        start_again_rect,
                        prev_action: None,
                        needs_redraw: true,
                    };
                }

                None
            }
            Phase::Menu {
                hold_to_confirm,
                start_again_rect,
                prev_action,
                ..
            } => {
                if action == ACTION_LIFT_UP && *prev_action == Some(ACTION_LIFT_UP) {
                    return None;
                }
                *prev_action = Some(action);

                if is_release && start_again_rect.contains(point) {
                    // Set prev_action to prevent touch from being processed again
                    self.phase = Phase::Testing {
                        failures: 0,
                        active: [true; 4],
                        prev_action: Some(ACTION_LIFT_UP),
                        needs_redraw: true,
                        touch_points: Vec::new(),
                    };
                    return None;
                }

                // Pass touch to HoldToConfirm widget (translate coordinates)
                let button_y = 20;
                let button_height = 60;
                let button_spacing = 10;
                let widget_y = button_y + button_height + button_spacing;
                if point.y >= widget_y {
                    let widget_point = Point::new(point.x, point.y - widget_y);
                    hold_to_confirm.handle_touch(widget_point, current_time, is_release);
                }

                None
            }
        }
    }

    fn force_full_redraw(&mut self) {
        if let Phase::Menu {
            hold_to_confirm, ..
        } = &mut self.phase
        {
            hold_to_confirm.force_full_redraw();
        }
    }
}

impl Widget for ScreenTest {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        match &mut self.phase {
            Phase::Testing {
                failures,
                active,
                needs_redraw,
                touch_points,
                ..
            } => {
                if *needs_redraw {
                    Self::draw_testing(
                        target,
                        *failures,
                        active,
                        self.grid_spacing,
                        self.screen_width,
                        self.screen_height,
                        touch_points,
                    )?;
                    *needs_redraw = false;
                }
            }
            Phase::Menu {
                hold_to_confirm,
                start_again_rect,
                needs_redraw,
                ..
            } => {
                if *needs_redraw {
                    target.clear(Rgb565::BLACK)?;

                    start_again_rect
                        .into_styled(
                            PrimitiveStyleBuilder::new()
                                .fill_color(Rgb565::WHITE)
                                .build(),
                        )
                        .draw(target)?;

                    let button_text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::BLACK);
                    let start_text = "Start Again";
                    let button_width = (self.screen_width * 80) / 100;
                    let button_x = (self.screen_width - button_width) / 2;
                    let button_y = 20;
                    let button_height = 60;
                    let text_height = 20;
                    let start_text_width = 10 * start_text.len() as i32;
                    let start_text_x = button_x + (button_width - start_text_width) / 2;
                    let start_text_y = button_y + (button_height - text_height) / 2;
                    EgText::with_baseline(
                        start_text,
                        Point::new(start_text_x, start_text_y),
                        button_text_style,
                        Baseline::Top,
                    )
                    .draw(target)?;

                    *needs_redraw = false;
                }

                // Always draw HoldToConfirm widget (it has animations)
                let button_y = 20;
                let button_height = 60;
                let button_spacing = 10;
                let widget_y = button_y + button_height + button_spacing;
                hold_to_confirm.draw(
                    &mut target.clone().translate(Point::new(0, widget_y)),
                    current_time,
                )?;
            }
        }

        Ok(())
    }
}
