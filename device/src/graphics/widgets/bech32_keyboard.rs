use alloc::{boxed::Box, string::ToString};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    geometry::AnchorY,
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU1},
        BinaryColor, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use u8g2_fonts::U8g2TextStyle;

use crate::graphics::palette::COLORS;

use super::{key_touch::KeyTouch, FONT_LARGE};

// Constants for the framebuffer and keyboard dimensions
const FRAMEBUFFER_WIDTH: u32 = 240;
const KEY_WIDTH: u32 = 60; // 240 / 4
const KEY_HEIGHT: u32 = 50;
const TOTAL_ROWS: usize = 8;
const BAR_HEIGHT: u32 = 2;
const FRAMEBUFFER_HEIGHT: u32 = (TOTAL_ROWS as u32 * KEY_HEIGHT) + 2 * BAR_HEIGHT;
const KEYBOARD_COLOR: Rgb565 = Rgb565::new(25, 52, 26);

const KEYBOARD_KEYS: [[char; 4]; 8] = [
    ['0', '2', '3', '4'],
    ['5', '6', '7', '8'],
    ['9', 'A', 'C', 'D'],
    ['E', 'F', 'G', 'H'],
    ['J', 'K', 'L', 'M'],
    ['N', 'P', 'Q', 'R'],
    ['S', 'T', 'U', 'V'],
    ['W', 'X', 'Y', 'Z'],
];

type Fb = Framebuffer<
    BinaryColor,
    RawU1,
    LittleEndian,
    { FRAMEBUFFER_WIDTH as usize },
    { FRAMEBUFFER_HEIGHT as usize },
    { buffer_size::<BinaryColor>(FRAMEBUFFER_WIDTH as usize, FRAMEBUFFER_HEIGHT as usize) },
>;

#[derive(Debug)]
pub struct Bech32Keyboard {
    scroll_position: i32, // Current scroll offset
    framebuffer: Box<Fb>, // Boxed fixed-length array for the framebuffer
    needs_redraw: bool,   // Flag indicating if the keyboard needs to be redrawn
    max_scroll: i32,
    keyspace: Rectangle,
}

impl Bech32Keyboard {
    // Create a new Bech32Keyboard instance
    pub fn new(visible_height: u32) -> Self {
        // Allocate the framebuffer on the heap using Box

        // Calculate max_scroll based on the visible height of the screen
        let max_scroll = (FRAMEBUFFER_HEIGHT as i32 - visible_height as i32).max(0);

        let keyspace = Rectangle::new(
            Point::new(0, BAR_HEIGHT as i32),
            Size {
                height: FRAMEBUFFER_HEIGHT - BAR_HEIGHT,
                width: FRAMEBUFFER_WIDTH,
            },
        );

        let mut keyboard = Self {
            framebuffer: Box::new(Fb::new()),
            scroll_position: 0, // Start at the top of the keyboard
            max_scroll,         // Set the maximum scroll value
            needs_redraw: true, // Initial rendering required
            keyspace,
        };

        // Render the full keyboard to the framebuffer
        keyboard.render_full_keyboard();

        keyboard
    }

    // Scroll the keyboard by a specified amount (positive to scroll up, negative to scroll down)
    pub fn scroll(&mut self, amount: i32) {
        let new_position = (self.scroll_position - amount).clamp(0, self.max_scroll);
        // Only update if the scroll position changes
        if new_position != self.scroll_position {
            self.scroll_position = new_position;
            self.needs_redraw = true; // Mark for redraw
        }
    }

    // Render the full keyboard to the framebuffer (called once during initialization)
    fn render_full_keyboard(&mut self) {
        let mut keyspace = self.framebuffer.cropped(&self.keyspace);
        // the space where keys will be drawn is a bit smaller than the framebuffer
        // because of the top and bottom bars

        let character_style = U8g2TextStyle::new(FONT_LARGE, BinaryColor::On);
        // Draw all the keys into the framebuffer
        for (row_index, row) in KEYBOARD_KEYS.iter().enumerate() {
            for (col_index, &key) in row.iter().enumerate() {
                let x = col_index as i32 * KEY_WIDTH as i32;
                let y = row_index as i32 * KEY_HEIGHT as i32;

                // Draw the key label
                let position = Point::new(x + (KEY_WIDTH as i32 / 2), y + (KEY_HEIGHT as i32 / 2));
                let _ = Text::with_text_style(
                    &ToString::to_string(&key),
                    position,
                    character_style.clone(),
                    TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Middle)
                        .build(),
                )
                .draw(&mut keyspace);
            }
        }

        let bar_style = PrimitiveStyleBuilder::new()
            //NOTE: Disable bar for now
            .fill_color(BinaryColor::Off)
            .build();
        let bar_size = Size::new(FRAMEBUFFER_WIDTH, BAR_HEIGHT);
        let _ = Rectangle::new(Point::zero(), bar_size)
            .into_styled(bar_style)
            .draw(self.framebuffer.as_mut());

        let _ = Rectangle::new(
            Point::new(0, FRAMEBUFFER_HEIGHT as i32 - BAR_HEIGHT as i32),
            bar_size,
        )
        .into_styled(bar_style)
        .draw(self.framebuffer.as_mut());
    }

    // Draw the currently visible portion of the keyboard
    pub fn draw(&mut self, target: &mut impl DrawTarget<Color = Rgb565>) {
        // Only draw if a redraw is needed
        if self.needs_redraw {
            // Get the height of the visible area from the DrawTarget's bounding box
            let visible_height = target.bounding_box().size.height as usize;

            // Calculate the number of pixels to skip for the current scroll position
            let skip_pixels = self.scroll_position as usize * FRAMEBUFFER_WIDTH as usize;

            // Clip and draw the portion of the framebuffer based on scroll_position
            let _ = target.fill_contiguous(
                &Rectangle::new(Point::new(0, 0), target.bounding_box().size),
                RawDataSlice::<RawU1, LittleEndian>::new(self.framebuffer.data())
                    .into_iter()
                    .skip(skip_pixels)
                    .take(FRAMEBUFFER_WIDTH as usize * visible_height)
                    .map(|r| match BinaryColor::from(r) {
                        BinaryColor::Off => COLORS.background,
                        BinaryColor::On => KEYBOARD_COLOR,
                    }),
            );

            // Reset the redraw flag
            self.needs_redraw = false;
        }
    }

    // Handle a touch event and return an Option<KeyTouch>
    pub fn handle_touch(&self, mut point: Point) -> Option<KeyTouch> {
        // Use scroll_position directly as it represents the pixel offset
        let scroll_offset = self.scroll_position;
        if self.keyspace.contains(point) {
            point -= self.keyspace.top_left;
        } else {
            return None;
        }

        // Adjust the y-coordinate of the touch based on the current scroll position
        let adjusted_y = point.y + scroll_offset;

        // Calculate the row and column index based on touch coordinates
        let col_index = point.x / KEY_WIDTH as i32;
        let row_index = adjusted_y / KEY_HEIGHT as i32;

        // Ensure indices are within the valid range
        if (0..4).contains(&col_index) && (0..8).contains(&row_index) {
            // Find the key character from the KEYBOARD_KEYS array
            let key = KEYBOARD_KEYS[row_index as usize][col_index as usize];

            // Calculate the top-left corner of the key's rectangle
            let x = col_index * KEY_WIDTH as i32;
            let y = row_index * KEY_HEIGHT as i32 - scroll_offset + self.keyspace.top_left.y;

            // Create the rectangle for the key clamped so the y-value is no less than 0 so the
            // rectange doesn't overflow into the space above.
            let rect = Rectangle::new(Point::new(x, y), Size::new(KEY_WIDTH, KEY_HEIGHT))
                .resized_height((KEY_HEIGHT as i32 + y.min(0)) as u32, AnchorY::Bottom);

            return Some(KeyTouch::new(key, rect));
        }

        None
    }

    pub fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        let scroll_amount = match prev_y {
            Some(prev_y) => new_y as i32 - prev_y as i32,
            None => 0,
        };
        self.scroll(scroll_amount);
    }
}
