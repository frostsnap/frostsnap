use super::{pixel_recorder::PixelRecorder, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Circle},
};

pub struct Checkmark {
    size: Size,
    check_pixels: Vec<Point>,
    progress: f32, // 0.0 to 1.0
    last_drawn_check_progress: f32,
    animation_state: AnimationState,
    enabled: bool,
    animation_start_time: Option<crate::Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AnimationState {
    Idle,
    Drawing,
    Complete,
}

impl Checkmark {
    pub fn new(size: Size) -> Self {
        let mut checkmark = Self {
            size,
            check_pixels: Vec::new(),
            progress: 0.0,
            last_drawn_check_progress: -1.0,
            animation_state: AnimationState::Idle,
            enabled: false,
            animation_start_time: None,
        };
        
        checkmark.record_pixels();
        checkmark
    }
    
    pub fn start_animation(&mut self) {
        self.enabled = true;
        self.progress = 0.0;
        self.last_drawn_check_progress = -1.0;
        self.animation_state = AnimationState::Drawing;
        self.animation_start_time = None; // Will be set on first draw
    }
    
    pub fn reset(&mut self) {
        self.enabled = false;
        self.progress = 0.0;
        self.last_drawn_check_progress = -1.0;
        self.animation_state = AnimationState::Idle;
    }
    
    pub fn is_complete(&self) -> bool {
        self.animation_state == AnimationState::Complete
    }
    
    fn record_pixels(&mut self) {
        // Use PixelRecorder to capture custom checkmark shape
        let mut recorder = PixelRecorder::new();
        
        // Calculate checkmark dimensions relative to widget size
        // The checkmark has a shorter left stroke and longer right stroke
        let center_x = self.size.width as i32 / 2;
        let center_y = self.size.height as i32 / 2;
        
        // Define checkmark points
        // Left stroke: shorter, going down-right
        let left_start = Point::new(center_x - 20, center_y);
        let middle = Point::new(center_x - 5, center_y + 15);
        
        // Right stroke: longer, going up-right
        let right_end = Point::new(center_x + 25, center_y - 20);
        
        // Draw with thicker lines for better visibility
        let stroke_width = 8;
        let style = PrimitiveStyle::with_stroke(BinaryColor::On, stroke_width);
        
        // Draw left stroke
        Line::new(left_start, middle)
            .into_styled(style)
            .draw(&mut recorder)
            .ok();
            
        // Draw right stroke
        Line::new(middle, right_end)
            .into_styled(style)
            .draw(&mut recorder)
            .ok();
        
        // Add rounded caps at the ends - matching stroke width for subtle rounding
        let cap_diameter = stroke_width; // Same as stroke width for natural rounding
        let cap_style = PrimitiveStyle::with_fill(BinaryColor::On);
        
        // Round cap at left start
        Circle::with_center(left_start, cap_diameter)
            .into_styled(cap_style)
            .draw(&mut recorder)
            .ok();
            
        // Round cap at right end
        Circle::with_center(right_end, cap_diameter)
            .into_styled(cap_style)
            .draw(&mut recorder)
            .ok();
            
        // Round cap at middle joint
        Circle::with_center(middle, cap_diameter)
            .into_styled(cap_style)
            .draw(&mut recorder)
            .ok();
        
        // Sort pixels by x-coordinate (left to right animation)
        self.check_pixels = recorder.pixels;
        self.check_pixels.sort_by_key(|p| p.x);
    }
    
    fn update_animation(&mut self, current_time: crate::Instant) {
        if !self.enabled {
            return;
        }
        
        // Animation duration in milliseconds
        const CHECK_DURATION_MS: u64 = 500;   // 0.5 seconds for checkmark
        
        // Initialize animation start time if not set
        if self.animation_start_time.is_none() {
            self.animation_start_time = Some(current_time);
        }
        
        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::Drawing => {
                let start = self.animation_start_time.unwrap();
                let elapsed = current_time.saturating_duration_since(start);
                self.progress = (elapsed as f32 / CHECK_DURATION_MS as f32).min(1.0);
                
                if self.progress >= 1.0 {
                    self.progress = 1.0;
                    self.animation_state = AnimationState::Complete;
                }
            }
            AnimationState::Complete => {}
        }
    }
    
    fn draw_animation<D: DrawTarget<Color = BinaryColor>>(
        &mut self,
        target: &mut D,
    ) -> Result<(), D::Error> {
        let color = BinaryColor::On;
        
        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::Drawing | AnimationState::Complete => {
                // Draw checkmark progress
                let check_progress = self.progress.min(1.0); // Clamp to 1.0
                let current_pixels = (self.check_pixels.len() as f32 * check_progress) as usize;
                let last_pixels = (self.check_pixels.len() as f32 * self.last_drawn_check_progress.min(1.0)) as usize;
                
                if current_pixels > last_pixels && current_pixels <= self.check_pixels.len() {
                    target.draw_iter(
                        self.check_pixels[last_pixels..current_pixels]
                            .iter()
                            .map(|&point| Pixel(point, color)),
                    )?;
                }
                
                self.last_drawn_check_progress = check_progress;
            }
        }
        
        Ok(())
    }
}

impl Widget for Checkmark {
    type Color = BinaryColor;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        if self.enabled {
            self.update_animation(current_time);
            self.draw_animation(target)?;
        }
        Ok(())
    }
    
    fn handle_touch(&mut self, _point: Point, _current_time: crate::Instant, _is_release: bool) -> Option<crate::KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _start_y: Option<u32>, _current_y: u32) {}
    
    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}