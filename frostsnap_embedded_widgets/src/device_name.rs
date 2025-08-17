use super::{Column, MutText, Text as TextWidget, Widget}; // , Cursor};
use crate::super_draw_target::SuperDrawTarget;
use crate::{
    bitmap::EncodedImage, image::Image, palette::PALETTE, vec_framebuffer::VecFramebuffer, Instant,
};
use alloc::string::String;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::{BinaryColor, Rgb565},
};
use u8g2_fonts::U8g2TextStyle;

// Constants for MutText buffer
const MAX_NAME_CHARS: usize = 16; // Capped at 16 characters

type MutTextWidget = MutText<U8g2TextStyle<Rgb565>>;

/// A widget for displaying device name with optional edit mode cursor
pub struct DeviceName {
    /// The device name text widget
    text_widget: MutTextWidget,
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
        let char_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.primary);

        let text_widget = MutText::new(&name_string, char_style, MAX_NAME_CHARS);

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
        self.text_widget.text()
    }

    /// Set a new device name
    pub fn set_name<S: Into<String>>(&mut self, name: S) {
        let name_string = name.into();
        let char_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.primary);
        let text_widget = TextWidget::new(&name_string, char_style);
        self.text_widget.set_text(text_widget);
        self.needs_redraw = true;
    }
}

impl crate::DynWidget for DeviceName {
    fn set_constraints(&mut self, max_size: Size) {
        self.text_widget.set_constraints(max_size);
    }

    fn sizing(&self) -> crate::Sizing {
        self.text_widget.sizing()
    }

    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }

    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, _new_y: u32, _is_release: bool) {}

    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
        self.text_widget.force_full_redraw();
    }
}

impl Widget for DeviceName {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
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
#[derive(frostsnap_macros::Widget)]
pub struct DeviceNameScreen {
    #[widget_delegate]
    column: Column<(Image<VecFramebuffer<BinaryColor>, Rgb565>, DeviceName)>,
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
        let encoded_image = EncodedImage::from_bytes(LOGO_DATA).expect("Failed to load logo");
        let framebuffer: VecFramebuffer<BinaryColor> = encoded_image.into();
        let logo_colored = Image::with_color_map(framebuffer, |c| match c {
            BinaryColor::On => PALETTE.logo,
            BinaryColor::Off => PALETTE.background,
        });

        // Create DeviceName widget
        let device_name_widget = DeviceName::new(device_name);

        // Create the column with main axis alignment for spacing
        let column = Column::new((logo_colored, device_name_widget))
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

// All trait implementations are now generated by the derive macro
