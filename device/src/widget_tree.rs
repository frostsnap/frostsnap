use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::*,
};
use frostsnap_embedded_widgets::{
    Widget, Welcome,
};
use crate::ui::UiEvent;

/// The widget tree represents the current UI state as a tree of widgets
pub enum WidgetTree {
    /// Default welcome screen
    Welcome(Welcome),
    // TODO: Add more widget variants for each UI state
}

impl Default for WidgetTree {
    fn default() -> Self {
        WidgetTree::Welcome(Welcome::new())
    }
}

impl WidgetTree {
    /// Handle touch input and return any resulting UI event
    pub fn handle_touch(&mut self, _point: Point, _current_time: frostsnap_embedded_widgets::Instant, _is_release: bool) -> Option<UiEvent> {
        // TODO: Implement touch handling
        None
    }
    
    /// Handle vertical drag events
    pub fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {
        // TODO: Implement drag handling
    }
    
    /// Force a full redraw of the current widget
    pub fn force_redraw(&mut self) {
        match self {
            WidgetTree::Welcome(widget) => widget.force_full_redraw(),
        }
    }
}

impl Widget for WidgetTree {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: frostsnap_embedded_widgets::Instant,
    ) -> Result<(), D::Error> {
        // Draw the appropriate widget
        match self {
            WidgetTree::Welcome(widget) => widget.draw(target, current_time)?,
        }
        
        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: frostsnap_embedded_widgets::Instant,
        is_release: bool,
    ) -> Option<frostsnap_embedded_widgets::KeyTouch> {
        // WidgetTree handles touch through its own method that returns UiEvent
        self.handle_touch(point, current_time, is_release);
        None
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        // Delegate to WidgetTree's own method
        self.handle_vertical_drag(prev_y, new_y, is_release);
    }
    
    fn size_hint(&self) -> Option<Size> {
        // Full screen for all widgets
        Some(Size::new(240, 280))
    }
    
    fn force_full_redraw(&mut self) {
        self.force_redraw();
    }
}