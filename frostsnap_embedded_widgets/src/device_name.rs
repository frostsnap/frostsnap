use super::{Widget, Text, Cursor};
use crate::{Instant, palette::PALETTE};
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, Rgb565},
};
use u8g2_fonts::U8g2TextStyle;

/// A widget for displaying device name with optional edit mode cursor
pub struct DeviceName {
    /// The device name text widget with color mapping
    text_widget: crate::color_map::ColorMap<Text<U8g2TextStyle<BinaryColor>>, Rgb565>,
    /// The cursor widget (used in edit mode)
    cursor: Option<Cursor>,
    /// The raw name string
    name: String,
    /// Whether we're in edit mode
    edit_mode: bool,
    /// Whether the widget needs redrawing
    needs_redraw: bool,
}

impl DeviceName {
    /// Create a new device name widget
    pub fn new<S: Into<String>>(name: S) -> Self {
        let name_string = name.into();
        let char_style = U8g2TextStyle::new(crate::FONT_MED, BinaryColor::On);
        
        let text = Text::new(name_string.clone(), char_style);
        let text_widget = text.color_map(|c| match c {
            BinaryColor::On => PALETTE.on_background,
            BinaryColor::Off => PALETTE.background,
        });
        
        Self {
            text_widget,
            cursor: None,
            name: name_string,
            edit_mode: false,
            needs_redraw: true,
        }
    }
    
    /// Set edit mode on/off
    pub fn set_edit_mode(&mut self, edit_mode: bool) {
        if self.edit_mode != edit_mode {
            self.edit_mode = edit_mode;
            if edit_mode {
                // Get the text size from the widget
                if let Some(text_size) = self.text_widget.size_hint() {
                    // Position cursor after the text
                    // Since text is centered at (120, 140), calculate where it ends
                    let text_start_x = 120 - (text_size.width as i32 / 2);
                    let cursor_x = text_start_x + text_size.width as i32;
                    let cursor_y = 140 - (text_size.height as i32 / 2);
                    self.cursor = Some(Cursor::new(Point::new(cursor_x, cursor_y)));
                }
            } else {
                self.cursor = None;
            }
            self.needs_redraw = true;
        }
    }
    
    /// Update the name
    pub fn set_name<S: Into<String>>(&mut self, name: S) {
        self.name = name.into();
        let char_style = U8g2TextStyle::new(crate::FONT_MED, BinaryColor::On);
        let text = Text::new(self.name.clone(), char_style);
        self.text_widget = text.color_map(|c| match c {
            BinaryColor::On => PALETTE.on_background,
            BinaryColor::Off => PALETTE.background,
        });
        
        if self.edit_mode {
            // Update cursor position based on new text size
            if let Some(text_size) = self.text_widget.size_hint() {
                let text_start_x = 120 - (text_size.width as i32 / 2);
                let cursor_x = text_start_x + text_size.width as i32;
                let cursor_y = 140 - (text_size.height as i32 / 2);
                if let Some(cursor) = &mut self.cursor {
                    cursor.set_position(Point::new(cursor_x, cursor_y));
                }
            }
        }
        self.needs_redraw = true;
    }
    
    /// Get the current name
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Widget for DeviceName {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Only redraw if needed
        if !self.needs_redraw {
            // Still need to update cursor if in edit mode
            if let Some(cursor) = &mut self.cursor {
                cursor.draw(target, current_time)?;
            }
            return Ok(());
        }
        
        // Clear the screen
        target.clear(PALETTE.background)?;
        
        // Draw the text widget
        self.text_widget.draw(target, current_time)?;
        
        // Draw cursor if in edit mode
        if let Some(cursor) = &mut self.cursor {
            cursor.draw(target, current_time)?;
        }
        
        self.needs_redraw = false;
        Ok(())
    }
    
    fn handle_touch(&mut self, _point: Point, _current_time: Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}
    
    fn size_hint(&self) -> Option<Size> {
        self.text_widget.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
        self.text_widget.force_full_redraw();
    }
}