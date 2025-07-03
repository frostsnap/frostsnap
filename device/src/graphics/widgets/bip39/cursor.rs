#[derive(Debug)]
struct Cursor {
    visible: bool,
    last_toggle: Option<crate::Instant>,
    pub position: Point,
}

impl Cursor {
    fn new(position: Point) -> Self {
        Self {
            visible: true,
            last_toggle: None,
            position,
        }
    }

    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        // Update visibility based on time
        let cursor_rect = Rectangle::new(
            Point::new(
                self.position.x,
                self.position.y + FONT_SIZE.height as i32 - 4,
            ),
            Size::new(FONT_SIZE.width - 4, 2),
        );

        if let Some(last_toggle) = self.last_toggle {
            // Check if 600ms has passed since last toggle
            if current_time
                .checked_duration_since(last_toggle)
                .map(|d| d.to_millis() >= 600)
                .unwrap_or(false)
            {
                self.visible = !self.visible;
                self.last_toggle = Some(current_time);

                // Draw or clear based on new visibility state
                if self.visible {
                    let _ = cursor_rect
                        .into_styled(PrimitiveStyle::with_fill(COLORS.primary))
                        .draw(target);
                } else {
                    let _ = cursor_rect
                        .into_styled(PrimitiveStyle::with_fill(COLORS.background))
                        .draw(target);
                }
            }
        } else {
            // First time - draw cursor
            self.last_toggle = Some(current_time);
            let _ = cursor_rect
                .into_styled(PrimitiveStyle::with_fill(COLORS.primary))
                .draw(target);
        }
    }
}
