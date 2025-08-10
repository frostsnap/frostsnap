//! Snapshot widget that caches another widget's rendered output

use crate::{Widget, DynWidget, Sizing, Instant, KeyTouch, vec_framebuffer::VecFramebuffer};
use embedded_graphics::{
    prelude::*,
    image::{Image, ImageRaw, ImageDrawable},
    pixelcolor::raw::LittleEndian,
    primitives::Rectangle,
    Drawable,
};
use core::marker::PhantomData;

/// A widget that renders its child once to a framebuffer and then
/// efficiently reuses that cached rendering
pub struct Snapshot<W, C> 
where
    W: Widget<Color = C>,
    C: PixelColor + From<<C as PixelColor>::Raw>,
    VecFramebuffer<C>: DrawTarget<Color = C>,
{
    /// The child widget to render
    child: W,
    /// The cached framebuffer
    framebuffer: Option<VecFramebuffer<C>>,
    /// Whether we need to redraw ourselves (not the child)
    needs_redraw: bool,
    /// Phantom data for the color type
    _phantom: PhantomData<C>,
}

impl<W, C> Snapshot<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor + From<<C as PixelColor>::Raw>,
    VecFramebuffer<C>: DrawTarget<Color = C>,
{
    /// Create a new Snapshot widget
    pub fn new(child: W) -> Self {
        Self {
            child,
            framebuffer: None,
            needs_redraw: true,
            _phantom: PhantomData,
        }
    }
    
    /// Force the child to redraw and recapture its output
    /// This will call force_full_redraw on the child and re-render it
    pub fn retake(&mut self) {
        // Force the child to redraw
        self.child.force_full_redraw();
        // Clear our framebuffer to force re-rendering
        self.framebuffer = None;
        self.needs_redraw = true;
    }
    
    /// Get a reference to the child widget
    pub fn child(&self) -> &W {
        &self.child
    }
    
    /// Get a mutable reference to the child widget
    pub fn child_mut(&mut self) -> &mut W {
        &mut self.child
    }
    
    /// Ensure the framebuffer exists and is the right size
    fn ensure_framebuffer(&mut self) {
        let sizing = self.child.sizing();
        
        // Check if we need to create or resize the framebuffer
        let needs_new_buffer = match &self.framebuffer {
            None => true,
            Some(fb) => fb.width != sizing.width as usize || fb.height != sizing.height as usize,
        };
        
        if needs_new_buffer {
            self.framebuffer = Some(VecFramebuffer::new(
                sizing.width as usize,
                sizing.height as usize,
            ));
            self.needs_redraw = true;
        }
    }
    
    /// Render the child to the framebuffer if needed
    fn render_to_framebuffer(&mut self, current_time: Instant)
    where
        C: Default,
    {
        self.ensure_framebuffer();
        
        if let Some(fb) = &mut self.framebuffer {
            // Clear the framebuffer first using DrawTarget's clear method
            DrawTarget::clear(fb, C::default()).ok();
            
            // Draw the child widget to the framebuffer
            self.child.draw(fb, current_time).ok();
        }
    }
}

impl<W, C> DynWidget for Snapshot<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor + From<<C as PixelColor>::Raw>,
    VecFramebuffer<C>: DrawTarget<Color = C>,
{
    fn set_constraints(&mut self, max_size: Size) {
        self.child.set_constraints(max_size);
    }
    
    fn sizing(&self) -> Sizing {
        self.child.sizing()
    }
    
    fn flex(&self) -> bool {
        self.child.flex()
    }
    
    fn handle_touch(&mut self, point: Point, current_time: Instant, is_release: bool) -> Option<KeyTouch> {
        self.child.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, prev_y: Option<u32>, new_y: u32, is_release: bool) {
        self.child.handle_vertical_drag(prev_y, new_y, is_release)
    }
    
    fn force_full_redraw(&mut self) {
        // We only mark ourselves as needing redraw
        // We do NOT forward this to the child - that's the whole point of caching!
        self.needs_redraw = true;
    }
}

impl<W, C> Widget for Snapshot<W, C>
where
    W: Widget<Color = C>,
    C: PixelColor + From<<C as PixelColor>::Raw> + Default,
    VecFramebuffer<C>: DrawTarget<Color = C>,
    for<'a> ImageRaw<'a, C, LittleEndian>: ImageDrawable<Color = C>,
{
    type Color = C;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: Instant,
    ) -> Result<(), D::Error> {
        // Only render to framebuffer if we don't have one yet
        if self.framebuffer.is_none() {
            self.render_to_framebuffer(current_time);
        }
        
        // Draw the cached framebuffer content
        if self.needs_redraw {
            if let Some(fb) = &self.framebuffer {
                // Use fill_contiguous for efficient drawing
                let area = Rectangle::new(
                    Point::zero(),
                    Size::new(fb.width as u32, fb.height as u32)
                );
                
                // Create an ImageRaw from the framebuffer data
                let raw_image = ImageRaw::<C, LittleEndian>::new(&fb.data, fb.width as u32);
                let image = Image::new(&raw_image, Point::zero());
                image.draw(target)?;
                
                self.needs_redraw = false;
            }
        }
        
        Ok(())
    }
}

