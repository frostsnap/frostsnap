use crate::aa::rounded_rect::AARoundedRectangle;
use crate::super_draw_target::SuperDrawTarget;
use crate::DefaultTextStyle;
use crate::{palette::PALETTE, Column, Frac, Switcher, Text as TextWidget, Widget, FONT_SMALL};
use alloc::format;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    primitives::Rectangle,
    text::Alignment,
    Drawable,
};

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
}

impl ProgressBar {
    /// Create a new progress bar
    pub fn new() -> Self {
        Self {
            progress: Frac::ZERO,
            bar_height: 20,
            corner_radius: 10,
            bar_rect: None,
        }
    }

    /// Create a new progress bar with custom dimensions
    pub fn with_dimensions(bar_height: u32, corner_radius: u32) -> Self {
        Self {
            progress: Frac::ZERO,
            bar_height,
            corner_radius,
            bar_rect: None,
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

        // Our AA rounded rect writes every pixel of its bounding rect (corners
        // are blended against the backdrop), so redrawing it covers everything
        // it previously lit — unlike embedded-graphics' RoundedRectangle, whose
        // silently confined radius at small widths orphans corner pixels.
        let background_color = target.background_color();

        AARoundedRectangle::new(bar_rect, background_color)
            .with_corner_radius(self.corner_radius)
            .with_border(PALETTE.outline, 2)
            .draw(target)?;

        // Calculate the filled width based on progress
        let filled_width = (self.progress * bar_rect.size.width).round().max(1);

        if self.progress > Frac::ZERO && filled_width > 4 {
            let fill_rect = Rectangle::new(
                Point::new(bar_rect.top_left.x + 2, bar_rect.top_left.y + 2),
                Size::new(filled_width.saturating_sub(4), self.bar_height - 4),
            );

            AARoundedRectangle::new(fill_rect, background_color)
                .with_corner_radius(self.corner_radius.saturating_sub(2))
                .with_fill(PALETTE.primary)
                .draw(target)?;
        }

        Ok(())
    }
}

/// A progress indicator widget with a progress bar and percentage text
#[derive(frostsnap_macros::Widget)]
pub struct ProgressIndicator {
    /// Column containing the progress bar, spacer, and text switcher
    #[widget_delegate]
    column: Column<(ProgressBar, Switcher<TextWidget>)>,
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
        let initial_text = TextWidget::new(
            "00%",
            DefaultTextStyle::new(FONT_SMALL, PALETTE.on_background),
        )
        .with_alignment(Alignment::Center);
        let text_switcher = Switcher::new(initial_text).with_shrink_to_fit();

        let column = Column::builder()
            .push(progress_bar)
            .gap(8)
            .push(text_switcher);

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
            let percentage_text = format!("{percentage}%");
            let new_text = TextWidget::new(
                percentage_text,
                DefaultTextStyle::new(FONT_SMALL, PALETTE.on_background),
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
