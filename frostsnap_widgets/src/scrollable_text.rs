use crate::super_draw_target::SuperDrawTarget;
use crate::{palette::PALETTE, scroll_bar::ScrollBar, DynWidget, Widget};
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{renderer::TextRenderer, Alignment},
};

/// A text widget that can be scrolled vertically if content exceeds available height
pub struct ScrollableText<S> {
    text: String,
    style: S,
    alignment: Alignment,
    scroll_position: i32,
    max_size: Size,
    content_height: u32,
    scroll_bar: ScrollBar,
    needs_redraw: bool,
}

impl<S> ScrollableText<S>
where
    S: TextRenderer<Color = Rgb565> + Clone,
{
    pub fn new(text: String, style: S) -> Self {
        let thumb_size = crate::Frac::from_ratio(1, 2);
        Self {
            text,
            style,
            alignment: Alignment::Center,
            scroll_position: 0,
            max_size: Size::zero(),
            content_height: 0,
            scroll_bar: ScrollBar::new(thumb_size),
            needs_redraw: true,
        }
    }

    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    fn calculate_content_height(&self) -> u32 {
        use embedded_graphics::text::{Baseline, Text as EgText, TextStyleBuilder};

        let text_style = TextStyleBuilder::new()
            .alignment(self.alignment)
            .baseline(Baseline::Top)
            .build();

        let text_obj = EgText::with_text_style(
            self.text.as_str(),
            Point::zero(),
            self.style.clone(),
            text_style,
        );

        text_obj.bounding_box().size.height
    }

    fn update_scroll_bar(&mut self) {
        let scrollable_height = self.max_size.height as i32;
        let max_scroll = (self.content_height as i32)
            .saturating_sub(scrollable_height)
            .max(0);

        let fraction = if max_scroll > 0 {
            crate::Rat::from_ratio(self.scroll_position as u32, max_scroll as u32)
        } else {
            crate::Rat::ZERO
        };

        // Update thumb size based on visible ratio
        if self.content_height > 0 {
            let visible_ratio = self.max_size.height as f32 / self.content_height as f32;
            let thumb_size = crate::Frac::from_ratio(
                (visible_ratio * 100.0) as u32,
                100,
            ).min(crate::Frac::ONE);
            self.scroll_bar = ScrollBar::new(thumb_size);
        }

        self.scroll_bar.set_scroll_position(fraction);
    }
}

impl<S> DynWidget for ScrollableText<S>
where
    S: TextRenderer<Color = Rgb565> + Clone,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.max_size = max_size;
        self.content_height = self.calculate_content_height();
        self.update_scroll_bar();
    }

    fn sizing(&self) -> crate::Sizing {
        crate::Sizing {
            width: self.max_size.width,
            height: self.max_size.height.min(self.content_height),
        }
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        let delta = prev_y.map_or(0, |p| new_y as i32 - p as i32);

        if delta.abs() > 0 {
            let scrollable_height = self.max_size.height as i32;
            let max_scroll = (self.content_height as i32)
                .saturating_sub(scrollable_height)
                .max(0);

            let new_scroll_position = (self.scroll_position - delta).clamp(0, max_scroll);

            if new_scroll_position != self.scroll_position {
                self.scroll_position = new_scroll_position;
                self.update_scroll_bar();
                self.needs_redraw = true;
            }
        }
    }

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
        self.scroll_bar.force_full_redraw();
    }
}

impl<S> Widget for ScrollableText<S>
where
    S: TextRenderer<Color = Rgb565> + Clone,
{
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if !self.needs_redraw {
            return Ok(());
        }

        use embedded_graphics::text::{Baseline, Text as EgText, TextStyleBuilder};

        let bounds = target.bounding_box();

        // Clear background before drawing
        Rectangle::new(Point::zero(), bounds.size)
            .into_styled(PrimitiveStyleBuilder::new().fill_color(PALETTE.surface).build())
            .draw(target)?;

        let text_style = TextStyleBuilder::new()
            .alignment(self.alignment)
            .baseline(Baseline::Top)
            .build();

        // Calculate the y position for drawing text
        let text_y = -self.scroll_position;

        // Only draw if at least some part of the text is visible
        if text_y + self.content_height as i32 >= 0 && text_y < bounds.size.height as i32 {
            // Draw text with vertical offset for scrolling
            let mut text_obj = EgText::with_text_style(
                self.text.as_str(),
                Point::new(0, text_y),
                self.style.clone(),
                text_style,
            );

            // Adjust x position for center/right alignment
            if text_obj.bounding_box().top_left.x < 0 {
                text_obj.position.x += text_obj.bounding_box().top_left.x.abs();
            }

            let _ = text_obj.draw(target);
        }

        // Only draw scrollbar if content is larger than viewport
        if self.content_height > self.max_size.height {
            const SCROLLBAR_MARGIN: u32 = 2;
            let scrollbar_x = bounds.size.width as i32 - (crate::scroll_bar::SCROLLBAR_WIDTH + SCROLLBAR_MARGIN) as i32;
            let scrollbar_area = Rectangle::new(
                Point::new(scrollbar_x, 0),
                Size::new(crate::scroll_bar::SCROLLBAR_WIDTH, bounds.size.height),
            );
            self.scroll_bar.draw(&mut target.clone().crop(scrollbar_area));
        }

        self.needs_redraw = false;
        Ok(())
    }
}
