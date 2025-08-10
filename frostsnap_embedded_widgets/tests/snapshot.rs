use frostsnap_embedded_widgets::{
    snapshot::Snapshot,
    vec_framebuffer::VecFramebuffer,
    Widget, DynWidget, Sizing, Instant, KeyTouch,
};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle},
};

// A test widget that counts how many times it's been drawn
struct CountingWidget {
    draw_count: usize,
}

impl DynWidget for CountingWidget {
    fn sizing(&self) -> Sizing {
        Sizing { width: 100, height: 100 }
    }
    
    fn handle_touch(&mut self, _: Point, _: Instant, _: bool) -> Option<KeyTouch> {
        None
    }
    
    fn handle_vertical_drag(&mut self, _: Option<u32>, _: u32, _: bool) {}
    
    fn force_full_redraw(&mut self) {}
}

impl Widget for CountingWidget {
    type Color = Rgb565;
    
    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        _: Instant,
    ) -> Result<(), D::Error> {
        self.draw_count += 1;
        
        // Draw something
        Circle::new(Point::new(50, 50), 30)
            .into_styled(PrimitiveStyle::with_fill(Rgb565::RED))
            .draw(target)?;
            
        Ok(())
    }
}

#[test]
fn test_snapshot_caching() {
    let counting_widget = CountingWidget { draw_count: 0 };
    let mut snapshot = Snapshot::new(counting_widget);
    
    // Create a target to draw to
    let mut target = VecFramebuffer::<Rgb565>::new(100, 100);
    let current_time = Instant::from_millis(0);
    
    // First draw should render the child
    snapshot.draw(&mut target, current_time).unwrap();
    assert_eq!(snapshot.child().draw_count, 1);
    
    // Second draw should use cached version
    snapshot.draw(&mut target, current_time).unwrap();
    assert_eq!(snapshot.child().draw_count, 1); // Still 1!
    
    // force_full_redraw should NOT cause child to redraw
    snapshot.force_full_redraw();
    snapshot.draw(&mut target, current_time).unwrap();
    assert_eq!(snapshot.child().draw_count, 1); // Still 1!
    
    // After retake, should draw again
    snapshot.retake();
    snapshot.draw(&mut target, current_time).unwrap();
    assert_eq!(snapshot.child().draw_count, 2);
}

#[test]
fn test_snapshot_resizing() {
    struct ResizableWidget {
        size: Sizing,
    }
    
    impl DynWidget for ResizableWidget {
        fn sizing(&self) -> Sizing {
            self.size
        }
        
        fn handle_touch(&mut self, _: Point, _: Instant, _: bool) -> Option<KeyTouch> {
            None
        }
        
        fn handle_vertical_drag(&mut self, _: Option<u32>, _: u32, _: bool) {}
        
        fn force_full_redraw(&mut self) {}
    }
    
    impl Widget for ResizableWidget {
        type Color = Rgb565;
        
        fn draw<D: DrawTarget<Color = Self::Color>>(
            &mut self,
            target: &mut D,
            _: Instant,
        ) -> Result<(), D::Error> {
            // Fill with a color based on size
            let color = if self.size.width > 50 {
                Rgb565::RED
            } else {
                Rgb565::BLUE
            };
            
            embedded_graphics::primitives::Rectangle::new(
                Point::zero(),
                Size::new(self.size.width, self.size.height)
            )
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(target)
        }
    }
    
    let widget = ResizableWidget {
        size: Sizing { width: 30, height: 30 },
    };
    let mut snapshot = Snapshot::new(widget);
    
    let mut target = VecFramebuffer::<Rgb565>::new(100, 100);
    let current_time = Instant::from_millis(0);
    
    // Draw with initial size
    snapshot.draw(&mut target, current_time).unwrap();
    
    // Change the size of the child widget
    snapshot.child_mut().size = Sizing { width: 60, height: 60 };
    
    // Retake to update the cached framebuffer
    snapshot.retake();
    snapshot.draw(&mut target, current_time).unwrap();
    
    // The snapshot should now have the new size
    assert_eq!(snapshot.sizing().width, 60);
    assert_eq!(snapshot.sizing().height, 60);
}