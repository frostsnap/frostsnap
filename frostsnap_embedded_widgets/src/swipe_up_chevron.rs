use crate::{Widget, FONT_SMALL};
use embedded_graphics::{
    pixelcolor::PixelColor,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
    image::Image,
};
use u8g2_fonts::U8g2TextStyle;
use embedded_iconoir::{size24px::navigation::{NavArrowUp, NavArrowDown}, prelude::IconoirNewIcon};

#[derive(PartialEq)]
pub enum SwipeDirection {
    Up,
    Down,
}

pub struct SwipeUpChevron<C: PixelColor> {
    size: Size,
    color: C,
    direction: Option<SwipeDirection>,
    needs_redraw: bool,
}

impl<C: PixelColor> SwipeUpChevron<C> {
    pub fn new(size: Size, color: C) -> Self {
        Self { 
            size, 
            color,
            direction: Some(SwipeDirection::Up),
            needs_redraw: true,
        }
    }
    
    pub fn set_direction(&mut self, direction: Option<SwipeDirection>) {
        if self.direction != direction {
            self.direction = direction;
            self.needs_redraw = true;
        }
    }
}

impl<C: PixelColor + Default> Widget for SwipeUpChevron<C> {
    type Color = C;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if !self.needs_redraw {
            return Ok(());
        }
        
        if let Some(direction) = &self.direction {
            let center_x = (self.size.width / 2) as i32;
            let text_y = self.size.height as i32 - 10;
            
            match direction {
                SwipeDirection::Up => {
                    // Draw text at very bottom
                    Text::with_text_style(
                        "Swipe up",
                        Point::new(center_x, text_y),
                        U8g2TextStyle::new(FONT_SMALL, self.color),
                        TextStyleBuilder::new()
                            .alignment(Alignment::Center)
                            .baseline(Baseline::Middle)
                            .build(),
                    )
                    .draw(target)?;
                    
                    // Draw chevron above text
                    let chevron_up = NavArrowUp::new(self.color);
                    let chevron_size = chevron_up.size();
                    let chevron_point = Point::new(
                        center_x - chevron_size.width as i32 / 2,
                        text_y - 15 - chevron_size.height as i32 / 2,  // Reduced from 25 to 15
                    );
                    Image::new(&chevron_up, chevron_point).draw(target)?;
                }
                SwipeDirection::Down => {
                    // Draw text at very bottom
                    Text::with_text_style(
                        "Swipe down",
                        Point::new(center_x, text_y),
                        U8g2TextStyle::new(FONT_SMALL, self.color),
                        TextStyleBuilder::new()
                            .alignment(Alignment::Center)
                            .baseline(Baseline::Middle)
                            .build(),
                    )
                    .draw(target)?;
                    
                    // Draw chevron above text
                    let chevron_down = NavArrowDown::new(self.color);
                    let chevron_size = chevron_down.size();
                    let chevron_point = Point::new(
                        center_x - chevron_size.width as i32 / 2,
                        text_y - 15 - chevron_size.height as i32 / 2,  // Reduced from 25 to 15
                    );
                    Image::new(&chevron_down, chevron_point).draw(target)?;
                }
            }
        }
        
        self.needs_redraw = false;
        Ok(())
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
    
    fn force_full_redraw(&mut self) {
        self.needs_redraw = true;
    }
}
