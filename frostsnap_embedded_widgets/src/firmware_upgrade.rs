use crate::{HoldToConfirm, Widget, Text, Column, SizedBox, Container, Padding, palette::PALETTE, FONT_SMALL, FONT_MED};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Size,
    pixelcolor::Rgb565,
    text::Alignment,
    primitives::PrimitiveStyleBuilder,
};
use u8g2_fonts::{U8g2TextStyle, fonts};
use alloc::format;

// Use small font (17px) for the hash
const HASH_FONT: fonts::u8g2_font_profont17_mf = fonts::u8g2_font_profont17_mf;

/// Hold to confirm widget for firmware upgrades
/// Displays the firmware hash and size
pub struct FirmwareUpgradeConfirm {
    hold_to_confirm: HoldToConfirm<Column<(
        Text<U8g2TextStyle<Rgb565>>,
        SizedBox<Rgb565>,
        Container<Padding<Column<(
            Text<U8g2TextStyle<Rgb565>>,
            Text<U8g2TextStyle<Rgb565>>,
            Text<U8g2TextStyle<Rgb565>>,
            Text<U8g2TextStyle<Rgb565>>
        )>>>,
        SizedBox<Rgb565>,
        Text<U8g2TextStyle<Rgb565>>
    )>>,
}

impl FirmwareUpgradeConfirm {
    pub fn new(screen_size: Size, firmware_digest: [u8; 32], size_bytes: u32) -> Self {
        // Format the full hash as 4 lines of 16 hex chars each
        let hash_line1 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[0], firmware_digest[1], firmware_digest[2], firmware_digest[3],
            firmware_digest[4], firmware_digest[5], firmware_digest[6], firmware_digest[7]
        );
        let hash_line2 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[8], firmware_digest[9], firmware_digest[10], firmware_digest[11],
            firmware_digest[12], firmware_digest[13], firmware_digest[14], firmware_digest[15]
        );
        let hash_line3 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[16], firmware_digest[17], firmware_digest[18], firmware_digest[19],
            firmware_digest[20], firmware_digest[21], firmware_digest[22], firmware_digest[23]
        );
        let hash_line4 = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            firmware_digest[24], firmware_digest[25], firmware_digest[26], firmware_digest[27],
            firmware_digest[28], firmware_digest[29], firmware_digest[30], firmware_digest[31]
        );
        
        // Format size in KB or MB
        let size_text = if size_bytes < 1024 * 1024 {
            format!("{} KB", size_bytes / 1024)
        } else {
            format!("{:.1} MB", size_bytes as f32 / (1024.0 * 1024.0))
        };
        
        // Create the content with title, hash lines, and size
        let title = Text::new(
            "Upgrade firmware?",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background)
        ).with_alignment(Alignment::Center);
        
        let hash1 = Text::new(
            hash_line1,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface)
        ).with_alignment(Alignment::Center);
        
        let hash2 = Text::new(
            hash_line2,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface)
        ).with_alignment(Alignment::Center);
        
        let hash3 = Text::new(
            hash_line3,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface)
        ).with_alignment(Alignment::Center);
        
        let hash4 = Text::new(
            hash_line4,
            U8g2TextStyle::new(HASH_FONT, PALETTE.on_surface)
        ).with_alignment(Alignment::Center);
        
        let size = Text::new(
            size_text,
            U8g2TextStyle::new(FONT_SMALL, PALETTE.on_surface_variant)
        ).with_alignment(Alignment::Center);
        
        // Put just the hash lines in a container with rounded border, fill, and padding
        let hash_column = Column::new((hash1, hash2, hash3, hash4));
        let hash_with_padding = Padding::all(5, hash_column);
        let hash_container = Container::new(hash_with_padding)
            .with_border(PrimitiveStyleBuilder::new()
                .stroke_color(PALETTE.outline)
                .stroke_width(2)
                .fill_color(PALETTE.surface)
                .build())
            .with_corner_radius(Size::new(10, 10));
        
        // Create main column with title, container, and size
        let spacer1 = SizedBox::<Rgb565>::new(Size::new(1, 8));
        let spacer2 = SizedBox::<Rgb565>::new(Size::new(1, 8));
        let content = Column::new((title, spacer1, hash_container, spacer2, size));
        
        // Create hold to confirm with 3 second hold time
        let hold_to_confirm = HoldToConfirm::new(screen_size, 3000, content);
        
        Self {
            hold_to_confirm,
        }
    }
    
    /// Check if the confirmation is complete
    pub fn is_confirmed(&self) -> bool {
        self.hold_to_confirm.is_completed()
    }
}

impl crate::DynWidget for FirmwareUpgradeConfirm {
    fn handle_touch(
        &mut self,
        point: embedded_graphics::geometry::Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        self.hold_to_confirm.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.hold_to_confirm.handle_vertical_drag(prev_y, new_y, is_release)
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.hold_to_confirm.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.hold_to_confirm.force_full_redraw()
    }
}

impl Widget for FirmwareUpgradeConfirm {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        self.hold_to_confirm.draw(target, current_time)
    }
}