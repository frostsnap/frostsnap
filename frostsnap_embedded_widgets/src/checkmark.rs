use super::{pixel_recorder::PixelRecorder, Widget};
use alloc::vec::Vec;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::PrimitiveStyle,
};

pub struct Checkmark {
    size: Size,
    circle_pixels: Vec<Point>,
    check_pixels: Vec<Point>,
    progress: f32, // 0.0 to 1.0
    last_drawn_circle_progress: f32,
    last_drawn_check_progress: f32,
    animation_state: AnimationState,
    enabled: bool,
    animation_start_time: Option<crate::Instant>,
    phase_start_time: Option<crate::Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AnimationState {
    Idle,
    DrawingCircle,
    DrawingCheck,
    Complete,
}

impl Checkmark {
    pub fn new(size: Size) -> Self {
        let mut checkmark = Self {
            size,
            circle_pixels: Vec::new(),
            check_pixels: Vec::new(),
            progress: 0.0,
            last_drawn_circle_progress: -1.0,
            last_drawn_check_progress: -1.0,
            animation_state: AnimationState::Idle,
            enabled: false,
            animation_start_time: None,
            phase_start_time: None,
        };
        
        checkmark.record_pixels();
        checkmark
    }
    
    pub fn start_animation(&mut self) {
        self.enabled = true;
        self.progress = 0.0;
        self.last_drawn_circle_progress = -1.0;
        self.last_drawn_check_progress = -1.0;
        self.animation_state = AnimationState::DrawingCircle;
        self.animation_start_time = None; // Will be set on first draw
        self.phase_start_time = None;
    }
    
    pub fn reset(&mut self) {
        self.enabled = false;
        self.progress = 0.0;
        self.last_drawn_circle_progress = -1.0;
        self.last_drawn_check_progress = -1.0;
        self.animation_state = AnimationState::Idle;
    }
    
    pub fn is_complete(&self) -> bool {
        self.animation_state == AnimationState::Complete
    }
    
    fn record_pixels(&mut self) {
        let center = Point::new(self.size.width as i32 / 2, self.size.height as i32 / 2);
        let radius = (self.size.width.min(self.size.height) / 2 - 4) as i32;
        
        // Record the full circle and checkmark to get all pixels, then we'll draw them in order
        self.circle_pixels.clear();
        
        // Use PixelRecorder to capture circle
        let mut recorder = PixelRecorder::new();
        let circle_style = PrimitiveStyle::with_stroke(BinaryColor::On, 3);
        let _ = embedded_graphics::primitives::Circle::with_center(center, (radius * 2) as u32)
            .into_styled(circle_style)
            .draw(&mut recorder);
        
        // Sort circle pixels by angle to get smooth animation
        // We sort by angle starting from top and going clockwise
        let mut circle_pixels = recorder.pixels;
        circle_pixels.sort_by(|&a, &b| {
            let a_dx = a.x - center.x;
            let a_dy = a.y - center.y;
            let b_dx = b.x - center.x;
            let b_dy = b.y - center.y;
            
            // Determine quadrant (starting from top, going clockwise)
            // Quadrant 0: x >= 0, y < 0 (top-right)
            // Quadrant 1: x > 0, y >= 0 (bottom-right)
            // Quadrant 2: x <= 0, y > 0 (bottom-left)
            // Quadrant 3: x < 0, y <= 0 (top-left)
            let a_quad = match (a_dx >= 0, a_dy >= 0) {
                (true, false) => 0,  // top-right
                (true, true) => 1,   // bottom-right
                (false, true) => 2,  // bottom-left
                (false, false) => 3, // top-left
            };
            
            let b_quad = match (b_dx >= 0, b_dy >= 0) {
                (true, false) => 0,  // top-right
                (true, true) => 1,   // bottom-right
                (false, true) => 2,  // bottom-left
                (false, false) => 3, // top-left
            };
            
            // First compare by quadrant
            match a_quad.cmp(&b_quad) {
                core::cmp::Ordering::Equal => {
                    // Same quadrant, compare by angle within quadrant
                    // We can use the cross product to determine relative angle
                    let cross = (a_dx * b_dy) - (a_dy * b_dx);
                    if cross > 0 {
                        core::cmp::Ordering::Less
                    } else if cross < 0 {
                        core::cmp::Ordering::Greater
                    } else {
                        core::cmp::Ordering::Equal
                    }
                }
                other => other,
            }
        });
        
        self.circle_pixels = circle_pixels;
        
        // Record checkmark pixels
        self.check_pixels.clear();
        
        // Define checkmark points (made bigger relative to circle)
        let check_start = Point::new(
            center.x - (radius * 7 / 10),  // Was 5/10, now 7/10
            center.y
        );
        let check_middle = Point::new(
            center.x - (radius * 3 / 10),  // Was 2/10, now 3/10
            center.y + (radius * 7 / 10)   // Was 5/10, now 7/10
        );
        let check_end = Point::new(
            center.x + (radius * 8 / 10),  // Was 6/10, now 8/10
            center.y - (radius * 7 / 10)   // Was 5/10, now 7/10
        );
        
        // Use PixelRecorder to capture checkmark lines
        let mut recorder = PixelRecorder::new();
        let line_style = PrimitiveStyle::with_stroke(BinaryColor::On, 3);
        
        // Draw first line segment
        let _ = embedded_graphics::primitives::Line::new(check_start, check_middle)
            .into_styled(line_style)
            .draw(&mut recorder);
        
        let first_segment_pixels = recorder.pixels.clone();
        recorder.pixels.clear();
        
        // Draw second line segment  
        let _ = embedded_graphics::primitives::Line::new(check_middle, check_end)
            .into_styled(line_style)
            .draw(&mut recorder);
        
        // Sort pixels from each segment
        let mut first_sorted: Vec<Point> = first_segment_pixels;
        first_sorted.sort_by_key(|p| {
            // Sort by distance along the line
            let progress = ((p.x - check_start.x) as f32 / (check_middle.x - check_start.x) as f32)
                .max((p.y - check_start.y) as f32 / (check_middle.y - check_start.y) as f32);
            (progress * 1000.0) as i32
        });
        
        let mut second_sorted: Vec<Point> = recorder.pixels;
        second_sorted.sort_by_key(|p| {
            // Sort by distance along the line
            let progress = ((p.x - check_middle.x) as f32 / (check_end.x - check_middle.x) as f32)
                .max((p.y - check_middle.y) as f32 / (check_end.y - check_middle.y) as f32);
            (progress * 1000.0) as i32
        });
        
        // Combine sorted segments
        self.check_pixels = first_sorted;
        self.check_pixels.extend(second_sorted);
    }
    
    fn update_animation(&mut self, current_time: crate::Instant) {
        if !self.enabled {
            return;
        }
        
        // Animation durations in milliseconds
        const CIRCLE_DURATION_MS: u64 = 500;  // 0.5 seconds for circle
        const CHECK_DURATION_MS: u64 = 500;   // 0.5 seconds for checkmark
        
        // Initialize animation start time if not set
        if self.animation_start_time.is_none() {
            self.animation_start_time = Some(current_time);
            self.phase_start_time = Some(current_time);
        }
        
        match self.animation_state {
            AnimationState::Idle => {}
            AnimationState::DrawingCircle => {
                let phase_start = self.phase_start_time.unwrap();
                let elapsed = current_time.saturating_duration_since(phase_start);
                self.progress = (elapsed as f32 / CIRCLE_DURATION_MS as f32).min(1.0);
                
                if self.progress >= 1.0 {
                    self.progress = 0.0;
                    self.animation_state = AnimationState::DrawingCheck;
                    self.phase_start_time = Some(current_time);
                }
            }
            AnimationState::DrawingCheck => {
                let phase_start = self.phase_start_time.unwrap();
                let elapsed = current_time.saturating_duration_since(phase_start);
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
            AnimationState::DrawingCircle => {
                // Draw circle progress
                let current_pixels = (self.circle_pixels.len() as f32 * self.progress) as usize;
                let last_pixels = (self.circle_pixels.len() as f32 * self.last_drawn_circle_progress) as usize;
                
                if current_pixels > last_pixels {
                    target.draw_iter(
                        self.circle_pixels[last_pixels..current_pixels]
                            .iter()
                            .map(|&point| Pixel(point, color)),
                    )?;
                }
                
                self.last_drawn_circle_progress = self.progress;
            }
            AnimationState::DrawingCheck | AnimationState::Complete => {
                // Draw full circle if not already drawn
                if self.last_drawn_circle_progress < 1.0 {
                    target.draw_iter(
                        self.circle_pixels
                            .iter()
                            .map(|&point| Pixel(point, color)),
                    )?;
                    self.last_drawn_circle_progress = 1.0;
                }
                
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