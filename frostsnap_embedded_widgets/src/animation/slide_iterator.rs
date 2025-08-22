use core::marker::PhantomData;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlideDirection {
    Left,
    Right,
}

struct IteratorSegment<I> {
    iter: I,
    pixels_to_take: usize,
}

impl<I, C> SlideIterator<I, C>
where
    I: Iterator<Item = C>,
{
    /// Creates an iterator that slides between two framebuffer iterators
    ///
    /// The transition is going from current to next
    pub fn new(
        current_iter: I,
        next_iter: I,
        width: usize,
        next_pixels: usize,
        direction: SlideDirection,
    ) -> SlideIterator<I, C> {
        let current_pixels = width.saturating_sub(next_pixels);

        let mut first = IteratorSegment {
            iter: current_iter,
            pixels_to_take: current_pixels,
        };
        let mut second = IteratorSegment {
            iter: next_iter,
            pixels_to_take: next_pixels,
        };

        if direction == SlideDirection::Right {
            core::mem::swap(&mut first, &mut second);
        }

        // We have to shorten the row we're taking from first before we start emitting pixels
        if second.pixels_to_take > 0 {
            first.iter.nth(second.pixels_to_take - 1);
        }

        // Create a custom iterator
        SlideIterator {
            first,
            second,
            width,
            pixel_in_row: 0,
            ty_: PhantomData,
        }
    }

    /// Creates a transition where teh
    pub fn new_overlapping(
        current_iter: I,
        next_iter: I,
        width: usize,
        next_pixels: usize,
        direction: SlideDirection,
    ) -> SlideIterator<I, C> {
        let current_pixels = width.saturating_sub(next_pixels);

        let mut first = IteratorSegment {
            iter: current_iter,
            pixels_to_take: current_pixels,
        };
        let mut second = IteratorSegment {
            iter: next_iter,
            pixels_to_take: next_pixels,
        };

        if direction == SlideDirection::Right {
            core::mem::swap(&mut first, &mut second);
        }

        // Create a custom iterator
        SlideIterator {
            first,
            second,
            width,
            pixel_in_row: 0,
            ty_: PhantomData,
        }
    }
}

pub struct SlideIterator<I, C> {
    /// The iterator that has the first pixels in the row
    first: IteratorSegment<I>,
    /// The iterator that has the rest of the pixels for the row
    second: IteratorSegment<I>,
    /// The size of the row
    width: usize,
    /// The current pixel of the row
    pixel_in_row: usize,
    ty_: PhantomData<C>,
}

impl<I, C> Iterator for SlideIterator<I, C>
where
    I: Iterator<Item = C>,
{
    type Item = C;

    fn next(&mut self) -> Option<Self::Item> {
        let pixel = if self.pixel_in_row < self.first.pixels_to_take {
            self.first.iter.next()
        } else {
            self.second.iter.next()
        };

        self.pixel_in_row += 1;
        if self.pixel_in_row >= self.width {
            self.pixel_in_row = 0;
            if self.first.pixels_to_take > 0 {
                self.second.iter.nth(self.first.pixels_to_take - 1);
            }

            if self.second.pixels_to_take > 0 {
                self.first.iter.nth(self.second.pixels_to_take - 1)?;
            }
        }

        pixel
    }
}

// Type alias for compatibility
pub type HorizontalSlideIterator<I, C> = SlideIterator<I, C>;
