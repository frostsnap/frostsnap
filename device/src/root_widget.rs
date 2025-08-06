use crate::widget_tree::WidgetTree;
use frostsnap_embedded_widgets::{DynWidget, Widget, text::Text, FadeSwitcher};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Rgb565,
    prelude::*,
};
use u8g2_fonts::U8g2TextStyle;
use alloc::string::ToString;

/// Root widget that contains the main widget tree and optional debug text
pub struct RootWidget {
    pub page_switcher: FadeSwitcher<WidgetTree>,
    pub debug_text: FadeSwitcher<Text<U8g2TextStyle<Rgb565>>>,
}

impl RootWidget {
    pub fn new(initial_widget: WidgetTree, fade_duration_ms: u32, background_color: Rgb565) -> Self {
        let name = initial_widget.widget_name();
        let style = U8g2TextStyle::new(
            frostsnap_embedded_widgets::FONT_SMALL,
            Rgb565::GREEN,
        );
        let debug_text = Text::new(name.to_string(), style);
        
        Self {
            page_switcher: FadeSwitcher::new(initial_widget, fade_duration_ms, background_color),
            debug_text: FadeSwitcher::new(debug_text, 0, background_color), // 0ms for instant switch
        }
    }
    
    pub fn set_debug_text(&mut self, text: impl ToString) {
        let style = U8g2TextStyle::new(
            frostsnap_embedded_widgets::FONT_SMALL,
            Rgb565::GREEN,
        );
        let new_text = Text::new(text.to_string(), style);
        self.debug_text.switch_to(new_text);
    }
    
    /// Forward switch_to calls to the FadeSwitcher
    pub fn switch_to(&mut self, new_widget: WidgetTree) {
        self.set_debug_text(new_widget.widget_name());
        self.page_switcher.switch_to(new_widget);
    }
    
    /// Get a mutable reference to the current widget
    pub fn current_mut(&mut self) -> &mut WidgetTree {
        self.page_switcher.current_mut()
    }
}

impl DynWidget for RootWidget {
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: frostsnap_embedded_widgets::Instant,
        is_release: bool,
    ) -> Option<frostsnap_embedded_widgets::KeyTouch> {
        // Forward touch to page_switcher
        self.page_switcher.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.page_switcher.handle_vertical_drag(prev_y, new_y, is_release)
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
        let mut cropped = target.translated(
            Point::new(20, 5),
        );
        self.debug_text.draw(&mut cropped, current_time)?;
        
        Ok(())
    }
}
