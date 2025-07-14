use super::{pixel_recorder::PixelRecorder, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
};

#[derive(Debug)]
pub struct HoldToConfirmBorder<W> {
    child: W,
    progress: f32, // 0.0 to 1.0 (percentage of border completed)
    last_drawn_progress: f32,
    screen_size: Size,
    border_pixels: Vec<Point>, // Recorded border pixels
}

impl<W: Widget> HoldToConfirmBorder<W> {
    pub fn new(child: W) -> Self {
        // Get size from child widget - panic if it doesn't provide one
        let screen_size = child
            .size_hint()
            .expect("HoldToConfirm requires a child widget that provides size_hint()");

        let mut self_ = Self {
            child,
            progress: 0.0,
            last_drawn_progress: 0.0,
            screen_size,
            border_pixels: Vec::new(),
        };

        self_.record_border_pixels();

        self_
    }

    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    pub fn get_progress(&self) -> f32 {
        self.progress
    }

    fn record_border_pixels(&mut self) {
        const CORNER_RADIUS: u32 = 42;
        const BORDER_WIDTH: u32 = 6;

        let mut recorder = PixelRecorder::new();

        // Draw the rounded rectangle to the recorder
        use embedded_graphics::primitives::CornerRadii;
        let _ = RoundedRectangle::new(
            Rectangle::new(Point::new(0, 0), self.screen_size),
            CornerRadii::new(Size::new(CORNER_RADIUS, CORNER_RADIUS)),
        )
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, BORDER_WIDTH))
        .draw(&mut recorder);

        // Get the pixels and sort them
        let mut pixels = recorder.pixels;

        // Sort by (y/3, distance from middle x)
        let middle_x = self.screen_size.width as i32 / 2;
        pixels.sort_by_key(|point| {
            let mut y_bucket = point.y;

            if y_bucket < BORDER_WIDTH as i32 {
                y_bucket = 0;
            } else if y_bucket > (self.screen_size.height - BORDER_WIDTH) as i32 {
                y_bucket = i32::MAX;
            }
            let mut x_distance = (point.x - middle_x).abs() as i32;
            if point.y > self.screen_size.height as i32 / 2 {
                x_distance = -x_distance;
            }

            (y_bucket, x_distance)
        });

        self.border_pixels = pixels;
    }

    fn draw_border<D: DrawTarget<Color = BinaryColor>>(
        &mut self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        // Handle progress changes
        let total_pixels = self.border_pixels.len();
        let current_progress_pixels = (self.progress * total_pixels as f32) as usize;
        let last_progress_pixels =
            (self.last_drawn_progress.max(0.0) * total_pixels as f32) as usize;

        if current_progress_pixels > last_progress_pixels {
            target.draw_iter(
                self.border_pixels[last_progress_pixels..current_progress_pixels]
                    .iter()
                    .map(|&point| Pixel(point, BinaryColor::On)),
            )?;
        } else if current_progress_pixels < last_progress_pixels {
            target.draw_iter(
                self.border_pixels[current_progress_pixels..last_progress_pixels]
                    .iter()
                    .map(|&point| Pixel(point, BinaryColor::Off)),
            )?;
        }

        self.last_drawn_progress = self.progress;
        Ok(())
    }
}

impl<W: Widget<Color = BinaryColor>> Widget for HoldToConfirmBorder<W> {
    type Color = BinaryColor;

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

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32) {
        // Always forward to child
        self.child.handle_vertical_drag(prev_y, new_y);
    }

    fn size_hint(&self) -> Option<Size> {
        Some(self.screen_size)
    }

    fn force_full_redraw(&mut self) {
        self.last_drawn_progress = 0.0;
        // Also propagate to child
        self.child.force_full_redraw();
    }
}
