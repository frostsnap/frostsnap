use frostsnap_embedded_widgets::Widget;
#[cfg(feature = "debug_fps")]
use frostsnap_embedded_widgets::Fps;
#[cfg(all(feature = "debug_fps", feature = "debug_mem"))]
use frostsnap_embedded_widgets::{Row, Container};
#[cfg(not(any(feature = "debug_fps", feature = "debug_mem")))]
use frostsnap_embedded_widgets::SizedBox;
use embedded_graphics::pixelcolor::Rgb565;
#[cfg(not(any(feature = "debug_fps", feature = "debug_mem")))]
use embedded_graphics::prelude::Size;
#[cfg(feature = "debug_mem")]
use frostsnap_embedded_widgets::{
    prelude::*,
    DynWidget, Instant, FONT_SMALL, 
    mut_text::{MutText, mut_text_buffer_size},
    palette::PALETTE,
    color_map::ColorMap,
    string_buffer::StringBuffer,
};
#[cfg(feature = "debug_mem")]
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{RgbColor, BinaryColor},
    text::Alignment,
};
#[cfg(feature = "debug_mem")]
use u8g2_fonts::U8g2TextStyle;

/// Create a debug stats widget that conditionally displays FPS and/or memory usage
pub fn create_debug_stats() -> impl Widget<Color = Rgb565> {
    #[cfg(all(feature = "debug_fps", feature = "debug_mem"))]
    {
        let fps = Fps::new();
        let memory = MemoryIndicator::new();
        
        Row::new((fps, memory))
            .with_main_axis_alignment(MainAxisAlignment::SpaceAround)
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
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

// Constants for MutText dimensions - "Mem: 262144" needs more space
#[cfg(feature = "debug_mem")]
const MEM_MAX_CHARS: usize = 15;  // Increased for full byte count
#[cfg(feature = "debug_mem")]
// ProFont17 is exactly 10px wide per character for ASCII
// "Mem: 262144" = 11 chars = 110px wide
const MEM_WIDTH: usize = 110;  // Exact width for "Mem: 262144"
#[cfg(feature = "debug_mem")]
const MEM_HEIGHT: usize = 17;  // ProFont17 is exactly 17px tall
#[cfg(feature = "debug_mem")]
const MEM_BUFFER_SIZE: usize = mut_text_buffer_size::<MEM_WIDTH, MEM_HEIGHT>();

#[cfg(feature = "debug_mem")]
type MemMutText = MutText<U8g2TextStyle<BinaryColor>, MEM_MAX_CHARS, MEM_WIDTH, MEM_HEIGHT, MEM_BUFFER_SIZE>;

/// Memory usage indicator component that polls esp_alloc directly
#[cfg(feature = "debug_mem")]
pub struct MemoryIndicator {
    display: ColorMap<MemMutText, Rgb565>,
    last_used: usize,
    last_draw_time: Option<Instant>,
}

#[cfg(feature = "debug_mem")]
impl MemoryIndicator {
    fn new() -> Self {
        let text_style = U8g2TextStyle::new(FONT_SMALL, BinaryColor::On);
        let mut_text = MutText::new("Mem: 0", text_style).with_alignment(Alignment::Left);
        
        let display = mut_text.color_map(|c| match c {
            BinaryColor::On => Rgb565::CYAN,
            BinaryColor::Off => PALETTE.background,
        });
        
        Self {
            display,
            last_used: 0,
            last_draw_time: None,
        }
    }
}

#[cfg(feature = "debug_mem")]
impl DynWidget for MemoryIndicator {
    fn set_constraints(&mut self, max_size: Size) {
        self.display.set_constraints(max_size);
    }
    
    fn sizing(&self) -> frostsnap_embedded_widgets::Sizing {
        self.display.sizing()
    }
    
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
            
            // Always update the text to force a redraw
            self.last_used = used;
            
            use core::fmt::Write;
            let mut buf = StringBuffer::<MEM_MAX_CHARS>::new();
            write!(&mut buf, "Mem: {}", used).ok();
            self.display.child.set_text(buf.as_str());
            
            // Force a redraw even if the text hasn't changed
            self.display.force_full_redraw();
            
            self.last_draw_time = Some(current_time);
        }
        
        // Draw the display
        self.display.draw(target, current_time)
    }
}
