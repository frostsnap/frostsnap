use crate::{vec_framebuffer::VecFramebuffer, DynWidget, Sizing, SuperDrawTarget, Widget};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
};

pub struct FrameCache<W: Widget> {
    child: W,
    framebuffer: VecFramebuffer<W::Color>,
    needs_reblit: bool,
    dirty_rect_offset: Point,
}

impl<W: Widget + Clone> Clone for FrameCache<W>
where
    W::Color: Copy,
{
    fn clone(&self) -> Self {
        Self {
            child: self.child.clone(),
            framebuffer: self.framebuffer.clone(),
            needs_reblit: self.needs_reblit,
            dirty_rect_offset: self.dirty_rect_offset,
        }
    }
}

impl<W: Widget> FrameCache<W>
where
    W::Color: Copy,
{
    pub fn new(child: W) -> Self {
        Self {
            child,
            framebuffer: VecFramebuffer::new(0, 0),
            needs_reblit: true,
            dirty_rect_offset: Point::zero(),
        }
    }

    pub fn child(&self) -> &W {
        &self.child
    }

    pub fn child_mut(&mut self) -> &mut W {
        &mut self.child
    }
}

impl<W: Widget> DynWidget for FrameCache<W>
where
    W::Color: Copy,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.child.set_constraints(max_size);
        let dirty_rect = self.child.sizing().dirty_rect();
        self.dirty_rect_offset = dirty_rect.top_left;
        let w = dirty_rect.size.width as usize;
        let h = dirty_rect.size.height as usize;
        if self.framebuffer.width() != w || self.framebuffer.height() != h {
            self.framebuffer = VecFramebuffer::new(w, h);
            self.needs_reblit = true;
        }
    }

    fn sizing(&self) -> Sizing {
        self.child.sizing()
    }

    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        lift_up: bool,
    ) -> Option<crate::KeyTouch> {
        self.child.handle_touch(point, current_time, lift_up)
    }

    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, is_release);
    }

    fn force_full_redraw(&mut self) {
        // ⛔ Don't forward — just reblit the cached frame
        self.needs_reblit = true;
    }
}

impl<W: Widget> Widget for FrameCache<W>
where
    W::Color: Copy,
{
    type Color = W::Color;

    fn draw<D>(
        &mut self,
        target: &mut SuperDrawTarget<D, Self::Color>,
        current_time: crate::Instant,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let mut fb_target = SuperDrawTarget::new(&mut self.framebuffer, target.background_color())
            .translate(-self.dirty_rect_offset);
        self.child.draw(&mut fb_target, current_time).unwrap();
        drop(fb_target);

        if self.needs_reblit || self.framebuffer.take_dirty() {
            self.framebuffer.blit(target, self.dirty_rect_offset)?;
            self.needs_reblit = false;
        }
        Ok(())
    }
}
