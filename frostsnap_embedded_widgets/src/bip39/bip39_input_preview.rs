use super::bip39_model::{FramebufferMutation, ViewState};
use crate::cursor::Cursor;
use crate::palette::PALETTE;
use crate::progress_bars::ProgressBars;
use crate::super_draw_target::SuperDrawTarget;
use crate::{icons, DynWidget, Key, KeyTouch, Widget, FONT_LARGE};
use alloc::{boxed::Box, rc::Rc, string::ToString};
use core::cell::RefCell;
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    geometry::AnchorX,
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU2},
        Gray2, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use frostsnap_backup::bip39_words::FROSTSNAP_BACKUP_WORDS;
use u8g2_fonts::U8g2TextStyle;

// Constants for vertical BIP39 word display
pub(super) const TOTAL_WORDS: usize = FROSTSNAP_BACKUP_WORDS;
pub(super) const FONT_SIZE: Size = Size::new(16, 24);
pub(super) const VERTICAL_PAD: u32 = 12; // 6px top + 6px bottom padding per word
                                         // 180 pixels width / 16 pixels per char = 11.25 chars total
                                         // So we can fit 11 chars total
const INDEX_CHARS: usize = 2; // "25" (no dot)
const SPACE_BETWEEN: usize = 1;
const PREVIEW_LEFT_PAD: i32 = 4; // Left padding for preview rect
pub(super) const TOP_PADDING: u32 = 10; // Top padding before first word
pub(super) const FB_WIDTH: u32 = 176; // Divisible by 4 for Gray2 alignment
pub(super) const FB_HEIGHT: u32 =
    TOP_PADDING + ((TOTAL_WORDS + 1) as u32 * (FONT_SIZE.height + VERTICAL_PAD)); // +1 for share index row

pub(super) type Fb = Framebuffer<
    Gray2,
    RawU2,
    LittleEndian,
    { FB_WIDTH as usize },
    { FB_HEIGHT as usize },
    { buffer_size::<Gray2>(FB_WIDTH as usize, FB_HEIGHT as usize) },
>;

#[derive(Debug)]
pub struct Bip39InputPreview {
    pub(super) area: Rectangle,
    preview_rect: Rectangle,
    backspace_rect: Rectangle,
    progress_rect: Rectangle,
    progress: ProgressBars,
    framebuf: Bip39Framebuf,
    init_draw: bool,
    cursor: Cursor,
}

impl Bip39InputPreview {
    pub fn new(area: Rectangle) -> Self {
        let progress_height = 4;
        let backspace_width = area.size.width / 4;
        let backspace_rect = Rectangle::new(
            Point::new(area.size.width as i32 - backspace_width as i32, 0),
            Size {
                width: backspace_width,
                height: area.size.height - progress_height,
            },
        );

        // Preview rect should use full available height
        let preview_rect = Rectangle::new(
            Point::new(PREVIEW_LEFT_PAD, 0),
            Size {
                width: FB_WIDTH, // Must match framebuffer width exactly
                height: area.size.height - progress_height,
            },
        );

        let progress_rect = Rectangle::new(
            Point::new(0, area.size.height as i32 - progress_height as i32),
            Size::new(area.size.width, progress_height),
        );

        // 26 segments: 1 for share index + 25 words for Frostsnap backup
        let mut progress = ProgressBars::new(FROSTSNAP_BACKUP_WORDS + 1);
        progress.set_constraints(progress_rect.size);
        let framebuf = Bip39Framebuf::new();

        Self {
            area,
            preview_rect,
            backspace_rect,
            progress_rect,
            progress,
            framebuf,
            init_draw: false,
            cursor: Cursor::new(Point::zero()), // Will update position in draw
        }
    }

    pub fn apply_mutations(&mut self, mutations: &[FramebufferMutation]) {
        self.framebuf.apply_mutations(mutations);
    }

    pub fn update_progress(&mut self, completed_words: usize) {
        self.progress.progress(completed_words);
    }

    pub fn contains(&self, point: Point) -> bool {
        self.preview_rect.contains(point)
    }

    pub fn get_framebuffer(&self) -> Rc<RefCell<Fb>> {
        self.framebuf.framebuffer.clone()
    }

    /// Force redraw of the input preview (including progress bar)
    pub fn force_redraw(&mut self) {
        self.init_draw = false;
        self.framebuf.redraw = true;
        self.progress.force_full_redraw();
    }

    /// Fast forward any ongoing scrolling animation
    pub fn fast_forward_scrolling(&mut self) {
        self.framebuf.fast_forward_scrolling();
    }

    pub fn update_from_view_state(&mut self, view_state: &ViewState) {
        // Update cursor position based on view state
        let x = ((INDEX_CHARS + SPACE_BETWEEN) + view_state.cursor_pos) * FONT_SIZE.width as usize;
        // Fixed Y position - cursor always appears at the same vertical position
        let y = self.preview_rect.size.height as i32 / 2 - FONT_SIZE.height as i32 / 2;
        self.cursor.set_position(Point::new(x as i32, y));

        // Update scroll position to show the current row
        self.framebuf
            .update_scroll_position_for_row(view_state.row, false);
    }

    fn draw_cursor<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut SuperDrawTarget<D, Rgb565>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Let the cursor handle its own drawing and blinking
        self.cursor.draw(target, current_time)?;
        Ok(())
    }
}

impl crate::DynWidget for Bip39InputPreview {
    fn set_constraints(&mut self, _max_size: Size) {
        // Bip39InputPreview has fixed size based on its area
    }

    fn sizing(&self) -> crate::Sizing {
        self.area.size.into()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        _current_time: crate::Instant,
        _lift_up: bool,
    ) -> Option<KeyTouch> {
        if self.backspace_rect.contains(point) {
            Some(KeyTouch::new(Key::Keyboard('⌫'), self.backspace_rect))
        } else if self.preview_rect.contains(point) {
            // Tap on the text preview area triggers the entered words view
            Some(KeyTouch::new(Key::ShowEnteredWords, self.preview_rect))
        } else {
            None
        }
    }
}

impl Widget for Bip39InputPreview {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        // Draw backspace icon on first draw
        if !self.init_draw {
            // Clear the entire area first
            let clear_rect = Rectangle::new(Point::zero(), self.area.size);
            let _ = clear_rect
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(PALETTE.background)
                        .build(),
                )
                .draw(target);

            icons::backspace()
                .with_color(PALETTE.error)
                .with_center(
                    self.backspace_rect
                        .resized_width(self.backspace_rect.size.width / 2, AnchorX::Left)
                        .center(),
                )
                .draw(target);
            self.init_draw = true;
        }

        // Always draw the framebuffer (it has its own redraw logic)
        self.framebuf
            .draw(&mut target.clone().crop(self.preview_rect), current_time)?;

        // Draw cursor if editing
        let _ = self.draw_cursor(&mut target.clone().crop(self.preview_rect), current_time);

        // Always draw progress bars (they have their own redraw logic)
        self.progress
            .draw(&mut target.clone().crop(self.progress_rect), current_time)?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct Bip39Framebuf {
    framebuffer: Rc<RefCell<Fb>>,
    current_position: u32, // Current vertical scroll position
    current_time: Option<crate::Instant>,
    target_position: u32, // Target vertical scroll position
    animation_start_time: Option<crate::Instant>, // When current animation started
    viewport_height: u32, // Height of the visible area
    pub(super) redraw: bool,
}

impl Bip39Framebuf {
    pub fn new() -> Self {
        let fb = Rc::new(RefCell::new(Fb::new()));
        
        // Clear the framebuffer
        let _ = fb.borrow_mut().clear(Gray2::BLACK);

        // Pre-render share index placeholder with '#' prefix
        let share_y = TOP_PADDING as i32 + (VERTICAL_PAD / 2) as i32;
        let _ = Text::with_text_style(
            " #",
            Point::new(0, share_y),
            U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x01)),
            TextStyleBuilder::new()
                .alignment(Alignment::Left)
                .baseline(Baseline::Top)
                .build(),
        )
        .draw(&mut *fb.borrow_mut());

        // Pre-render word indices with aligned dots (starting from second position)
        for i in 0..TOTAL_WORDS {
            // Word i is at row i+1 (row 0 is share index)
            let row = i + 1;
            let y = TOP_PADDING as i32
                + (row as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as i32
                + (VERTICAL_PAD / 2) as i32;
            let number = (i + 1).to_string();

            // Right-align numbers at 2 characters from left (no dots)
            let number_right_edge = 32; // 2 * 16 pixels

            // Calculate number position to right-align
            let number_x = if i < 9 {
                // Single digit: right-aligned at position
                number_right_edge - FONT_SIZE.width as i32
            } else {
                // Double digit: starts at position 0
                0
            };

            // Draw the number with a different gray level
            let _ = Text::with_text_style(
                &number,
                Point::new(number_x, y),
                U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x01)), // Use Gray level 1 for numbers
                TextStyleBuilder::new()
                    .alignment(Alignment::Left)
                    .baseline(Baseline::Top)
                    .build(),
            )
            .draw(&mut *fb.borrow_mut());
        }

        Self {
            framebuffer: fb,
            current_position: 0,
            current_time: None,
            target_position: 0,
            animation_start_time: None,
            viewport_height: 34, // Default viewport height
            redraw: true,
        }
    }

    pub fn apply_mutations(&mut self, mutations: &[FramebufferMutation]) {
        let mut fb = self.framebuffer.borrow_mut();

        for mutation in mutations {
            match mutation {
                FramebufferMutation::SetCharacter { row, pos, char: ch } => {
                    let x = ((INDEX_CHARS + SPACE_BETWEEN) + pos) * FONT_SIZE.width as usize;
                    let y = TOP_PADDING as usize
                        + (*row as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as usize
                        + (VERTICAL_PAD / 2) as usize;

                    let mut char_frame = fb.cropped(&Rectangle::new(
                        Point::new(x as i32, y as i32),
                        Size::new(FONT_SIZE.width, FONT_SIZE.height),
                    ));

                    let _ = char_frame.clear(Gray2::BLACK);
                    let _ = Text::with_text_style(
                        &ch.to_string(),
                        Point::zero(),
                        U8g2TextStyle::new(FONT_LARGE, Gray2::new(0x02)),
                        TextStyleBuilder::new()
                            .alignment(Alignment::Left)
                            .baseline(Baseline::Top)
                            .build(),
                    )
                    .draw(&mut char_frame);
                }
                FramebufferMutation::DelCharacter { row, pos } => {
                    let x = ((INDEX_CHARS + SPACE_BETWEEN) + pos) * FONT_SIZE.width as usize;
                    let y = TOP_PADDING as usize
                        + (*row as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as usize
                        + (VERTICAL_PAD / 2) as usize;

                    let mut char_frame = fb.cropped(&Rectangle::new(
                        Point::new(x as i32, y as i32),
                        Size::new(FONT_SIZE.width, FONT_SIZE.height),
                    ));
                    let _ = char_frame.clear(Gray2::BLACK);
                }
            }
        }

        self.redraw = true;
    }

    // Update scroll position for a specific row
    pub fn update_scroll_position_for_row(&mut self, row: usize, skip_animation: bool) {
        // Calculate position to center the row in the viewport
        let row_height = FONT_SIZE.height + VERTICAL_PAD;
        let row_position = TOP_PADDING + (row as u32 * row_height);
        
        // To center the row vertically: we want the row to appear at viewport_height/2
        // The row's center is at row_position + row_height/2
        // So scroll position should be: (row_position + row_height/2) - viewport_height/2
        let row_center = row_position + row_height / 2;
        let new_target = row_center.saturating_sub(self.viewport_height / 2);

        if new_target != self.target_position {
            self.target_position = new_target;
            if skip_animation {
                self.current_position = new_target;
                self.animation_start_time = None;
            } else {
                self.animation_start_time = self.current_time;
            }
            self.redraw = true;
        }
    }

    /// Fast forward scrolling by jumping to target position
    pub fn fast_forward_scrolling(&mut self) {
        self.redraw = self.current_position != self.target_position;
        self.current_position = self.target_position;
        self.animation_start_time = None;
    }
}

impl crate::DynWidget for Bip39Framebuf {
    fn set_constraints(&mut self, max_size: Size) {
        // Update viewport height based on constraints
        self.viewport_height = max_size.height;
    }

    fn sizing(&self) -> crate::Sizing {
        // Return the actual framebuffer dimensions
        crate::Sizing {
            width: FB_WIDTH,
            height: self.viewport_height,
        }
    }
}

impl Widget for Bip39Framebuf {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let bb = target.bounding_box();

        // Assert that framebuffer width matches target width
        assert_eq!(
            FB_WIDTH, bb.size.width,
            "Framebuffer width ({}) must match target width ({})",
            FB_WIDTH, bb.size.width
        );

        // Check if this is the first draw
        let is_first_draw = self.current_time.is_none();

        // Update viewport height if it changed
        if self.viewport_height != bb.size.height {
            self.viewport_height = bb.size.height;
            // Recalculate target position with new viewport
            // For now, just keep position at 0 since we don't track current word
            self.target_position = 0;
            if is_first_draw {
                self.current_position = 0;
            }
        }

        // On first draw, jump to target position
        if is_first_draw {
            self.current_position = self.target_position;
        }

        // Animate scrolling using acceleration
        let last_draw_time = self.current_time.get_or_insert(current_time);

        if self.current_position != self.target_position {
            // Calculate time since animation started
            let animation_elapsed = if let Some(start_time) = self.animation_start_time {
                current_time.duration_since(start_time).unwrap_or(0) as f32
            } else {
                self.animation_start_time = Some(current_time);
                0.0
            };

            // Accelerating curve: starts slow, speeds up
            // Using a quadratic function for smooth acceleration
            const ACCELERATION: f32 = 0.00000005; // Acceleration factor (5x faster)
            const MIN_VELOCITY: f32 = 0.0005; // Minimum velocity to ensure it starts moving

            // Calculate current velocity based on time elapsed
            let velocity = MIN_VELOCITY + (ACCELERATION * animation_elapsed * animation_elapsed);

            // Calculate distance to move this frame
            let frame_duration = current_time.duration_since(*last_draw_time).unwrap_or(0) as f32;

            // For upward scrolling, we want positive distance to move up (decrease position)
            // When velocity is negative, we actually want to move down briefly
            // Manual rounding: add 0.5 and truncate for positive values
            let raw_distance = frame_duration * velocity;
            let distance = if raw_distance >= 0.0 {
                (raw_distance + 0.5) as i32
            } else {
                (raw_distance - 0.5) as i32
            };

            // Only proceed if we're actually going to move
            if distance != 0 {
                *last_draw_time = current_time;

                // Direction: negative means scrolling up (decreasing position)
                let direction =
                    (self.target_position as i32 - self.current_position as i32).signum();

                // Apply the velocity in the correct direction
                // For upward scroll (direction < 0), positive velocity should decrease position
                let position_change = if direction < 0 {
                    -distance // Upward scroll
                } else {
                    distance // Downward scroll
                };

                let new_position = (self.current_position as i32 + position_change).max(0);

                // Check if we've reached or passed the target
                if (direction < 0 && new_position <= self.target_position as i32)
                    || (direction > 0 && new_position >= self.target_position as i32)
                    || direction == 0
                {
                    self.current_position = self.target_position;
                    self.animation_start_time = None; // Animation complete
                } else {
                    self.current_position = new_position as u32;
                }

                self.redraw = true; // Keep redrawing until animation completes
            }
            // If distance is 0, we don't update last_draw_time, allowing frame_duration to accumulate
        } else {
            *last_draw_time = current_time;
            self.animation_start_time = None;
        }

        // Only redraw if needed
        if !self.redraw {
            return Ok(());
        }

        // Skip to the correct starting position in the framebuffer
        // current_position is already in pixels (Y coordinate), so we need to skip
        // that many rows worth of pixels in the framebuffer
        let skip_rows = self.current_position as usize;
        let skip_pixels = skip_rows * FB_WIDTH as usize;
        let take_pixels = bb.size.height as usize * bb.size.width as usize;

        {
            let fb = self.framebuffer.try_borrow().unwrap();
            let framebuffer_pixels = RawDataSlice::<RawU2, LittleEndian>::new(fb.data())
                .into_iter()
                .skip(skip_pixels)
                .take(take_pixels)
                .map(|pixel| match Gray2::from(pixel).luma() {
                    0x00 => PALETTE.background,
                    0x01 => PALETTE.outline, // Numbers in subtle outline color
                    0x02 => PALETTE.on_background, // Words in normal text color
                    0x03 => PALETTE.on_background, // Also words
                    _ => PALETTE.background,
                });

            target.fill_contiguous(&bb, framebuffer_pixels)?;
        }

        // Only clear redraw flag if animation is complete
        if self.current_position == self.target_position {
            self.redraw = false;
        }

        Ok(())
    }
}
