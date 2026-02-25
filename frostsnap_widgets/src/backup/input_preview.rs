use super::backup_model::{FramebufferMutation, MainViewState, ViewState};
use crate::animation_speed::AnimationSpeed;
use crate::cursor::Cursor;
use crate::palette::PALETTE;
use crate::progress_bars::ProgressBars;
use crate::super_draw_target::SuperDrawTarget;
use crate::{icons, DynWidget, Frac, Key, KeyTouch, Widget, FONT_HUGE_MONO};
use alloc::rc::Rc;
use core::cell::RefCell;
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU4},
        Gray4, GrayColor, Rgb565,
    },
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
};
use frost_backup::NUM_WORDS;
use frostsnap_fonts::Gray4Font;

// Constants for vertical BIP39 word display
pub(super) const TOTAL_WORDS: usize = NUM_WORDS;
pub(super) const FONT_SIZE: Size = Size::new(17, 29);
pub(super) const TOP_PADDING: u32 = 10;
pub(super) const PREVIEW_HEIGHT: u32 = 60;
// ⚖️ when a row is centered in the viewport the next row's label must not peek in.
// Solving: FONT_SIZE.height + 2*VERTICAL_PAD >= PREVIEW_HEIGHT
pub(super) const VERTICAL_PAD: u32 = (PREVIEW_HEIGHT - FONT_SIZE.height).div_ceil(2);
const INDEX_CHARS: usize = 3; // "25." (with dot)
const SPACE_BETWEEN: usize = 0;
const PREVIEW_LEFT_PAD: i32 = 4;
pub(super) const FB_WIDTH: u32 = 188; // 11 chars * 17px = 187, rounded up to 188 (divisible by 4)
pub(super) const FB_HEIGHT: u32 =
    TOP_PADDING + ((TOTAL_WORDS + 1) as u32 * (FONT_SIZE.height + VERTICAL_PAD)); // +1 for share index row

/// The Gray4 font used for the word list framebuffer
const FB_FONT: &Gray4Font = FONT_HUGE_MONO;

/// Gray levels used in the framebuffer to distinguish word numbers from word text
const INDEX_GRAY: u8 = 6; // Dim - for row numbers
const TEXT_GRAY: u8 = 15; // Full brightness - for entered text

pub(super) type Fb = Framebuffer<
    Gray4,
    RawU4,
    LittleEndian,
    { FB_WIDTH as usize },
    { FB_HEIGHT as usize },
    { buffer_size::<Gray4>(FB_WIDTH as usize, FB_HEIGHT as usize) },
>;

/// Draw a single character from a Gray4Font into a Gray4 DrawTarget.
/// Positioned at the given point using left alignment and top baseline.
fn draw_gray4_char<D: DrawTarget<Color = Gray4>>(
    target: &mut D,
    font: &'static Gray4Font,
    ch: char,
    position: Point,
    scale: u8,
) {
    let glyph = match font.get_glyph(ch) {
        Some(g) => g,
        None => return,
    };

    let draw_x = position.x + glyph.x_offset as i32;
    let draw_y = position.y + glyph.y_offset as i32;

    for Pixel(point, gray) in font.glyph_pixels(glyph) {
        let scaled = (gray.luma() as u16 * scale as u16 / 15) as u8;
        if scaled > 0 {
            let _ = Pixel(
                Point::new(draw_x + point.x, draw_y + point.y),
                Gray4::new(scaled),
            )
            .draw(target);
        }
    }
}

/// Draw a string of characters from a Gray4Font into a Gray4 DrawTarget.
/// Characters are drawn left-to-right using each glyph's x_advance.
fn draw_gray4_string<D: DrawTarget<Color = Gray4>>(
    target: &mut D,
    font: &'static Gray4Font,
    text: &str,
    position: Point,
    scale: u8,
) {
    let mut x = position.x;
    for ch in text.chars() {
        if let Some(glyph) = font.get_glyph(ch) {
            let draw_x = x + glyph.x_offset as i32;
            let draw_y = position.y + glyph.y_offset as i32;

            for Pixel(point, gray) in font.glyph_pixels(glyph) {
                let scaled = (gray.luma() as u16 * scale as u16 / 15) as u8;
                if scaled > 0 {
                    let _ = Pixel(
                        Point::new(draw_x + point.x, draw_y + point.y),
                        Gray4::new(scaled),
                    )
                    .draw(target);
                }
            }
            x += glyph.x_advance as i32;
        } else if ch == ' ' {
            x += (font.line_height / 4) as i32;
        }
    }
}

pub struct InputPreview {
    pub(super) area: Rectangle,
    preview_rect: Rectangle,
    backspace_rect: Rectangle,
    progress_rect: Rectangle,
    progress: ProgressBars,
    framebuf: Framebuf,
    init_draw: bool,
    cursor: Cursor,
    current_view_state: Option<ViewState>,
}

impl Default for InputPreview {
    fn default() -> Self {
        Self::new()
    }
}

impl InputPreview {
    pub fn new() -> Self {
        // Initialize with zero-sized rectangles - will be set in set_constraints
        let backspace_rect = Rectangle::zero();
        let preview_rect = Rectangle::zero();
        let progress_rect = Rectangle::zero();

        // 26 segments: 1 for share index + 25 words for Frostsnap backup
        let progress = ProgressBars::new(NUM_WORDS + 1);
        let framebuf = Framebuf::new();

        Self {
            area: Rectangle::zero(),
            preview_rect,
            backspace_rect,
            progress_rect,
            progress,
            framebuf,
            init_draw: false,
            cursor: Cursor::new(Point::zero()),
            current_view_state: None,
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

    pub fn is_scrolling(&self) -> bool {
        self.framebuf.is_scrolling()
    }

    pub fn update_from_view_state(&mut self, view_state: &ViewState) {
        // Store the current view state
        self.current_view_state = Some(view_state.clone());
        // Update cursor position based on view state
        let x = ((INDEX_CHARS + SPACE_BETWEEN) + view_state.cursor_pos) * FONT_SIZE.width as usize;
        // Y position: align cursor bottom with text cell bottom in the viewport.
        // Text cell bottom in viewport = (TOP_PADDING + VERTICAL_PAD/2 + FONT_SIZE.height) - scroll_offset
        // where scroll_offset centers the row: TOP_PADDING + row_height/2 - viewport_height/2
        // Simplifies to: (viewport_height + FONT_SIZE.height) / 2 - cursor_height
        let cursor_height = 2i32;
        let y =
            (self.preview_rect.size.height as i32 + FONT_SIZE.height as i32) / 2 - cursor_height;
        self.cursor.set_position(Point::new(x as i32, y));

        // Enable cursor when there's text but row isn't complete (not in word selection)
        let cursor_enabled = match &view_state.main_view {
            MainViewState::EnterShareIndex { current } => !current.is_empty(),
            MainViewState::EnterWord { .. } => view_state.cursor_pos > 0,
            MainViewState::WordSelect { .. } => false, // No cursor during word selection
            MainViewState::AllWordsEntered { .. } => false, // No cursor when all words entered
        };
        self.cursor.enabled(cursor_enabled);

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

impl crate::DynWidget for InputPreview {
    fn set_constraints(&mut self, max_size: Size) {
        let progress_height = 4;
        let backspace_width = max_size.width / 4;

        self.backspace_rect = Rectangle::new(
            Point::new(max_size.width as i32 - backspace_width as i32, 0),
            Size {
                width: backspace_width,
                height: max_size.height - progress_height,
            },
        );

        self.preview_rect = Rectangle::new(
            Point::new(PREVIEW_LEFT_PAD, 0),
            Size {
                width: FB_WIDTH, // Must match framebuffer width exactly
                height: max_size.height - progress_height,
            },
        );

        self.progress_rect = Rectangle::new(
            Point::new(0, max_size.height as i32 - progress_height as i32),
            Size::new(max_size.width, progress_height),
        );

        self.progress.set_constraints(self.progress_rect.size);
        self.framebuf.set_constraints(self.preview_rect.size);
        self.area = Rectangle::new(Point::zero(), max_size);
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
            // Only allow showing entered words if the current state permits it
            if let Some(ref view_state) = self.current_view_state {
                if view_state.can_show_entered_words() {
                    Some(KeyTouch::new(Key::ShowEnteredWords, self.preview_rect))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Widget for InputPreview {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        if !self.init_draw {
            // Clear the entire area on first draw
            let clear_rect = Rectangle::new(Point::zero(), self.area.size);
            let _ = clear_rect
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(PALETTE.background)
                        .build(),
                )
                .draw(target);

            // Draw backspace icon in the right portion of its touch area
            icons::backspace()
                .with_color(PALETTE.error)
                .with_center(self.backspace_rect.center())
                .draw(target);

            self.init_draw = true;
        }

        self.framebuf
            .draw(&mut target.clone().crop(self.preview_rect), current_time)?;

        // Draw cursor when enabled (text entered but row not complete)
        let _ = self.draw_cursor(&mut target.clone().crop(self.preview_rect), current_time);

        // Always draw progress bars (they have their own redraw logic)
        self.progress
            .draw(&mut target.clone().crop(self.progress_rect), current_time)?;

        Ok(())
    }
}

pub struct Framebuf {
    framebuffer: Rc<RefCell<Fb>>,
    current_position: u32,
    current_time: Option<crate::Instant>,
    target_position: u32,
    scroll_start_position: u32,
    scroll_start_time: Option<crate::Instant>,
    scroll_easing: AnimationSpeed,
    viewport_height: u32,
    pub(super) redraw: bool,
}

impl Framebuf {
    pub fn new() -> Self {
        let fb = Rc::new(RefCell::new(Fb::new()));

        // Clear the framebuffer
        let _ = fb.borrow_mut().clear(Gray4::new(0));

        // Pre-render share index placeholder with '#' prefix (no dot for share index)
        let share_y = TOP_PADDING as i32 + (VERTICAL_PAD / 2) as i32;
        draw_gray4_string(
            &mut *fb.borrow_mut(),
            FB_FONT,
            " #",
            Point::new(0, share_y),
            INDEX_GRAY,
        );

        // Pre-render word indices with dots
        for i in 0..TOTAL_WORDS {
            // Word i is at row i+1 (row 0 is share index)
            let row = i + 1;
            let y = TOP_PADDING as i32
                + (row as u32 * (FONT_SIZE.height + VERTICAL_PAD)) as i32
                + (VERTICAL_PAD / 2) as i32;
            let number_with_dot = alloc::format!("{}.", i + 1);

            // Right-align numbers at 3 characters from left (with dots)
            let number_right_edge = 3 * FONT_SIZE.width as i32;

            // Calculate number position to right-align
            let number_x = if i < 9 {
                // Single digit + dot: right-aligned (takes 2 chars)
                number_right_edge - (2 * FONT_SIZE.width as i32)
            } else {
                // Double digit + dot: starts at position 0 (takes 3 chars)
                0
            };

            draw_gray4_string(
                &mut *fb.borrow_mut(),
                FB_FONT,
                &number_with_dot,
                Point::new(number_x, y),
                INDEX_GRAY,
            );
        }

        Self {
            framebuffer: fb,
            current_position: 0,
            current_time: None,
            target_position: 0,
            scroll_start_position: 0,
            scroll_start_time: None,
            scroll_easing: AnimationSpeed::EaseIn,
            viewport_height: 34,
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

                    let _ = char_frame.clear(Gray4::new(0));
                    draw_gray4_char(&mut char_frame, FB_FONT, *ch, Point::zero(), TEXT_GRAY);
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
                    let _ = char_frame.clear(Gray4::new(0));
                }
            }
            self.redraw = true;
        }
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
                self.scroll_start_time = None;
            } else {
                self.scroll_start_position = self.current_position;
                self.scroll_start_time = self.current_time;
                // ⏩ forward (next word): linger then accelerate away
                // ⏪ backward (backspace): snap quickly then settle
                self.scroll_easing = if new_target > self.current_position {
                    AnimationSpeed::EaseIn
                } else {
                    AnimationSpeed::EaseOut
                };
            }
            self.redraw = true;
        }
    }

    /// Fast forward scrolling by jumping to target position
    pub fn fast_forward_scrolling(&mut self) {
        self.redraw = self.current_position != self.target_position;
        self.current_position = self.target_position;
        self.scroll_start_time = None;
    }

    /// Check if the framebuffer is currently scrolling
    pub fn is_scrolling(&self) -> bool {
        self.current_position != self.target_position
    }
}

impl crate::DynWidget for Framebuf {
    fn set_constraints(&mut self, max_size: Size) {
        // Update viewport height based on constraints
        self.viewport_height = max_size.height;
    }

    fn sizing(&self) -> crate::Sizing {
        // Return the actual framebuffer dimensions
        crate::Sizing {
            width: FB_WIDTH,
            height: self.viewport_height,
            ..Default::default()
        }
    }
}

impl Widget for Framebuf {
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

        // Assert that viewport height matches what was set in set_constraints
        assert_eq!(
            self.viewport_height, bb.size.height,
            "Viewport height mismatch: expected {} from set_constraints, got {} in draw",
            self.viewport_height, bb.size.height
        );

        // On first draw, jump to target position
        if is_first_draw {
            self.current_position = self.target_position;
        }

        self.current_time = Some(current_time);

        if self.current_position != self.target_position {
            let start_time = *self.scroll_start_time.get_or_insert(current_time);
            let elapsed = current_time.saturating_duration_since(start_time) as u32;

            const SCROLL_DURATION_MS: u32 = 1500;

            let progress = Frac::from_ratio(elapsed.min(SCROLL_DURATION_MS), SCROLL_DURATION_MS);
            let eased = self.scroll_easing.apply(progress);

            let start = self.scroll_start_position as i32;
            let end = self.target_position as i32;
            self.current_position = (start + eased.as_rat() * (end - start)).max(0) as u32;

            if progress >= Frac::ONE {
                self.current_position = self.target_position;
                self.scroll_start_time = None;
            }

            self.redraw = true;
        } else {
            self.scroll_start_time = None;
        }

        // Only redraw if needed
        if !self.redraw {
            return Ok(());
        }

        // Build a linear color LUT for Gray4 → Rgb565 mapping.
        // Numbers appear dim (INDEX_GRAY=4 scales pixels to 0-4 range),
        // text appears bright (TEXT_GRAY=15 uses full 0-15 range).
        let color_lut = {
            use crate::{ColorInterpolate, Frac};
            let mut lut = [PALETTE.background; 16];
            for i in 1..16u8 {
                let alpha = Frac::from_ratio(i as u32, 15);
                lut[i as usize] = PALETTE.background.interpolate(PALETTE.on_background, alpha);
            }
            lut
        };

        // Skip to the correct starting position in the framebuffer
        let skip_rows = self.current_position as usize;
        let skip_pixels = skip_rows * FB_WIDTH as usize;
        let take_pixels = bb.size.height as usize * bb.size.width as usize;

        {
            let fb = self.framebuffer.try_borrow().unwrap();
            let framebuffer_pixels = RawDataSlice::<RawU4, LittleEndian>::new(fb.data())
                .into_iter()
                .skip(skip_pixels)
                .take(take_pixels)
                .map(|r| color_lut[Gray4::from(r).luma() as usize]);

            target.fill_contiguous(&bb, framebuffer_pixels)?;
        }

        // Only clear redraw flag if animation is complete
        if self.current_position == self.target_position {
            self.redraw = false;
        }

        Ok(())
    }
}
