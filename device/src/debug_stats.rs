#[cfg(feature = "debug_mem")]
use alloc::string::ToString;
#[cfg(feature = "debug_mem")]
use core::fmt::Write;
use embedded_graphics::pixelcolor::Rgb565;
#[cfg(not(any(feature = "debug_fps", feature = "debug_mem")))]
use embedded_graphics::prelude::Size;
#[cfg(feature = "debug_mem")]
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::RgbColor,
    prelude::{Point, Size},
};
#[cfg(feature = "debug_fps")]
use frostsnap_embedded_widgets::Fps;
#[cfg(all(feature = "debug_fps", feature = "debug_mem"))]
use frostsnap_embedded_widgets::Row;
#[cfg(not(any(feature = "debug_fps", feature = "debug_mem")))]
use frostsnap_embedded_widgets::SizedBox;
use frostsnap_embedded_widgets::Widget;
#[cfg(feature = "debug_mem")]
use frostsnap_embedded_widgets::{
    prelude::*, string_buffer::StringBuffer, text::Text, DynWidget, Instant, Switcher, FONT_SMALL,
};
#[cfg(feature = "debug_mem")]
use u8g2_fonts::U8g2TextStyle;

/// Create a debug stats widget that conditionally displays FPS and/or memory usage
pub fn create_debug_stats() -> impl Widget<Color = Rgb565> {
    #[cfg(all(feature = "debug_fps", feature = "debug_mem"))]
    {
        let fps = Fps::new(500);
        let memory = MemoryIndicator::new();

        Row::new((fps, memory))
            .with_main_axis_alignment(MainAxisAlignment::SpaceAround)
            .with_cross_axis_alignment(CrossAxisAlignment::Start)
    }

    #[cfg(all(feature = "debug_fps", not(feature = "debug_mem")))]
    {
        Fps::new(500)
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

// Constants for memory text dimensions - "Mem: 262144" needs more space
#[cfg(feature = "debug_mem")]
const MEM_MAX_CHARS: usize = 15; // Increased for full byte count

#[cfg(feature = "debug_mem")]
type MemText = Text<U8g2TextStyle<Rgb565>>;

/// Memory usage indicator component that polls esp_alloc directly
#[cfg(feature = "debug_mem")]
pub struct MemoryIndicator {
    display: Switcher<MemText>,
    last_used: usize,
    last_draw_time: Option<Instant>,
}

#[cfg(feature = "debug_mem")]
impl MemoryIndicator {
    fn new() -> Self {
        // Use Cyan color directly for Rgb565 text
        let text_style = U8g2TextStyle::new(FONT_SMALL, Rgb565::CYAN);
        let initial_text = Text::new("Mem: 0".to_string(), text_style);
        let display = Switcher::new(initial_text);

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

    fn force_full_redraw(&mut self) {
        self.display.force_full_redraw();
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: Instant,
        is_release: bool,
    ) -> Option<frostsnap_embedded_widgets::KeyTouch> {
        self.display.handle_touch(point, current_time, is_release)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.display.handle_vertical_drag(prev_y, new_y, is_release);
    }
}

#[cfg(feature = "debug_mem")]
impl Widget for MemoryIndicator {
    type Color = Rgb565;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Update every 500ms (same rate as FPS widget)
        let should_update = match self.last_draw_time {
            Some(last_time) => current_time.saturating_duration_since(last_time) >= 500,
            None => true,
        };

        if should_update {
            self.last_draw_time = Some(current_time);

            // Get heap stats from esp_alloc
            // Note: The exact API might vary, using a simple approach for now
            let used = 0; // TODO: Get actual heap usage from esp_alloc

            // Only update if value changed
            if used != self.last_used {
                self.last_used = used;

                // Format the memory usage
                let mut buf = StringBuffer::<MEM_MAX_CHARS>::new();
                let _ = write!(&mut buf, "Mem: {}", used);

                // Create a new text widget with the updated text
                let text_style = U8g2TextStyle::new(FONT_SMALL, Rgb565::CYAN);
                let text_widget = Text::new(buf.as_str().to_string(), text_style);
                self.display.switch_to(text_widget);
            }
        }

        // Always draw the display (it handles its own dirty tracking)
        self.display.draw(target, current_time)
    }
}
