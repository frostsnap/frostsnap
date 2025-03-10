use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_iconoir::{icons, Icon};
use u8g2_fonts::U8g2TextStyle; // Assuming these exist in embedded_iconoir

pub struct BackupResultScreen {
    area: Size,
    ok: bool,
    // Only used for invalid backup (to offer a retry)
    try_again_rect: Rectangle,
}

impl BackupResultScreen {
    /// Create a new BackupResultScreen covering the given area.
    /// The `ok` parameter indicates if the backup was valid.
    pub fn new(area: Size, ok: bool) -> Self {
        // Define a try-again button near the bottom of the screen.
        // (It’s only drawn & active if `ok` is false.)
        let try_again_rect = Rectangle::new(
            Point::new(10, area.height as i32 - 50),
            Size::new(area.width - 20, 40),
        );
        Self {
            area,
            ok,
            try_again_rect,
        }
    }

    /// Draw the screen.
    ///
    /// Fills the background with green (if valid) or red (if invalid), then displays:
    /// - A centered text message ("Backup Valid" or "Invalid Backup")
    /// - An icon above the text (a check for valid, a cross for invalid)
    /// - If invalid, a "Try Again" button at the bottom.
    pub fn draw<D: DrawTarget<Color = Rgb565>>(&mut self, target: &mut D) -> Result<(), D::Error> {
        // Set background color based on backup result.
        let background_color = if self.ok { Rgb565::GREEN } else { Rgb565::RED };

        // Fill the entire area.
        Rectangle::new(Point::new(0, 0), self.area)
            .into_styled(PrimitiveStyle::with_fill(background_color))
            .draw(target)?;

        // Prepare a white text style using a simple font.
        let text_style = TextStyle::new(Font6x8, Rgb565::WHITE);

        // Set the result text.
        let result_text = if self.ok {
            "Backup Valid"
        } else {
            "Invalid Backup"
        };

        // Calculate position to center the text.
        let char_width = 6; // approximate width per character with Font6x8
        let text_width = result_text.len() as i32 * char_width;
        let text_height = 8;
        let text_x = (self.area.width as i32 - text_width) / 2;
        let text_y = (self.area.height as i32 - text_height) / 2;
        Text::new(result_text, Point::new(text_x, text_y))
            .into_styled(text_style)
            .draw(target)?;

        // Draw an icon above the text.
        // For valid backups, we use a check icon; for invalid ones, a cross.
        let icon_position = Point::new((self.area.width as i32 - 24) / 2, text_y - 30);
        if self.ok {
            Icon::<icons::size48px::Check>::default()
                .with_center(icon_position)
                .draw(target);
        } else {
            Icon::<icons::size48px::Cross>::default()
                .with_center(icon_position)
                .draw(target);
            // Draw a try-again button.
            self.try_again_rect
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
                .draw(target)?;

            // Center "Try Again" text in the button.
            let button_text = "Try Again";
            let btn_text_width = button_text.len() as i32 * char_width;
            let btn_text_height = 8;
            let btn_text_x = self.try_again_rect.top_left.x
                + (self.try_again_rect.size.width as i32 - btn_text_width) / 2;
            let btn_text_y = self.try_again_rect.top_left.y
                + (self.try_again_rect.size.height as i32 - btn_text_height) / 2;
            Text::new(button_text, Point::new(btn_text_x, btn_text_y))
                .into_styled(text_style)
                .draw(target)?;
        }

        Ok(())
    }

    /// Handle touch events.
    ///
    /// If the backup was invalid and the user lifts their finger
    /// over the "Try Again" button, we trigger a retry callback.
    pub fn handle_touch(&mut self, point: Point, _current_time: crate::Instant, lift_up: bool) {
        if !self.ok && lift_up && self.try_again_rect.contains(point) {
            self.on_try_again();
        }
    }

    /// Called when the user selects "Try Again."
    fn on_try_again(&mut self) {
        todo!()
        // Here you would normally trigger an event or reset the input state.
    }
}
