use super::{Widget, Column, MutText}; // , Cursor};
use crate::{Instant, palette::PALETTE, bitmap::{EncodedImage, BitmapWidget}, color_map::ColorMap};
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, Rgb565},
};
use u8g2_fonts::U8g2TextStyle;

// Constants for MutText buffer
const MAX_NAME_CHARS: usize = 16;  // Capped at 16 characters
const NAME_WIDTH: usize = 220;  // Reasonable width for 16 chars with larger font
const NAME_HEIGHT: usize = 40;  // Increased for larger font
// Calculate buffer size: BinaryColor uses 1 bit per pixel, so W*H/8 bytes
// Add some extra bytes for alignment
const NAME_BUFFER_SIZE: usize = ((NAME_WIDTH * NAME_HEIGHT + 7) / 8) + 100;

type MutTextWidget = MutText<U8g2TextStyle<BinaryColor>, MAX_NAME_CHARS, NAME_WIDTH, NAME_HEIGHT, NAME_BUFFER_SIZE>;

/// A widget for displaying device name with optional edit mode cursor
pub struct DeviceName {
    /// The device name text widget (mutable text with color mapping)
    text_widget: ColorMap<MutTextWidget, Rgb565>,
    /// The cursor widget (used in edit mode)
    // cursor: Option<Cursor>,
    /// Whether we're in edit mode
    edit_mode: bool,
    /// Whether the widget needs redrawing
    needs_redraw: bool,
}

impl DeviceName {
    /// Create a new device name widget
    pub fn new<S: Into<String>>(name: S) -> Self {
        let name_string = name.into();
        let char_style = U8g2TextStyle::new(crate::FONT_LARGE, BinaryColor::On);
        
        let mut_text = MutText::new(&name_string, char_style);
        let text_widget = mut_text.color_map(|c| match c {
            BinaryColor::On => PALETTE.primary,
            BinaryColor::Off => PALETTE.background,
        });
        
        Self {
            text_widget,
            // cursor: None,
            edit_mode: false,
            needs_redraw: true,
        }
    }
    
    /// Set edit mode on/off
    pub fn set_edit_mode(&mut self, edit_mode: bool) {
        if self.edit_mode != edit_mode {
            self.edit_mode = edit_mode;
            // if edit_mode {
            //     // Get the text size from the widget
            //     if let Some(text_size) = self.text_widget.size_hint() {
            //         // Position cursor after the text
            //         // Since text is centered at (120, 140), calculate where it ends
            //         let text_start_x = 120 - (text_size.width as i32 / 2);
            //         let cursor_x = text_start_x + text_size.width as i32;
            //         let cursor_y = 140 - (text_size.height as i32 / 2);
            //         self.cursor = Some(Cursor::new(Point::new(cursor_x, cursor_y)));
            //     }
            // } else {
            //     self.cursor = None;
            // }
            self.needs_redraw = true;
        }
    }
    
    
    /// Get the current name
    pub fn name(&self) -> &str {
        self.text_widget.child.text()
    }
    
    /// Set a new device name
    pub fn set_name<S: Into<String>>(&mut self, name: S) {
        let name_string = name.into();
        self.text_widget.child.set_text(&name_string);
        self.needs_redraw = true;
    }
}

impl crate::DynWidget for DeviceName {
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

impl Widget for DeviceName {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Only redraw if needed
        if !self.needs_redraw {
            // // Still need to update cursor if in edit mode
            // if let Some(cursor) = &mut self.cursor {
            //     cursor.draw(target, current_time)?;
            // }
            return Ok(());
        }
        
        // Draw the text widget
        self.text_widget.draw(target, current_time)?;
        
        // // Draw cursor if in edit mode
        // if let Some(cursor) = &mut self.cursor {
        //     cursor.draw(target, current_time)?;
        // }
        
        self.needs_redraw = false;
        Ok(())
    }
}

const LOGO_DATA: &[u8] = include_bytes!("../assets/frostsnap-logo-96x96.bin");

/// A screen showing the Frostsnap logo and the DeviceName widget
pub struct DeviceNameScreen {
    column: Column<(
        ColorMap<BitmapWidget, Rgb565>,
        DeviceName,
    )>,
}

impl DeviceNameScreen {
    /// Get a reference to the inner DeviceName widget
    fn device_name_widget(&self) -> &DeviceName {
        &self.column.children.1
    }
    
    /// Get a mutable reference to the inner DeviceName widget
    fn device_name_widget_mut(&mut self) -> &mut DeviceName {
        &mut self.column.children.1
    }
    
    pub fn new(device_name: String) -> Self {
        // Load logo
        let image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let bitmap_widget = BitmapWidget::new(image.into());
        let logo_colored = bitmap_widget.color_map(|c| match c {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });
        
        // Create DeviceName widget
        let device_name_widget = DeviceName::new(device_name);
        
        // Create the column with main axis alignment for spacing
        let column = Column::new((
            logo_colored,
            device_name_widget,
        ))
        .with_main_axis_alignment(crate::MainAxisAlignment::SpaceEvenly);
        
        Self { column }
    }
    
    /// Set edit mode for the device name widget
    pub fn set_edit_mode(&mut self, edit_mode: bool) {
        self.device_name_widget_mut().set_edit_mode(edit_mode);
    }
    
    /// Get the current device name
    pub fn name(&self) -> &str {
        self.device_name_widget().name()
    }
    
    /// Set a new device name
    pub fn set_name<S: Into<String>>(&mut self, name: S) {
        self.device_name_widget_mut().set_name(name);
    }
}

impl crate::DynWidget for DeviceNameScreen {
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.column.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.column.handle_vertical_drag(prev_y, new_y, is_release)
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.column.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.column.force_full_redraw()
    }
}

impl Widget for DeviceNameScreen {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.column.draw(target, current_time)
    }
}
