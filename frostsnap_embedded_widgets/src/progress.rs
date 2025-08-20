use crate::super_draw_target::SuperDrawTarget;
use crate::{
    palette::PALETTE, Column, Frac, Switcher, Text as TextWidget, Widget, FONT_SMALL,
};
use alloc::format;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    primitives::{Primitive, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
    text::Alignment,
    Drawable,
};
use u8g2_fonts::U8g2TextStyle;

/// A progress bar widget with a rounded rectangle (no text)
pub struct ProgressBar {
    /// The current progress as a fraction (0.0 to 1.0)
    progress: Frac,
    /// Height of the progress bar
    bar_height: u32,
    /// Corner radius for the rounded rectangles
    corner_radius: u32,
    /// Padding from edges
    bar_rect: Option<Rectangle>,
    /// Last drawn filled width to track changes
    last_filled_width: Option<u32>,
    /// Whether background has been drawn
    background_drawn: bool,
}

impl ProgressBar {
    /// Create a new progress bar
    pub fn new() -> Self {
        Self {
            progress: Frac::ZERO,
            bar_height: 20,
            corner_radius: 10,
            bar_rect: None,
            last_filled_width: None,
            background_drawn: false,
        }
    }

    /// Create a new progress bar with custom dimensions
    pub fn with_dimensions(bar_height: u32, corner_radius: u32) -> Self {
        Self {
            progress: Frac::ZERO,
            bar_height,
            corner_radius,
            bar_rect: None,
            last_filled_width: None,
            background_drawn: false,
        }
    }

    /// Set the progress (0.0 to 1.0)
    pub fn set_progress(&mut self, progress: Frac) {
        self.progress = progress;
    }

    /// Get the current progress
    pub fn progress(&self) -> Frac {
        self.progress
    }
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::DynWidget for ProgressBar {
    fn sizing(&self) -> crate::Sizing {
        self.bar_rect
            .expect("ProgressBar::sizing called before set_constraints")
            .size
            .into()
    }

    fn set_constraints(&mut self, max_size: Size) {
        self.bar_rect = Some(Rectangle::new(
            Point::new(0, 0),
            Size::new(max_size.width, self.bar_height),
        ));

        // Reset drawing state when constraints change
        self.background_drawn = false;
        self.last_filled_width = None;
    }

    fn force_full_redraw(&mut self) {
        self.background_drawn = false;
        self.last_filled_width = None;
    }
}

impl Widget for ProgressBar {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        let bar_rect = self
            .bar_rect
            .expect("ProgressBar::draw called before set_constraints");

        // Draw the background/border only if not already drawn
        if !self.background_drawn {
            let background_rect = RoundedRectangle::with_equal_corners(
                bar_rect,
                Size::new(self.corner_radius, self.corner_radius),
            );

            let background_style = PrimitiveStyleBuilder::new()
                .stroke_color(PALETTE.outline)
                .stroke_width(2)
                .build();

            background_rect.into_styled(background_style).draw(target)?;
        }

        // Calculate the filled width based on progress
        let filled_width = (self.progress * bar_rect.size.width).round().max(1);

        // Only redraw if the filled width has changed
        if self.last_filled_width != Some(filled_width) {
            // Clear the inside of the bar first (in case progress decreased)
            if let Some(last_width) = self.last_filled_width {
                if filled_width < last_width {
                    // Clear the area that was previously filled
                    let clear_rect = Rectangle::new(
                        Point::new(
                            bar_rect.top_left.x + 2 + filled_width as i32,
                            bar_rect.top_left.y + 2,
                        ),
                        Size::new(last_width - filled_width, self.bar_height - 4),
                    );
                    let clear_style = PrimitiveStyle::with_fill(PALETTE.background);
                    clear_rect.into_styled(clear_style).draw(target)?;
                }
            }

            // Draw the filled progress rectangle (if there's any progress)
            if self.progress > Frac::ZERO && filled_width > 2 {
                // Account for the border width
                let fill_rect = RoundedRectangle::with_equal_corners(
                    Rectangle::new(
                        Point::new(bar_rect.top_left.x + 2, bar_rect.top_left.y + 2),
                        Size::new(filled_width.saturating_sub(4), self.bar_height - 4),
                    ),
                    Size::new(
                        self.corner_radius.saturating_sub(2),
                        self.corner_radius.saturating_sub(2),
                    ),
                );

                let fill_style = PrimitiveStyle::with_fill(PALETTE.primary);
                fill_rect.into_styled(fill_style).draw(target)?;
            }

            self.last_filled_width = Some(filled_width);
        }

        Ok(())
    }
}

/// A progress indicator widget with a progress bar and percentage text
#[derive(frostsnap_macros::Widget)]
pub struct ProgressIndicator {
    /// Column containing the progress bar, spacer, and text switcher
    #[widget_delegate]
    column: Column<(ProgressBar, Switcher<TextWidget<U8g2TextStyle<Rgb565>>>)>,
    /// Last percentage to track changes
    last_percentage: u32,
}

impl Default for ProgressIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressIndicator {
    /// Create a new progress indicator
    pub fn new() -> Self {
        let progress_bar = ProgressBar::new();
        let initial_text =
            TextWidget::new("00%", U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background))
                .with_alignment(Alignment::Center);
        let text_switcher = Switcher::new(initial_text);

        let column = Column::builder()
            .push(progress_bar)
            .push(text_switcher).gap(8);

        Self {
            column,
            last_percentage: 0,
        }
    }

    /// Set the progress (0.0 to 1.0)
    pub fn set_progress(&mut self, progress: Frac) {
        // Update progress bar
        self.column.children.0.set_progress(progress);

        // Update text if percentage changed
        let percentage = (progress * 100u32).round();
        if percentage != self.last_percentage {
            self.last_percentage = percentage;
            let percentage_text = format!("{:02}%", percentage);
            let new_text = TextWidget::new(
                percentage_text,
                U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background),
            )
            .with_alignment(Alignment::Center);
            self.column.children.1.switch_to(new_text);
        }
    }

    /// Get the current progress
    pub fn progress(&self) -> Frac {
        self.column.children.0.progress()
    }
}

// All trait implementations are now generated by the derive macro
