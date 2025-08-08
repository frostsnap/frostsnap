use crate::{Widget, Column, Text, ProgressBar, Frac, palette::PALETTE, FONT_MED, FONT_SMALL, FONT_LARGE, Center};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Size,
    pixelcolor::Rgb565,
    text::Alignment,
};
use u8g2_fonts::U8g2TextStyle;
use crate::column::MainAxisAlignment;
use alloc::format;

/// Widget for showing firmware upgrade progress
pub enum FirmwareUpgradeProgress {
    /// Actively erasing or downloading with progress
    Active {
        column: Column<(Text<U8g2TextStyle<Rgb565>>, ProgressBar, Text<U8g2TextStyle<Rgb565>>)>,
    },
    /// Passive state - just show text
    Passive {
        center: Center<Text<U8g2TextStyle<Rgb565>>>,
    },
}

impl FirmwareUpgradeProgress {
    /// Create a new firmware upgrade progress widget in erasing state
    pub fn erasing(progress: f32) -> Self {
        let title = Text::new(
            "Erasing...",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background)
        ).with_alignment(Alignment::Center);
        
        let mut progress_bar = ProgressBar::new();
        progress_bar.set_progress(Frac::from_ratio((progress * 100.0) as u32, 100));
        
        let percentage = Text::new(
            format!("{:02}%", (progress * 100.0) as u32),
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background)
        ).with_alignment(Alignment::Center);
        
        let column = Column::new((title, progress_bar, percentage))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(crate::column::CrossAxisAlignment::Center);
        
        Self::Active { column }
    }
    
    /// Create a new firmware upgrade progress widget in downloading state
    pub fn downloading(progress: f32) -> Self {
        let title = Text::new(
            "Downloading firmware...",
            U8g2TextStyle::new(FONT_MED, PALETTE.on_background)
        ).with_alignment(Alignment::Center);
        
        let mut progress_bar = ProgressBar::new();
        progress_bar.set_progress(Frac::from_ratio((progress * 100.0) as u32, 100));
        
        let percentage = Text::new(
            format!("{:02}%", (progress * 100.0) as u32),
            U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background)
        ).with_alignment(Alignment::Center);
        
        let column = Column::new((title, progress_bar, percentage))
            .with_main_axis_alignment(MainAxisAlignment::Center)
            .with_cross_axis_alignment(crate::column::CrossAxisAlignment::Center);
        
        Self::Active { column }
    }
    
    /// Create a new firmware upgrade progress widget in passive state
    pub fn passive() -> Self {
        // Show "Firmware Upgrade" text in passive state
        let text = Text::new(
            "Firmware\nUpgrade",
            U8g2TextStyle::new(FONT_LARGE, PALETTE.primary)
        ).with_alignment(Alignment::Center);
        let center = Center::new(text);
        
        Self::Passive { center }
    }
    
    /// Update the progress for active states
    pub fn update_progress(&mut self, progress: f32, status_text: &str) {
        if let Self::Active { column } = self {
            // Update the title text
            column.children.0 = Text::new(
                status_text,
                U8g2TextStyle::new(FONT_MED, PALETTE.on_background)
            ).with_alignment(Alignment::Center);
            
            // Update the progress bar
            column.children.1.set_progress(Frac::from_ratio((progress * 100.0) as u32, 100));
            
            // Update the percentage text
            column.children.2 = Text::new(
                format!("{:02}%", (progress * 100.0) as u32),
                U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background)
            ).with_alignment(Alignment::Center);
        }
    }
}

impl crate::DynWidget for FirmwareUpgradeProgress {
    fn handle_touch(
        &mut self,
        point: embedded_graphics::geometry::Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        match self {
            Self::Active { column } => column.handle_touch(point, current_time, is_release),
            Self::Passive { center } => center.handle_touch(point, current_time, is_release),
        }
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        match self {
            Self::Active { column } => column.handle_vertical_drag(prev_y, new_y, is_release),
            Self::Passive { center } => center.handle_vertical_drag(prev_y, new_y, is_release),
        }
    }
    
    fn size_hint(&self) -> Option<Size> {
        match self {
            Self::Active { column } => column.size_hint(),
            Self::Passive { center } => center.size_hint(),
        }
    }
    
    fn force_full_redraw(&mut self) {
        match self {
            Self::Active { column } => column.force_full_redraw(),
            Self::Passive { center } => center.force_full_redraw(),
        }
    }
}

impl Widget for FirmwareUpgradeProgress {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        match self {
            Self::Active { column } => column.draw(target, current_time),
            Self::Passive { center } => center.draw(target, current_time),
        }
    }
}
