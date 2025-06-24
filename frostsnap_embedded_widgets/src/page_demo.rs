use crate::{Widget, PageByPage};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::Gray2,
    prelude::*,
    text::{Alignment, Text as EgText},
    mono_font::{ascii::FONT_10X20, ascii::FONT_8X13, MonoTextStyle},
};
use alloc::format;

pub struct PageDemo {
    current_page: usize,
    total_pages: usize,
    size: Size,
}

impl PageDemo {
    pub fn new(size: Size) -> Self {
        Self {
            current_page: 0,
            total_pages: 5,
            size,
        }
    }
}

impl Widget for PageDemo {
    type Color = Gray2;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Clear the framebuffer
        let _ = target.clear(Gray2::new(0));
        
        // Draw page content
        let center = Point::new(self.size.width as i32 / 2, self.size.height as i32 / 2);
        
        // Page title
        let title = match self.current_page {
            0 => "Page 1",
            1 => "Page 2", 
            2 => "Page 3",
            3 => "Page 4",
            4 => "Page 5",
            _ => "Unknown",
        };
        
        let title_style = MonoTextStyle::new(&FONT_10X20, Gray2::new(3));
        let _ = EgText::with_alignment(
            title,
            center - Point::new(0, 40),
            title_style,
            Alignment::Center,
        )
        .draw(target);
        
        // Page content
        let content = match self.current_page {
            0 => "Swipe up or down",
            1 => "This is page 2",
            2 => "Middle page",
            3 => "Almost there!",
            4 => "Final page",
            _ => "",
        };
        
        let content_style = MonoTextStyle::new(&FONT_8X13, Gray2::new(2));
        let _ = EgText::with_alignment(
            content,
            center + Point::new(0, 20),
            content_style,
            Alignment::Center,
        )
        .draw(target);
        
        // Page indicator
        let indicator = format!("{}/{}", self.current_page + 1, self.total_pages);
        let indicator_style = MonoTextStyle::new(&FONT_8X13, Gray2::new(1));
        let _ = EgText::with_alignment(
            &indicator,
            Point::new(center.x, self.size.height as i32 - 20),
            indicator_style,
            Alignment::Center,
        )
        .draw(target);
        
        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        _point: Point,
        _current_time: crate::Instant,
        _is_release: bool,
    ) -> Option<crate::KeyTouch> {
        None
    }
    
    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}

impl PageByPage for PageDemo {
    fn has_next_page(&self) -> bool {
        self.current_page < self.total_pages - 1
    }
    
    fn has_prev_page(&self) -> bool {
        self.current_page > 0
    }
    
    fn next_page(&mut self) {
        if self.has_next_page() {
            self.current_page += 1;
        }
    }
    
    fn prev_page(&mut self) {
        if self.has_prev_page() {
            self.current_page -= 1;
        }
    }
    
    fn current_page(&self) -> usize {
        self.current_page
    }
    
    fn total_pages(&self) -> usize {
        self.total_pages
    }
}