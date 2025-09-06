use super::{Column, Row, Text as TextWidget};
use crate::{
    bitmap::EncodedImage, cursor::Cursor, image::Image, palette::PALETTE, prelude::*,
    vec_framebuffer::VecFramebuffer, Switcher,
};
use alloc::string::String;
use embedded_graphics::pixelcolor::{BinaryColor, Rgb565};
use embedded_graphics::text::renderer::TextRenderer;
use u8g2_fonts::U8g2TextStyle;

/// A widget for displaying device name with optional edit mode cursor

#[derive(frostsnap_macros::Widget)]
pub struct DeviceName {
    /// The device name text widget with optional cursor
    #[widget_delegate]
    text_widget: Container<Switcher<Row<(TextWidget<U8g2TextStyle<Rgb565>>, Option<Cursor>)>>>,
}

impl DeviceName {
    /// Create a new device name widget
    pub fn new<S: Into<String>>(name: S) -> Self {
        let name_string = name.into();
        let char_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.on_background);
        let text = TextWidget::new(name_string, char_style.clone());
        let row = Row::new((text, None::<Cursor>))
            .with_main_axis_alignment(crate::MainAxisAlignment::Center);
        let switcher = Switcher::new(row);
        let container = Container::new(switcher)
            .with_width(u32::MAX)
            .with_height(char_style.line_height());

        Self {
            text_widget: container,
        }
    }

    /// Get the current name
    pub fn name(&self) -> &str {
        self.text_widget.child.current().children.0.text()
    }

    /// Set a new device name
    pub fn set_name<S: Into<String>>(&mut self, name: S) {
        let name_string = name.into();
        let char_style = U8g2TextStyle::new(crate::FONT_LARGE, PALETTE.on_background);
        let text = TextWidget::new(name_string, char_style);
        let row = Row::new((text, None::<Cursor>))
            .with_main_axis_alignment(crate::MainAxisAlignment::Center)
            .with_cross_axis_alignment(crate::CrossAxisAlignment::End);
        self.text_widget.child.switch_to(row);
    }

    /// Enable the cursor for edit mode
    pub fn enable_cursor(&mut self) {
        let current_row = self.text_widget.child.current();
        if current_row.children.1.is_none() {
            // Get the current text
            let text_widget = current_row.children.0.clone();
            // Create a cursor at the end of the text
            let cursor = Cursor::new(embedded_graphics::prelude::Point::zero());
            let new_row = Row::new((text_widget, Some(cursor)))
                .with_main_axis_alignment(crate::MainAxisAlignment::Center)
                .with_cross_axis_alignment(crate::CrossAxisAlignment::End);
            self.text_widget.child.switch_to(new_row);
        }
    }

    /// Disable the cursor
    pub fn disable_cursor(&mut self) {
        let current_row = self.text_widget.child.current();
        if current_row.children.1.is_some() {
            // Get the current text
            let text_widget = current_row.children.0.clone();
            let new_row = Row::new((text_widget, None::<Cursor>))
                .with_main_axis_alignment(crate::MainAxisAlignment::Center)
                .with_cross_axis_alignment(crate::CrossAxisAlignment::End);
            self.text_widget.child.switch_to(new_row);
        }
    }

    /// Check if cursor is enabled
    pub fn is_cursor_enabled(&self) -> bool {
        self.text_widget.child.current().children.1.is_some()
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
