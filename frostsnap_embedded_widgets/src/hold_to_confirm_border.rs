use super::{pixel_recorder::PixelRecorder, compressed_point::CompressedPoint, rat::Frac, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{BinaryColor, PixelColor},
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
};


#[derive(Debug)]
pub struct HoldToConfirmBorder<W, C> 
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    pub child: W,
    progress: Frac, // 0 to 1 (percentage of border completed)
    last_drawn_progress: Frac,
    screen_size: Size,
    border_pixels: Vec<CompressedPoint>, // Recorded border pixels (compressed)
    border_width: u32,
    border_color: C,
    background_color: C,
}

impl<W, C> HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    pub fn new(child: W, border_width: u32, border_color: C, background_color: C) -> Self {
        // Get size from child widget - panic if it doesn't provide one
        let screen_size = child
            .size_hint()
            .expect("HoldToConfirm requires a child widget that provides size_hint()");

        let mut self_ = Self {
            child,
            progress: Frac::ZERO,
            last_drawn_progress: Frac::ZERO,
            screen_size,
            border_pixels: Vec::new(),
            border_width,
            border_color,
            background_color,
        };

        self_.record_border_pixels();

        self_
    }

    pub fn set_progress(&mut self, progress: Frac) {
        self.progress = progress;
    }

    pub fn get_progress(&self) -> Frac {
        self.progress
    }
    
    pub fn border_width(&self) -> u32 {
        self.border_width
    }

    fn record_border_pixels(&mut self) {
        const CORNER_RADIUS: u32 = 42;

        let mut recorder = PixelRecorder::new();

        // Draw the rounded rectangle to the recorder
        use embedded_graphics::primitives::CornerRadii;
        let _ = RoundedRectangle::new(
            Rectangle::new(Point::new(0, 0), self.screen_size),
            CornerRadii::new(Size::new(CORNER_RADIUS, CORNER_RADIUS)),
        )
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, self.border_width))
        .draw(&mut recorder);

        // Get the pixels and shrink capacity BEFORE sorting
        recorder.pixels.shrink_to_fit();
        let mut pixels = recorder.pixels;

        // Sort by (y/3, distance from middle x)
        let middle_x = self.screen_size.width as i32 / 2;
        pixels.sort_by_key(|cp| {
            let point = cp.to_point();
            let mut y_bucket = point.y;

            if y_bucket < self.border_width as i32 {
                y_bucket = 0;
            } else if y_bucket > (self.screen_size.height - self.border_width) as i32 {
                y_bucket = i32::MAX;
            }
            let mut x_distance = (point.x - middle_x).abs() as i32;
            if point.y > self.screen_size.height as i32 / 2 {
                x_distance = -x_distance;
            }

            (y_bucket, x_distance)
        });

        // Pixels are already compressed
        self.border_pixels = pixels;
    }

    fn draw_border<D: DrawTarget<Color = C>>(
        &mut self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        // Handle progress changes
        let total_pixels = self.border_pixels.len();
        let current_progress_pixels = (self.progress * total_pixels as u32).floor() as usize;
        let last_progress_pixels = (self.last_drawn_progress * total_pixels as u32).floor() as usize;

        if current_progress_pixels > last_progress_pixels {
            target.draw_iter(
                self.border_pixels[last_progress_pixels..current_progress_pixels]
                    .iter()
                    .map(|&pixel| Pixel(pixel.to_point(), self.border_color)),
            )?;
        } else if current_progress_pixels < last_progress_pixels {
            target.draw_iter(
                self.border_pixels[current_progress_pixels..last_progress_pixels]
                    .iter()
                    .map(|&pixel| Pixel(pixel.to_point(), self.background_color)),
            )?;
        }

        self.last_drawn_progress = self.progress;
        Ok(())
    }
}

impl<W, C> Widget for HoldToConfirmBorder<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor,
{
    type Color = C;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Always draw the child widget first
        self.child.draw(target, current_time)?;

        // Draw the border based on current progress
        self.draw_border(target)?;

        Ok(())
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<super::KeyTouch> {
        // Always forward to child
        self.child.handle_touch(point, current_time, lift_up)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, _is_release: bool) {
        // Always forward to child
        self.child.handle_vertical_drag(prev_y, new_y, _is_release);
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.screen_size)
    }

    fn force_full_redraw(&mut self) {
        self.last_drawn_progress = Frac::ZERO;
        // Also propagate to child
        self.child.force_full_redraw();
    }
}
