use super::Widget;
use crate::{super_draw_target::SuperDrawTarget, Instant};
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Dimensions, Point, Size},
    pixelcolor::PixelColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::{
        renderer::{CharacterStyle, TextRenderer},
        Alignment, Baseline, Text as EgText, TextStyle, TextStyleBuilder,
    },
    Drawable,
};

/// Distance in pixels between the bottom of the text and the underline
const UNDERLINE_DISTANCE: i32 = 2;

/// A simple text widget that renders text at a specific position
#[derive(Clone)]
pub struct Text<S: CharacterStyle> {
    text: String,
    character_style: S,
    text_style: TextStyle,
    underline_color: Option<<S as CharacterStyle>::Color>,
    drawn: bool,
    cached_size: Size,
}

impl<S, C> Text<S>
where
    C: PixelColor,
    S: CharacterStyle<Color = C> + TextRenderer<Color = C> + Clone,
{
    pub fn new<T: Into<String>>(text: T, character_style: S) -> Self {
        let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
        let text_string = text.into();

        // Calculate size once during creation
        let text_obj = EgText::with_text_style(
            &text_string,
            Point::zero(),
            character_style.clone(),
            text_style,
        );
        let bbox = text_obj.bounding_box();
        let cached_size = bbox.size;

        Self {
            text: text_string,
            character_style,
            text_style,
            underline_color: None,
            drawn: false,
            cached_size,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Create the EgText object at the given position
    fn create_eg_text(&self) -> EgText<'_, S> {
        EgText::with_text_style(
            &self.text,
            Point::zero(),
            self.character_style.clone(),
            self.text_style,
        )
    }

    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.text_style = TextStyleBuilder::from(&self.text_style)
            .alignment(alignment)
            .build();
        // Recalculate size with new alignment
        let text_obj = self.create_eg_text();
        let bbox = text_obj.bounding_box();
        self.cached_size = bbox.size;
        self
    }

    pub fn with_underline(mut self, color: <S as CharacterStyle>::Color) -> Self {
        self.underline_color = Some(color);
        // Add space for underline to cached size
        self.cached_size.height += UNDERLINE_DISTANCE as u32 + 1;
        self
    }
}

impl<S, C> crate::DynWidget for Text<S>
where
    C: PixelColor,
    S: CharacterStyle<Color = C> + TextRenderer<Color = C> + Clone,
{
    fn set_constraints(&mut self, _max_size: Size) {
        // Text has a fixed size based on its content, no action needed
    }

    fn sizing(&self) -> crate::Sizing {
        self.cached_size.into()
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // No drag handling needed
    }

    fn force_full_redraw(&mut self) {
        self.drawn = false;
    }
}

impl<S, C> Widget for Text<S>
where
    C: crate::WidgetColor,
    S: CharacterStyle<Color = C> + TextRenderer<Color = C> + Clone,
{
    type Color = C;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if !self.drawn {
            let mut text_obj = self.create_eg_text();
            if text_obj.bounding_box().top_left.x < 0 {
                text_obj.position.x += text_obj.bounding_box().top_left.x.abs();
            }
            text_obj.draw(target)?;

            // Draw underline if enabled
            if let Some(underline_color) = self.underline_color {
                let text_bbox = text_obj.bounding_box();
                let underline_y = text_bbox.bottom_right().unwrap().y + UNDERLINE_DISTANCE;

                Line::new(
                    Point::new(text_bbox.top_left.x, underline_y),
                    Point::new(text_bbox.bottom_right().unwrap().x, underline_y),
                )
                .into_styled(PrimitiveStyle::with_stroke(underline_color, 1))
                .draw(target)?;
            }

            self.drawn = true;
        }

        Ok(())
    }
}
