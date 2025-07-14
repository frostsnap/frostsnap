use crate::{Key, KeyTouch, Widget, FONT_LARGE, FONT_SMALL};
use alloc::format;
use embedded_graphics::{
    pixelcolor::{Gray2, Rgb565},
    prelude::*,
    primitives::Rectangle,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words::FROSTSNAP_BACKUP_WORDS;
use u8g2_fonts::{fonts, U8g2TextStyle};

use super::{
    navigation_buttons::NavigationButtons,
    page_transition_handler::{PageFramebuffer, PageTransitionHandler}
};

const WORDS_PER_PAGE: usize = 3;
const CONTENT_HEIGHT: u32 = 200;
const BUTTON_AREA_HEIGHT: u32 = 80;
const FB_WIDTH: usize = 240;
const FB_HEIGHT: usize = 200;

#[derive(Debug)]
pub struct DisplaySeedWords {
    words: [&'static str; 25],
    share_index: u16,
    current_page: usize,
    total_pages: usize,
    page_handler: PageTransitionHandler,
    nav_buttons: NavigationButtons,
    current_touch: Option<KeyTouch>,
    button_area: Rectangle,
    size: Size,
}

fn render_page_to_fb(
    page_index: usize,
    share_index: u16,
    words: &[&'static str; 25],
    fb: &mut PageFramebuffer,
) {
    // Clear framebuffer
    let _ = fb.clear(Gray2::new(0));

    if page_index == 0 {
        // First page: show share index
        let center_y = (FB_HEIGHT / 2) as i32;

        // Draw "Share number" label above in medium font
        let _ = Text::with_text_style(
            "Share number",
            Point::new((FB_WIDTH / 2) as i32, center_y - 60),
            U8g2TextStyle::new(fonts::u8g2_font_profont22_mf, Gray2::new(3)),
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(fb);

        // Draw share index number in extra large text
        let share_text = format!("#{}", share_index);
        let _ = Text::with_text_style(
            &share_text,
            Point::new((FB_WIDTH / 2) as i32, center_y),
            U8g2TextStyle::new(fonts::u8g2_font_inr42_mf, Gray2::new(3)),
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(fb);

        // Draw "Write it down" below
        let _ = Text::with_text_style(
            "Write it down",
            Point::new((FB_WIDTH / 2) as i32, center_y + 50),
            U8g2TextStyle::new(FONT_SMALL, Gray2::new(3)),
            TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Middle)
                .build(),
        )
        .draw(fb);
    } else {
        // Other pages: show words
        let start_y = 50;
        let line_spacing = 55;
        let number_x = 65; // Fixed x position for numbers
        let dot_x = 70; // Fixed x position for dots

        // Calculate which words to show (accounting for share index page)
        let word_start_index = (page_index - 1) * WORDS_PER_PAGE;

        // Show up to 3 words
        for i in 0..WORDS_PER_PAGE {
            let word_index = word_start_index + i;
            if word_index < FROSTSNAP_BACKUP_WORDS {
                let y_pos = start_y + (i as i32 * line_spacing);

                // Draw the number right-aligned
                let number_text = format!("{}", word_index + 1);
                let _ = Text::with_text_style(
                    &number_text,
                    Point::new(number_x, y_pos),
                    U8g2TextStyle::new(FONT_LARGE, Gray2::new(3)),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Right)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(fb);

                // Draw the dot
                let _ = Text::with_text_style(
                    ".",
                    Point::new(dot_x, y_pos),
                    U8g2TextStyle::new(FONT_LARGE, Gray2::new(3)),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(fb);

                // Draw the word
                let _ = Text::with_text_style(
                    words[word_index],
                    Point::new(dot_x + 10, y_pos),
                    U8g2TextStyle::new(FONT_LARGE, Gray2::new(3)),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Left)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(fb);
            }
        }
    }
}

impl DisplaySeedWords {
    pub fn new(area: Size, words: [&'static str; 25], share_index: u16) -> Self {
        // Calculate total pages: 1 share index page + word pages
        let total_pages = 1 + (FROSTSNAP_BACKUP_WORDS + WORDS_PER_PAGE - 1) / WORDS_PER_PAGE;

        // Create content area for page transitions
        let content_area = Rectangle::new(Point::zero(), Size::new(area.width, CONTENT_HEIGHT));

        let mut page_handler = PageTransitionHandler::new(content_area);
        let nav_buttons =
            NavigationButtons::new(Size::new(area.width, BUTTON_AREA_HEIGHT), 0, total_pages);

        let button_area = Rectangle::new(
            Point::new(0, CONTENT_HEIGHT as i32),
            Size::new(area.width, BUTTON_AREA_HEIGHT),
        );

        // Initialize the first page
        page_handler.init_page(|fb| {
            render_page_to_fb(0, share_index, &words, fb);
        });

        Self {
            words,
            share_index,
            current_page: 0,
            total_pages,
            page_handler,
            nav_buttons,
            current_touch: None,
            button_area,
            size: area,
        }
    }

    pub fn handle_touch(&mut self, point: Point, current_time: crate::Instant, lift_up: bool) {
        if lift_up {
            // Handle touch release
            if let Some(ref mut touch) = self.current_touch {
                if let Some(key) = touch.let_go(current_time) {
                    match key {
                        Key::NavBack => self.navigate_prev(),
                        Key::NavForward => self.navigate_next(),
                        _ => {}
                    }
                }
            }
        } else {
            // Handle new touch
            if point.y >= CONTENT_HEIGHT as i32 {
                // Touch is in button area - translate to button coordinate system
                let button_point = Point::new(point.x, point.y - CONTENT_HEIGHT as i32);
                if let Some(mut key_touch) = self.nav_buttons.handle_touch(button_point) {
                    // Translate the KeyTouch rectangle back to screen coordinates
                    key_touch.translate(Point::new(0, CONTENT_HEIGHT as i32));
                    // Cancel current touch if it's a different key
                    if let Some(ref mut current) = self.current_touch {
                        if current.key != key_touch.key {
                            current.cancel();
                        }
                    }
                    self.current_touch = Some(key_touch);
                }
            }
        }
    }

    fn navigate_prev(&mut self) {
        if self.current_page > 0 && !self.page_handler.is_animating() {
            self.current_page -= 1;
            self.nav_buttons.set_current_page(self.current_page);

            let page = self.current_page;
            let share_index = self.share_index;
            let words = self.words;

            self.page_handler.prev_page(|fb| {
                render_page_to_fb(page, share_index, &words, fb);
            });
        }
    }

    fn navigate_next(&mut self) {
        if self.current_page < self.total_pages - 1 && !self.page_handler.is_animating() {
            self.current_page += 1;
            self.nav_buttons.set_current_page(self.current_page);

            let page = self.current_page;
            let share_index = self.share_index;
            let words = self.words;

            self.page_handler.next_page(|fb| {
                render_page_to_fb(page, share_index, &words, fb);
            });
        }
    }
}

impl Widget for DisplaySeedWords {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Draw page content
        self.page_handler.draw(target, current_time);

        // Draw navigation buttons in a cropped view
        let mut button_target = target.cropped(&self.button_area);
        self.nav_buttons.draw(&mut button_target, current_time);

        // Draw current touch if any
        if let Some(ref mut touch) = self.current_touch {
            touch.draw(target, current_time);
            if touch.is_finished() {
                self.current_touch = None;
            }
        }
        
        Ok(())
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<KeyTouch> {
        if lift_up {
            // Handle touch release
            if let Some(ref mut touch) = self.current_touch {
                if let Some(key) = touch.let_go(current_time) {
                    match key {
                        Key::NavBack => self.navigate_prev(),
                        Key::NavForward => self.navigate_next(),
                        _ => {}
                    }
                }
            }
            None
        } else {
            // Handle new touch
            if point.y >= CONTENT_HEIGHT as i32 {
                // Touch is in button area - translate to button coordinate system
                let button_point = Point::new(point.x, point.y - CONTENT_HEIGHT as i32);
                if let Some(mut key_touch) = self.nav_buttons.handle_touch(button_point) {
                    // Translate the KeyTouch rectangle back to screen coordinates
                    key_touch.translate(Point::new(0, CONTENT_HEIGHT as i32));
                    // Cancel current touch if it's a different key
                    if let Some(ref mut current) = self.current_touch {
                        if current.key != key_touch.key {
                            current.cancel();
                        }
                    }
                    self.current_touch = Some(key_touch);
                    None
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}
