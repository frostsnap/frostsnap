use crate::{Widget, Frac, Column, Text as TextWidget, Switcher, SizedBox, palette::PALETTE, FONT_SMALL};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    primitives::{Rectangle, RoundedRectangle, PrimitiveStyleBuilder, PrimitiveStyle, Primitive},
    text::Alignment,
    Drawable,
};
use u8g2_fonts::U8g2TextStyle;
use alloc::format;

/// A progress bar widget with a rounded rectangle (no text)
pub struct ProgressBar {
    /// The current progress as a fraction (0.0 to 1.0)
    progress: Frac,
    /// Height of the progress bar
    bar_height: u32,
    /// Corner radius for the rounded rectangles
    corner_radius: u32,
    /// Padding from edges
    padding: u32,
}

impl ProgressBar {
    /// Create a new progress bar
    pub fn new() -> Self {
        Self {
            progress: Frac::ZERO,
            bar_height: 20,
            corner_radius: 10,
            padding: 20,
        }
    }
    
    /// Create a new progress bar with custom dimensions
    pub fn with_dimensions(bar_height: u32, corner_radius: u32, padding: u32) -> Self {
        Self {
            progress: Frac::ZERO,
            bar_height,
            corner_radius,
            padding,
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
        crate::Sizing { width: 240, height: 280 }
    }
    
    
    fn force_full_redraw(&mut self) {
        // No internal state to reset
    }
}

impl Widget for ProgressBar {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        let bounds = target.bounding_box();
        let size = bounds.size;
        let center_x = bounds.center().x;
        let center_y = bounds.center().y;
        let bar_width = size.width - (self.padding * 2);
        let bar_x = center_x - (bar_width / 2) as i32;
        let bar_y = center_y - (self.bar_height / 2) as i32;
        
        // Draw the background/border rounded rectangle
        let background_rect = RoundedRectangle::with_equal_corners(
            Rectangle::new(
                Point::new(bar_x, bar_y),
                Size::new(bar_width, self.bar_height),
            ),
            Size::new(self.corner_radius, self.corner_radius),
        );
        
        let background_style = PrimitiveStyleBuilder::new()
            .stroke_color(PALETTE.outline)
            .stroke_width(2)
            .build();
            
        background_rect.into_styled(background_style).draw(target)?;
        
        // Calculate the filled width based on progress
        let filled_width = (self.progress * bar_width).round().max(1);
        
        // Draw the filled progress rectangle (if there's any progress)
        if self.progress > Frac::ZERO && filled_width > 2 {
            // Account for the border width
            let fill_rect = RoundedRectangle::with_equal_corners(
                Rectangle::new(
                    Point::new(bar_x + 2, bar_y + 2),
                    Size::new(filled_width.saturating_sub(4), self.bar_height - 4),
                ),
                Size::new(self.corner_radius.saturating_sub(2), self.corner_radius.saturating_sub(2)),
            );
            
            let fill_style = PrimitiveStyle::with_fill(PALETTE.primary);
            fill_rect.into_styled(fill_style).draw(target)?;
        }
        
        Ok(())
    }
}

/// A progress indicator widget with a progress bar and percentage text
pub struct ProgressIndicator {
    /// Column containing the progress bar, spacer, and text switcher
    column: Column<(ProgressBar, SizedBox<Rgb565>, Switcher<TextWidget<U8g2TextStyle<Rgb565>>>)>,
    /// Last percentage to track changes
    last_percentage: u32,
}

impl ProgressIndicator {
    /// Create a new progress indicator
    pub fn new() -> Self {
        let progress_bar = ProgressBar::new();
        let spacer = SizedBox::height(5);
        let initial_text = TextWidget::new(
            "00%",
            U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background)
        ).with_alignment(Alignment::Center);
        let text_switcher = Switcher::new(initial_text);
        
        let column = Column::new((progress_bar, spacer, text_switcher));

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
                U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background)
            ).with_alignment(Alignment::Center);
            self.column.children.2.switch_to(new_text);
        }
    }
    
    /// Get the current progress
    pub fn progress(&self) -> Frac {
        self.column.children.0.progress()
    }
}

impl crate::DynWidget for ProgressIndicator {
    fn sizing(&self) -> crate::Sizing {
        crate::Sizing { width: 240, height: 280 }
    }
    
    
    fn force_full_redraw(&mut self) {
        self.column.force_full_redraw()
    }
    
    fn handle_touch(
        &mut self,
        point: embedded_graphics::geometry::Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.column.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.column.handle_vertical_drag(prev_y, new_y, is_release)
    }
}

impl Widget for ProgressIndicator {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }
}
