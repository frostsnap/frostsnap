use crate::{
    hold_to_confirm_border::HoldToConfirmBorder, Widget,
    Instant,
};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Point,
    pixelcolor::BinaryColor,
    prelude::*,
};

pub struct HoldToConfirmButton<W> {
    hold_to_confirm: HoldToConfirmBorder<ButtonWrapper<W>>,
}

struct ButtonWrapper<W> {
    size: Size,
    child: W,
}

impl<W: Widget<Color = BinaryColor>> Widget for ButtonWrapper<W> {
    type Color = BinaryColor;
    
    fn draw<D: DrawTarget<Color = BinaryColor>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.child.draw(target, current_time)
    }
    
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, start_y: Option<u32>, current_y: u32) {
        self.child.handle_vertical_drag(start_y, current_y)
    }
    
    fn size_hint(&self) -> Option<Size> {
        Some(self.size)
    }
}

impl<W: Widget<Color = BinaryColor>> HoldToConfirmButton<W> {
    pub fn new(size: Size, child: W, hold_duration_ms: f64) -> Self {
        let wrapper = ButtonWrapper { size, child };
        let hold_to_confirm = HoldToConfirmBorder::new(wrapper, hold_duration_ms as f32);
        
        Self {
            hold_to_confirm,
        }
    }
    
    pub fn enable(&mut self) {
        self.hold_to_confirm.enable();
    }
    
    pub fn disable(&mut self) {
        self.hold_to_confirm.disable();
    }
    
    pub fn is_enabled(&self) -> bool {
        self.hold_to_confirm.is_enabled()
    }
    
    pub fn is_completed(&self) -> bool {
        self.hold_to_confirm.is_completed()
    }
    
    pub fn reset(&mut self) {
        self.hold_to_confirm.reset();
    }
}

impl<W: Widget<Color = BinaryColor>> Widget for HoldToConfirmButton<W> {
    type Color = BinaryColor;
    
    fn draw<D: DrawTarget<Color = BinaryColor>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        self.hold_to_confirm.draw(target, current_time)
    }
    
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<crate::KeyTouch> {
        self.hold_to_confirm.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, _start_y: Option<u32>, _current_y: u32) {
        // Buttons don't respond to drags
    }
    
    fn size_hint(&self) -> Option<Size> {
        self.hold_to_confirm.size_hint()
    }
}