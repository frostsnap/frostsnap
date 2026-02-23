use crate::palette::PALETTE;
use crate::super_draw_target::SuperDrawTarget;
use crate::{ColorInterpolate, DynWidget, Frac, Sizing, Widget};
use alloc::{boxed::Box, vec};
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU4},
        Gray4, GrayColor, Rgb565,
    },
    prelude::*,
    primitives::Rectangle,
};
use frostsnap_fonts::{Gray4Font, NOTO_SANS_18_LIGHT, NOTO_SANS_MONO_24_BOLD};

const ADDRESS_FONT: &Gray4Font = &NOTO_SANS_MONO_24_BOLD;
const TITLE_FONT: &Gray4Font = &NOTO_SANS_18_LIGHT;

// Address font metrics: x_advance=14, line_height=25
const CHAR_ADVANCE: u32 = 14;
const ADDRESS_LINE_HEIGHT: u32 = 25;

// Title font metrics: line_height=19
const TITLE_LINE_HEIGHT: u32 = 19;
const TITLE_SPACER: u32 = 10;
const TITLE_AREA_HEIGHT: u32 = TITLE_LINE_HEIGHT + TITLE_SPACER; // 29px

// Address layout: 3 chunks of 4 chars per row, 15px between chunks, 3px between rows, 6 rows
const CHARS_PER_CHUNK: u32 = 4;
const CHUNKS_PER_ROW: u32 = 3;
const HORIZONTAL_SPACING: u32 = 15;
const VERTICAL_SPACING: u32 = 3;
const NUM_ROWS: u32 = 6;

// Computed dimensions (address area only)
const CHUNK_WIDTH: u32 = CHARS_PER_CHUNK * CHAR_ADVANCE; // 56px
const FB_WIDTH: u32 = CHUNKS_PER_ROW * CHUNK_WIDTH + (CHUNKS_PER_ROW - 1) * HORIZONTAL_SPACING; // 198px
const ADDRESS_HEIGHT: u32 = NUM_ROWS * ADDRESS_LINE_HEIGHT + (NUM_ROWS - 1) * VERTICAL_SPACING; // 165px
// Total height including title
const FB_HEIGHT: u32 = TITLE_AREA_HEIGHT + ADDRESS_HEIGHT; // 194px

type GrayFb = Framebuffer<
    Gray4,
    RawU4,
    LittleEndian,
    { FB_WIDTH as usize },
    { FB_HEIGHT as usize },
    { buffer_size::<Gray4>(FB_WIDTH as usize, FB_HEIGHT as usize) },
>;

/// Pre-rendered address page. The title ("To Address #X") and the address chunks
/// are all rendered into Gray4 framebuffers at construction time, then converted
/// to final Rgb565 pixels. On draw, we just blit the pre-computed pixels via
/// `fill_contiguous` — animation frames are essentially free.
pub struct AddressFramebuffer {
    /// Pre-computed Rgb565 pixels, row-major, FB_WIDTH * content_height entries
    pixels: Box<[Rgb565]>,
    content_height: u32,
}

/// Measure the pixel width of a string in a given font.
fn measure_string_width(font: &Gray4Font, text: &str) -> u32 {
    let mut width = 0u32;
    for ch in text.chars() {
        if let Some(glyph) = font.get_glyph(ch) {
            width += glyph.x_advance as u32;
        }
    }
    width
}

impl AddressFramebuffer {
    /// Build an address page with title and address chunks.
    /// The title is centered above the address grid. All text is pre-rendered to Rgb565.
    pub fn from_chunks(
        title: &str,
        chunks: &[&str],
        highlighted: &[bool],
        num_rows: u32,
    ) -> Self {
        let address_height = if num_rows == 0 {
            0
        } else {
            num_rows * ADDRESS_LINE_HEIGHT + (num_rows - 1) * VERTICAL_SPACING
        };
        let content_height = TITLE_AREA_HEIGHT + address_height;

        // Render into temporary Gray4 framebuffers:
        // - title_fb: title text (rendered with text_secondary LUT)
        // - normal_fb: normal-colored address chunks
        // - highlight_fb: highlighted address chunks
        let mut title_fb: Box<GrayFb> = Box::new(Framebuffer::new());
        let mut normal_fb: Box<GrayFb> = Box::new(Framebuffer::new());
        let mut highlight_fb: Box<GrayFb> = Box::new(Framebuffer::new());

        // Render title centered horizontally
        let title_width = measure_string_width(TITLE_FONT, title);
        let title_x = (FB_WIDTH.saturating_sub(title_width)) / 2;
        draw_gray4_string(
            &mut *title_fb,
            TITLE_FONT,
            title,
            Point::new(title_x as i32, 0),
            15,
        );

        // Render address chunks below the title area
        let address_y_offset = TITLE_AREA_HEIGHT;
        for (i, chunk) in chunks.iter().enumerate() {
            let col = (i % 3) as u32;
            let row = (i / 3) as u32;
            let x = col * (CHUNK_WIDTH + HORIZONTAL_SPACING);
            let y = address_y_offset + row * (ADDRESS_LINE_HEIGHT + VERTICAL_SPACING);
            let position = Point::new(x as i32, y as i32);
            let is_highlighted = highlighted.get(i).copied().unwrap_or(false);

            let fb = if is_highlighted {
                &mut *highlight_fb
            } else {
                &mut *normal_fb
            };
            draw_gray4_string(fb, ADDRESS_FONT, chunk, position, 15);
        }

        // Build LUTs for each color
        let title_lut = build_lut(PALETTE.text_secondary);
        let normal_lut = build_lut(PALETTE.primary);
        let highlight_lut = build_lut(PALETTE.on_background);

        // Convert to Rgb565
        let total_pixels = FB_WIDTH as usize * content_height as usize;
        let mut pixels = vec![PALETTE.background; total_pixels].into_boxed_slice();

        let title_iter = RawDataSlice::<RawU4, LittleEndian>::new(title_fb.data())
            .into_iter()
            .take(total_pixels);
        let normal_iter = RawDataSlice::<RawU4, LittleEndian>::new(normal_fb.data())
            .into_iter()
            .take(total_pixels);
        let highlight_iter = RawDataSlice::<RawU4, LittleEndian>::new(highlight_fb.data())
            .into_iter()
            .take(total_pixels);

        for (i, ((title_raw, norm_raw), high_raw)) in
            title_iter.zip(normal_iter).zip(highlight_iter).enumerate()
        {
            let title_val = Gray4::from(title_raw).luma() as usize;
            if title_val > 0 {
                pixels[i] = title_lut[title_val];
            } else {
                let high_val = Gray4::from(high_raw).luma() as usize;
                if high_val > 0 {
                    pixels[i] = highlight_lut[high_val];
                } else {
                    let norm_val = Gray4::from(norm_raw).luma() as usize;
                    if norm_val > 0 {
                        pixels[i] = normal_lut[norm_val];
                    }
                }
            }
        }

        Self {
            pixels,
            content_height,
        }
    }
}

fn build_lut(color: Rgb565) -> [Rgb565; 16] {
    let mut lut = [PALETTE.background; 16];
    for i in 1..16u8 {
        let alpha = Frac::from_ratio(i as u32, 15);
        lut[i as usize] = PALETTE.background.interpolate(color, alpha);
    }
    lut
}

/// Draw a string into a Gray4 framebuffer.
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
        }
    }
}

impl DynWidget for AddressFramebuffer {
    fn set_constraints(&mut self, _max_size: Size) {}

    fn sizing(&self) -> Sizing {
        Size::new(FB_WIDTH, self.content_height).into()
    }

    fn force_full_redraw(&mut self) {
        // No-op: we always blit the same pre-rendered pixels
    }
}

impl Widget for AddressFramebuffer {
    type Color = Rgb565;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        _current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        target.fill_contiguous(
            &Rectangle::new(Point::zero(), Size::new(FB_WIDTH, self.content_height)),
            self.pixels.iter().copied(),
        )
    }
}
