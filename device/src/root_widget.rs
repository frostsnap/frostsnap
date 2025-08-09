use crate::widget_tree::WidgetTree;
use alloc::string::{String, ToString};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
};
use frostsnap_embedded_widgets::{text::Text, DynWidget, FadeSwitcher, Widget};
use u8g2_fonts::U8g2TextStyle;

/// Root widget that contains the main widget tree and optional debug text
pub struct RootWidget {
    pub page_switcher: FadeSwitcher<WidgetTree>,
    pub debug_text: FadeSwitcher<Text<U8g2TextStyle<Rgb565>>>,
}

impl RootWidget {
    pub fn new(
        initial_widget: WidgetTree,
        fade_duration_ms: u32,
        background_color: Rgb565,
    ) -> Self {
        let style = U8g2TextStyle::new(frostsnap_embedded_widgets::FONT_SMALL, Rgb565::GREEN);
        let debug_text = Text::new("", style);

        Self {
            page_switcher: FadeSwitcher::new(
                initial_widget,
                fade_duration_ms,
                30,
                background_color,
            ),
            debug_text: FadeSwitcher::new(debug_text, 0, 0, background_color), // 0ms for instant switch
        }
    }

    pub fn set_debug_text(&mut self, text: impl ToString) {
        let new_text = text.to_string();

        // Insert newlines every 20 characters
        let mut formatted = String::new();
        let mut chars_in_line = 0;
        for ch in new_text.chars() {
            if chars_in_line >= 20 && ch != '\n' {
                formatted.push('\n');
                chars_in_line = 0;
            }
            formatted.push(ch);
            if ch == '\n' {
                chars_in_line = 0;
            } else {
                chars_in_line += 1;
            }
        }

        if &formatted == self.debug_text.current().text() {
            return;
        }
        let style = U8g2TextStyle::new(frostsnap_embedded_widgets::FONT_SMALL, Rgb565::RED);
        let new_text = Text::new(formatted, style);
        self.debug_text.switch_to(new_text);
    }

    /// Forward switch_to calls to the FadeSwitcher
    pub fn switch_to(&mut self, new_widget: WidgetTree) {
        self.page_switcher.switch_to(new_widget);
    }

    /// Get a mutable reference to the current widget
    pub fn current_mut(&mut self) -> &mut WidgetTree {
        self.page_switcher.current_mut()
    }
}

impl DynWidget for RootWidget {
    fn set_constraints(&mut self, max_size: Size) {
        self.page_switcher.set_constraints(max_size);
        self.debug_text.set_constraints(max_size);
    }
    
    fn sizing(&self) -> frostsnap_embedded_widgets::Sizing {
        self.page_switcher.sizing()
    }
    
    fn flex(&self) -> bool {
        self.page_switcher.flex()
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: frostsnap_embedded_widgets::Instant,
        is_release: bool,
    ) -> Option<frostsnap_embedded_widgets::KeyTouch> {
        // Forward touch to page_switcher
        self.page_switcher
            .handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.page_switcher
            .handle_vertical_drag(prev_y, new_y, is_release)
    }

    fn size_hint(&self) -> Option<Size> {
        self.page_switcher.size_hint()
    }

    fn force_full_redraw(&mut self) {
        self.page_switcher.force_full_redraw();
        self.debug_text.force_full_redraw();
    }
}

impl Widget for RootWidget {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: frostsnap_embedded_widgets::Instant,
    ) -> Result<(), D::Error> {
        // First draw the page_switcher (which handles fading between widgets)
        self.page_switcher.draw(target, current_time)?;

        // Then draw debug text on top of everything
        let mut cropped = target.translated(Point::new(20, 5));
        self.debug_text
            .draw(&mut cropped.clipped(&cropped.bounding_box()), current_time)?;

        Ok(())
    }
}
