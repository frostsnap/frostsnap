use frostsnap_embedded_widgets::Widget;
#[cfg(feature = "debug_fps")]
use frostsnap_embedded_widgets::Fps;
#[cfg(all(feature = "debug_fps", feature = "debug_mem"))]
use frostsnap_embedded_widgets::Row;
#[cfg(not(any(feature = "debug_fps", feature = "debug_mem")))]
use frostsnap_embedded_widgets::SizedBox;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::geometry::Size;
#[cfg(feature = "debug_mem")]
use frostsnap_embedded_widgets::{DynWidget, Instant, Text, FONT_SMALL, Switcher};
#[cfg(feature = "debug_mem")]
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::RgbColor,
    text::Alignment,
};
#[cfg(feature = "debug_mem")]
use alloc::format;
#[cfg(feature = "debug_mem")]
use u8g2_fonts::U8g2TextStyle;

/// Create a status widget that conditionally displays FPS and/or memory usage
pub fn create_status() -> impl Widget<Color = Rgb565> {
    #[cfg(all(feature = "debug_fps", feature = "debug_mem"))]
    {
        let fps = Fps::new();
        let memory = MemoryIndicator::new();
        Row::new((fps, memory))
            .with_cross_axis_alignment(frostsnap_embedded_widgets::row::CrossAxisAlignment::Start)
    }
    
    #[cfg(all(feature = "debug_fps", not(feature = "debug_mem")))]
    {
        Fps::new()
    }
    
    #[cfg(all(not(feature = "debug_fps"), feature = "debug_mem"))]
    {
        MemoryIndicator::new()
    }
    
    #[cfg(all(not(feature = "debug_fps"), not(feature = "debug_mem")))]
    {
        // Empty widget when no debugging enabled
        SizedBox::<Rgb565>::new(Size::new(0, 0))
    }
}

/// Memory usage indicator component that polls esp_alloc directly
#[cfg(feature = "debug_mem")]
pub struct MemoryIndicator {
    display: Switcher<Text<U8g2TextStyle<Rgb565>>>,
    text_style: U8g2TextStyle<Rgb565>,
    last_percentage: usize,
    last_draw_time: Option<Instant>,
}

#[cfg(feature = "debug_mem")]
impl MemoryIndicator {
    fn new() -> Self {
        let text_style = U8g2TextStyle::new(FONT_SMALL, Rgb565::CYAN);
        let initial_text = Text::new("Mem: 0%", text_style.clone()).with_alignment(Alignment::Right);
        // Use Switcher for instant switching
        let display = Switcher::new(initial_text);
        
        Self {
            display,
            text_style,
            last_percentage: 0,
            last_draw_time: None,
        }
    }
}

#[cfg(feature = "debug_mem")]
impl DynWidget for MemoryIndicator {
    fn size_hint(&self) -> Option<Size> {
        self.display.size_hint()
    }
    
    fn force_full_redraw(&mut self) {
        self.display.force_full_redraw();
    }
}

#[cfg(feature = "debug_mem")]
impl Widget for MemoryIndicator {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Update every 500ms (same rate as FPS widget)
        let should_update = match self.last_draw_time {
            Some(last_time) => current_time.saturating_duration_since(last_time) >= 500,
            None => true,
        };
        
        if should_update {
            // Get memory info from esp_alloc
            let used = esp_alloc::HEAP.used();
            let free = esp_alloc::HEAP.free();
            let total = used + free;
            
            let percentage = if total > 0 {
                (used * 100) / total
            } else {
                0
            };
            
            // Update text if percentage changed
            if percentage != self.last_percentage {
                self.last_percentage = percentage;
                let used_kb = used / 1024;
                
                let new_text = Text::new(
                    format!("{}% {}k", percentage, used_kb),
                    self.text_style.clone()
                ).with_alignment(Alignment::Right);
                
                // Use switch_to to update the display with the new text
                self.display.switch_to(new_text);
            }
            
            self.last_draw_time = Some(current_time);
        }
        
        // Draw the display
        self.display.draw(target, current_time)
    }
}