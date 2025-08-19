use crate::{Widget, Container, Switcher, Text as TextWidget};
use crate::super_draw_target::SuperDrawTarget;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    prelude::*,
    text::{Text as EgText, TextStyle, TextStyleBuilder, Baseline, renderer::{CharacterStyle, TextRenderer}},
    pixelcolor::Rgb565,
};
use alloc::string::String;

/// A mutable text widget that maintains consistent size across text changes
pub struct MutText<S>
where
    S: CharacterStyle<Color = Rgb565> + TextRenderer<Color = Rgb565> + Clone,
{
    container: Container<Switcher<TextWidget<S>>>,
    character_style: S,
    max_chars: usize,
}

impl<S> MutText<S>
where
    S: CharacterStyle<Color = Rgb565> + TextRenderer<Color = Rgb565> + Clone,
{
    pub fn new(text: impl Into<String>, character_style: S, max_chars: usize) -> Self {
        // Calculate the maximum size needed by creating a test string with max chars
        let test_string = "M".repeat(max_chars); // Use 'M' as it's typically widest
        let test_text = EgText::with_text_style(
            &test_string,
            Point::zero(),
            character_style.clone(),
            TextStyleBuilder::new().baseline(Baseline::Top).build(),
        );
        let max_size = test_text.bounding_box().size;

        // Create initial text widget
        let text_widget = TextWidget::new(text, character_style.clone());

        // Create switcher and wrap in fixed-size container
        let switcher = Switcher::new(text_widget);
        let container = Container::with_size(switcher, max_size);

        Self {
            container,
            character_style,
            max_chars,
        }
    }

    /// Set new text widget
    pub fn set_text(&mut self, text_widget: TextWidget<S>) {
        self.container.child.switch_to(text_widget);
    }

    /// Get the current text
    pub fn text(&self) -> &str {
        self.container.child.current().text()
    }
}

impl<S> crate::DynWidget for MutText<S>
where
    S: CharacterStyle<Color = Rgb565> + TextRenderer<Color = Rgb565> + Clone,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.container.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.container.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.container.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.container.handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn force_full_redraw(&mut self) {
        self.container.force_full_redraw()
    }
}

impl<S> Widget for MutText<S>
where
    S: CharacterStyle<Color = Rgb565> + TextRenderer<Color = Rgb565> + Clone,
{
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        self.container.draw(target, current_time)
    }
}
