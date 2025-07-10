use crate::palette::PALETTE;
use alloc::boxed::Box;
use embedded_graphics::{
    framebuffer::{buffer_size, Framebuffer},
    iterator::raw::RawDataSlice,
    pixelcolor::{
        raw::{LittleEndian, RawU2},
        Gray2, Rgb565,
    },
    prelude::*,
    primitives::Rectangle,
};

use super::framebuffer_slide_iterator::{SlideDirection, SlideIterator};

const ANIMATION_DURATION_MS: u64 = 300;

pub type PageFramebuffer = Framebuffer<
    Gray2,
    RawU2,
    LittleEndian,
    240, // width
    200, // height
    { buffer_size::<Gray2>(240, 200) },
>;

#[derive(Debug)]
struct Animation {
    start_time: Option<crate::Instant>,
    direction: SlideDirection,
}

#[derive(Debug)]
pub struct PageTransitionHandler {
    area: Rectangle,
    current_fb: Box<PageFramebuffer>,
    next_fb: Box<PageFramebuffer>,
    animation: Option<Animation>,
    width: usize,
    init_draw: bool,
}

impl PageTransitionHandler {
    pub fn new(area: Rectangle) -> Self {
        let width = area.size.width as usize;

        Self {
            area,
            current_fb: Box::new(Framebuffer::new()),
            next_fb: Box::new(Framebuffer::new()),
            animation: None,
            width,
            init_draw: true,
        }
    }

    pub fn init_page<F>(&mut self, builder: F)
    where
        F: FnOnce(&mut PageFramebuffer),
    {
        builder(&mut self.current_fb);
    }

    pub fn next_page<F>(&mut self, builder: F)
    where
        F: FnOnce(&mut PageFramebuffer),
    {
        if self.animation.is_none() {
            builder(&mut self.next_fb);
            self.animation = Some(Animation {
                start_time: None,
                direction: SlideDirection::Left, // Next page slides in from the right
            });
        }
    }

    pub fn prev_page<F>(&mut self, builder: F)
    where
        F: FnOnce(&mut PageFramebuffer),
    {
        if self.animation.is_none() {
            builder(&mut self.next_fb);
            self.animation = Some(Animation {
                start_time: None,
                direction: SlideDirection::Right, // Previous page slides in from the left
            });
        }
    }

    pub fn draw<D: DrawTarget<Color = Rgb565>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) {
        // Skip drawing if no animation and not initial draw
        if self.animation.is_none() && !self.init_draw {
            return;
        }

        // Set animation start time if needed
        if let Some(ref mut anim) = self.animation {
            if anim.start_time.is_none() {
                anim.start_time = Some(current_time);
            }
        }

        // Calculate animation progress and direction
        let (progress, direction) = match &self.animation {
            Some(anim) => {
                let elapsed = anim
                    .start_time
                    .and_then(|start| current_time.checked_duration_since(start))
                    .map(|d| d.to_millis() as f32 / ANIMATION_DURATION_MS as f32)
                    .unwrap_or(0.0)
                    .min(1.0);
                (elapsed, anim.direction)
            }
            None => (0.0, SlideDirection::Left), // Default for init_draw
        };

        // Calculate how many pixels to take from next framebuffer
        let next_pixels = (self.width as f32 * progress) as usize;

        let pixels = SlideIterator::new_overlapping(
            RawDataSlice::<RawU2, LittleEndian>::new(self.current_fb.data()).into_iter(),
            RawDataSlice::<RawU2, LittleEndian>::new(self.next_fb.data()).into_iter(),
            self.width,
            next_pixels,
            direction,
        );

        // Convert RawU2 to Rgb565 and draw
        let color_pixels = pixels.map(|pixel_value| match pixel_value.into_inner() {
            0 => PALETTE.background,
            1 => Rgb565::new(10, 10, 10),
            2 => Rgb565::new(20, 20, 20),
            _ => PALETTE.primary,
        });

        let _ = target.fill_contiguous(&self.area, color_pixels);

        // Handle animation completion AFTER drawing
        if progress >= 1.0 && self.animation.is_some() {
            core::mem::swap(&mut self.current_fb, &mut self.next_fb);
            self.animation = None;
        }

        // Clear init_draw flag after first draw
        self.init_draw = false;
    }

    pub fn is_animating(&self) -> bool {
        self.animation.is_some()
    }
}
