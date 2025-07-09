use crate::graphics::palette::PALETTE;
use crate::graphics::widgets::{icons, Key, KeyTouch, FONT_SMALL};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use embedded_iconoir::size32px::navigation::{NavArrowLeft, NavArrowRight};
use u8g2_fonts::U8g2TextStyle;

const BUTTON_SIZE: u32 = 60;

#[derive(Debug)]
pub struct NavigationButtons {
    size: Size,
    prev_button_rect: Rectangle,
    next_button_rect: Rectangle,
    current_page: usize,
    total_pages: usize,
    needs_redraw: bool,
}

impl NavigationButtons {
    pub fn new(size: Size, current_page: usize, total_pages: usize) -> Self {
        // Calculate button positions relative to origin
        let button_y = 5;
        let button_spacing = 120;
        let center_x = size.width as i32 / 2;

        let prev_button_rect = Rectangle::new(
            Point::new(
                center_x - button_spacing / 2 - BUTTON_SIZE as i32 / 2,
                button_y,
            ),
            Size::new(BUTTON_SIZE, BUTTON_SIZE),
        );
        let next_button_rect = Rectangle::new(
            Point::new(
                center_x + button_spacing / 2 - BUTTON_SIZE as i32 / 2,
                button_y,
            ),
            Size::new(BUTTON_SIZE, BUTTON_SIZE),
        );

        Self {
            size,
            prev_button_rect,
            next_button_rect,
            current_page,
            total_pages,
            needs_redraw: true,
        }
    }

    pub fn set_current_page(&mut self, page: usize) {
        if self.current_page != page {
            self.current_page = page;
            self.needs_redraw = true;
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        _current_time: crate::Instant,
    ) {
        if !self.needs_redraw {
            return;
        }

        let _ = target.fill_solid(&target.bounding_box(), PALETTE.background);

        // Draw previous button (only if not at first page)
        if self.current_page > 0 {
            icons::Icon::<NavArrowLeft>::default()
                .with_color(PALETTE.primary_container)
                .with_center(self.prev_button_rect.center())
                .draw(target);
        }

        // Draw next button (only if not on last page)
        if self.current_page < self.total_pages - 1 {
            icons::Icon::<NavArrowRight>::default()
                .with_color(PALETTE.primary_container)
                .with_center(self.next_button_rect.center())
                .draw(target);
        }

        // Draw page counter between buttons
        let counter_text = format!("{}/{}", self.current_page + 1, self.total_pages);
        let counter_position =
            Point::new(self.size.width as i32 / 2, self.prev_button_rect.center().y);

        let _ = Text::with_text_style(
            &counter_text,
            counter_position,
            U8g2TextStyle::new(FONT_SMALL, PALETTE.on_background),
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(target);

        self.needs_redraw = false;
    }

    pub fn handle_touch(&self, point: Point) -> Option<KeyTouch> {
        if self.current_page > 0 && self.prev_button_rect.contains(point) {
            Some(KeyTouch::new(Key::NavBack, self.prev_button_rect))
        } else if self.current_page < self.total_pages - 1 && self.next_button_rect.contains(point)
        {
            Some(KeyTouch::new(Key::NavForward, self.next_button_rect))
        } else {
            None
        }
    }
}
