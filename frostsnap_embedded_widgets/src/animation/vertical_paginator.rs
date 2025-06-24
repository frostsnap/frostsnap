use crate::{Widget, PageByPage};
use alloc::boxed::Box;
use embedded_graphics::{
    draw_target::DrawTarget, framebuffer::Framebuffer, iterator::raw::RawDataSlice, pixelcolor::raw::LittleEndian, prelude::*, primitives::Rectangle
};

const ANIMATION_DURATION_MS: u64 = 700;
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug)]
struct Animation {
    start_time: Option<crate::Instant>,
    direction: Direction,
}

pub struct VerticalPaginator<W, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize>
    where W: PageByPage
{
    current_fb: Box<Framebuffer<W::Color, <<W as Widget>::Color as PixelColor>::Raw, LittleEndian,  WIDTH, HEIGHT, BUFFER_SIZE>>,
    next_fb: Box<Framebuffer<W::Color, <<W as Widget>::Color as PixelColor>::Raw, LittleEndian, WIDTH, HEIGHT, BUFFER_SIZE>>,
    animation: Option<Animation>,
    pub child: W,
    force_redraw: bool,
    drag_start: Option<u32>,
}

impl<W, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize> VerticalPaginator<W, WIDTH, HEIGHT, BUFFER_SIZE>
    where W: PageByPage,
          W::Color: PixelColor + Default,
          <W::Color as PixelColor>::Raw: Into<W::Color>,

    Framebuffer<W::Color, <<W as Widget>::Color as PixelColor>::Raw, LittleEndian, WIDTH, HEIGHT, BUFFER_SIZE>: DrawTarget<Color=W::Color>,
{
    pub fn new(child: W) -> Self {
        let mut paginator = Self {
            current_fb: Box::new(Framebuffer::new()),
            next_fb: Box::new(Framebuffer::new()),
            animation: None,
            child,
            force_redraw: true,
            drag_start: None,
        };
        
        // Draw initial page to current framebuffer
        paginator.draw_current_page();
        
        paginator
    }

    fn area() -> Rectangle {
        Rectangle::new(Point::default(), Size { width: WIDTH as u32, height: HEIGHT as u32 })
    }

    fn draw_current_page(&mut self) {
        let _ = DrawTarget::clear(&mut *self.current_fb, W::Color::default());
        let _ = self.child.draw(&mut *self.current_fb, crate::Instant::from_millis(0));
    }

    fn draw_next_page(&mut self) {
        let _ = self.next_fb.clear(W::Color::default());
        let _ = self.child.draw(&mut *self.next_fb, crate::Instant::from_millis(0));
    }

    fn start_transition(&mut self, direction: Direction) {
        if self.animation.is_some() {
            return;
        }

        // Check if transition is allowed
        let can_transition = match direction {
            Direction::Up => self.child.has_next_page(),
            Direction::Down => self.child.has_prev_page(),
        };

        if !can_transition {
            return;
        }

        // Update child based on direction
        match direction {
            Direction::Up => self.child.next_page(),
            Direction::Down => self.child.prev_page(),
        }
        
        // Draw the next page
        self.draw_next_page();

        self.animation = Some(Animation {
            start_time: None,
            direction,
        });
    }

}

impl<W, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize> Widget for VerticalPaginator<W, WIDTH, HEIGHT, BUFFER_SIZE>
    where W: PageByPage,
          W::Color: PixelColor + Default,
    Framebuffer<W::Color, <W::Color as PixelColor>::Raw, LittleEndian, WIDTH, HEIGHT, BUFFER_SIZE>: DrawTarget<Color=W::Color>,
    <W::Color as PixelColor>::Raw: Into<W::Color>,
    for<'a> RawDataSlice<'a, <W::Color as PixelColor>::Raw, LittleEndian>: IntoIterator<Item=<W::Color as PixelColor>::Raw>,
{
    type Color = W::Color;

    fn draw<D: DrawTarget<Color = Self::Color>>(
        &mut self,
        target: &mut D,
        current_time: crate::Instant,
    ) -> Result<(), D::Error> {
        // Initialize animation start time if needed
        if let Some(ref mut anim) = self.animation {
            if anim.start_time.is_none() {
                anim.start_time = Some(current_time);
            }
        } else if !self.force_redraw {
            return Ok(());
        }

        // Calculate animation progress (0 to 1000 representing 0.0 to 1.0)
        let progress_millis = match &self.animation {
            Some(anim) => {
                let elapsed = current_time
                    .duration_since(anim.start_time.unwrap())
                    .unwrap_or(0)
                    .min(ANIMATION_DURATION_MS);
                ((elapsed * 1000) / ANIMATION_DURATION_MS) as usize
            }
            None => 0,
        };

        // Get direction from animation
        let direction = self.animation.as_ref().map(|a| a.direction).unwrap_or(Direction::Up);

        // Calculate rows for vertical transition using integer arithmetic
        // transition_rows = HEIGHT * progress_millis / 1000
        let transition_rows = (HEIGHT * progress_millis) / 1000;
        let transition_pixels = transition_rows * WIDTH;
        let left_over_pixels = (HEIGHT - transition_rows) * WIDTH;

         // Create iterators based on skip/take values
        let current_iter = RawDataSlice::<<Self::Color as PixelColor>::Raw, LittleEndian>::new(self.current_fb.data())
            .into_iter().map(|c| c.into());

        let next_iter = RawDataSlice::<<Self::Color as PixelColor>::Raw, LittleEndian>::new(self.next_fb.data())
            .into_iter().map(|c| c.into());

        // Determine skip/take values based on direction
        match direction {
            Direction::Up => {
                let pixel_iter = current_iter.skip(transition_pixels).take(left_over_pixels).chain(next_iter.take(transition_pixels));
                target.fill_contiguous(&Self::area(), pixel_iter)?;
            }
            Direction::Down => {
                let pixel_iter = next_iter.skip(left_over_pixels).take(transition_pixels).chain(current_iter.take(left_over_pixels));
                target.fill_contiguous(&Self::area(), pixel_iter)?;
            }
        };

        // Handle animation completion
        if progress_millis >= 1000 && self.animation.is_some() {
            core::mem::swap(&mut self.current_fb, &mut self.next_fb);
            self.animation = None;
        }

        self.force_redraw = false;

        Ok(())
    }
    
    fn handle_touch(
        &mut self,
        point: Point,
        current_time: crate::Instant,
        is_release: bool,
    ) -> Option<crate::KeyTouch> {
        // Pass through to child widget
        self.child.handle_touch(point, current_time, is_release)
    }
    
    fn handle_vertical_drag(&mut self, _prev_y: Option<u32>, new_y: u32, is_release: bool) {
        if is_release {
            if let Some(drag_start) = self.drag_start.take() {
                if new_y > drag_start {
                    self.start_transition(Direction::Down);
                } else if drag_start > new_y {
                    self.start_transition(Direction::Up);
                }
            }

        } else {
            if self.drag_start.is_none() {
                self.drag_start = Some(new_y);
            }
        }

    }
    
    fn size_hint(&self) -> Option<Size> {
        Some(Self::area().size)
    }

    fn force_full_redraw(&mut self) {
        self.force_redraw = true;
    }
}

impl<W, const WIDTH: usize, const HEIGHT: usize, const BUFFER_SIZE: usize> PageByPage for VerticalPaginator<W, WIDTH, HEIGHT, BUFFER_SIZE>
where
    W: PageByPage,
    W::Color: PixelColor + Default,
    <W::Color as PixelColor>::Raw: Into<W::Color>,
    Framebuffer<W::Color, <<W as Widget>::Color as PixelColor>::Raw, LittleEndian, WIDTH, HEIGHT, BUFFER_SIZE>: DrawTarget<Color=W::Color>,
    for<'a> RawDataSlice<'a, <W::Color as PixelColor>::Raw, LittleEndian>: IntoIterator<Item=<W::Color as PixelColor>::Raw>,
{
    fn has_next_page(&self) -> bool {
        self.child.has_next_page()
    }
    
    fn has_prev_page(&self) -> bool {
        self.child.has_prev_page()
    }
    
    fn next_page(&mut self) {
        self.start_transition(Direction::Up);
    }
    
    fn prev_page(&mut self) {
        self.start_transition(Direction::Down);
    }
    
    fn current_page(&self) -> usize {
        self.child.current_page()
    }
    
    fn total_pages(&self) -> usize {
        self.child.total_pages()
    }
    
    fn is_transitioning(&self) -> bool {
        self.animation.is_some()
    }
}
