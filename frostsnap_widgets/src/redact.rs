//! Data a screen shows that the app's demo of it must not invite anyone to read.
//!
//! The app plays the device's own screens to show how to work the device. Amounts, addresses and
//! codes there are placeholders, and a sharp placeholder looks like something to check.
//!
//! A `Redact` blurs only when built on a thread inside [`enable_on_this_thread`]: the app also
//! draws real device screens (the simulator's), and those must stay readable. Without the `redact`
//! feature (firmware) `Redact` is the child itself.

#[cfg(not(feature = "redact"))]
pub type Redact<W> = W;

#[cfg(not(feature = "redact"))]
#[inline(always)]
pub fn redact<W>(child: W) -> Redact<W> {
    child
}

#[cfg(feature = "redact")]
pub use blurred::Redact;

#[cfg(feature = "redact")]
std::thread_local! {
    static ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Every `Redact` built on this thread until the guard drops blurs its child.
#[cfg(feature = "redact")]
#[must_use]
pub fn enable_on_this_thread() -> Enabled {
    Enabled {
        was: ENABLED.replace(true),
        on_this_thread: core::marker::PhantomData,
    }
}

#[cfg(feature = "redact")]
pub struct Enabled {
    was: bool,
    // Dropped on another thread, it would restore that thread's switch and leave this one on.
    on_this_thread: core::marker::PhantomData<*const ()>,
}

#[cfg(feature = "redact")]
impl Drop for Enabled {
    fn drop(&mut self) {
        ENABLED.set(self.was);
    }
}

#[cfg(feature = "redact")]
pub fn redact<W: crate::Widget<Color = embedded_graphics::pixelcolor::Rgb565>>(
    child: W,
) -> Redact<W> {
    Redact::new(child)
}

#[cfg(feature = "redact")]
mod blurred {
    use crate::{vec_framebuffer::VecFramebuffer, DynWidget, Sizing, SuperDrawTarget, Widget};
    use alloc::vec::Vec;
    use embedded_graphics::{
        draw_target::DrawTarget,
        geometry::{Point, Size},
        pixelcolor::{Rgb565, RgbColor},
    };

    const RADIUS: usize = 6;
    const PASSES: usize = 3;

    pub struct Redact<W> {
        child: W,
        blur: bool,
        sharp: VecFramebuffer<Rgb565>,
        blurred: VecFramebuffer<Rgb565>,
        offset: Point,
        needs_reblit: bool,
    }

    impl<W: Clone> Clone for Redact<W> {
        fn clone(&self) -> Self {
            Self {
                child: self.child.clone(),
                blur: self.blur,
                sharp: self.sharp.clone(),
                blurred: self.blurred.clone(),
                offset: self.offset,
                needs_reblit: self.needs_reblit,
            }
        }
    }

    // Derefs to the child so code driving it reads the same with or without the feature, when
    // `Redact<W>` is just `W`.
    impl<W> core::ops::Deref for Redact<W> {
        type Target = W;

        fn deref(&self) -> &W {
            &self.child
        }
    }

    impl<W> core::ops::DerefMut for Redact<W> {
        fn deref_mut(&mut self) -> &mut W {
            &mut self.child
        }
    }

    impl<W: Widget<Color = Rgb565>> Redact<W> {
        pub fn new(child: W) -> Self {
            Self {
                child,
                blur: super::ENABLED.get(),
                sharp: VecFramebuffer::new(0, 0),
                blurred: VecFramebuffer::new(0, 0),
                offset: Point::zero(),
                needs_reblit: true,
            }
        }
    }

    impl<W: Widget<Color = Rgb565>> DynWidget for Redact<W> {
        fn set_constraints(&mut self, max_size: Size) {
            self.child.set_constraints(max_size);
            if !self.blur {
                return;
            }
            let rect = self.child.sizing().dirty_rect();
            self.offset = rect.top_left;
            let (w, h) = (rect.size.width as usize, rect.size.height as usize);
            if self.sharp.width() != w || self.sharp.height() != h {
                self.sharp = VecFramebuffer::new(w, h);
                self.blurred = VecFramebuffer::new(w, h);
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
            self.child.force_full_redraw();
            self.needs_reblit = true;
        }
    }

    impl<W: Widget<Color = Rgb565>> Widget for Redact<W> {
        type Color = Rgb565;

        fn draw<D>(
            &mut self,
            target: &mut SuperDrawTarget<D, Rgb565>,
            current_time: crate::Instant,
        ) -> Result<(), D::Error>
        where
            D: DrawTarget<Color = Rgb565>,
        {
            if !self.blur {
                return self.child.draw(target, current_time);
            }
            let background = target.background_color();
            if self.needs_reblit {
                // A child's partial redraw assumes what it drew last is still there, and that
                // copy was blurred away; only a clean slate makes its full redraw complete.
                self.sharp.clear(background);
            }
            let mut sharp =
                SuperDrawTarget::new(&mut self.sharp, background).translate(-self.offset);
            self.child.draw(&mut sharp, current_time).unwrap();
            drop(sharp);

            if self.sharp.take_dirty() {
                blur(&self.sharp, &mut self.blurred, background);
                self.needs_reblit = true;
            }
            if self.needs_reblit {
                self.blurred.blit(target, self.offset)?;
                self.needs_reblit = false;
            }
            Ok(())
        }
    }

    /// Repeated box blurs approximate a gaussian. Outside the box counts as background, so the
    /// content fades out towards its edges instead of being cut off by them.
    fn blur(src: &VecFramebuffer<Rgb565>, dst: &mut VecFramebuffer<Rgb565>, bg: Rgb565) {
        let (w, h) = (src.width(), src.height());
        let bg = [bg.r() as u32, bg.g() as u32, bg.b() as u32];
        let mut channels: [Vec<u32>; 3] = core::array::from_fn(|_| Vec::with_capacity(w * h));
        for color in src.contiguous_pixels() {
            channels[0].push(color.r() as u32);
            channels[1].push(color.g() as u32);
            channels[2].push(color.b() as u32);
        }
        let mut scratch = vec![0u32; w * h];
        for (channel, &edge) in channels.iter_mut().zip(bg.iter()) {
            for _ in 0..PASSES {
                box_pass(channel, &mut scratch, w, h, 1, w, edge);
                box_pass(&scratch, channel, h, w, w, 1, edge);
            }
        }
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                dst.set_pixel(
                    Point::new(x as i32, y as i32),
                    Rgb565::new(
                        channels[0][i] as u8,
                        channels[1][i] as u8,
                        channels[2][i] as u8,
                    ),
                );
            }
        }
    }

    /// One box blur along lines of `len` pixels `step` apart; `lines` lines start `line_step`
    /// apart.
    fn box_pass(
        src: &[u32],
        dst: &mut [u32],
        len: usize,
        lines: usize,
        step: usize,
        line_step: usize,
        edge: u32,
    ) {
        let window = (2 * RADIUS + 1) as u32;
        let at = |line: usize, i: isize| -> u32 {
            if i < 0 || i as usize >= len {
                edge
            } else {
                src[line * line_step + i as usize * step]
            }
        };
        for line in 0..lines {
            let mut sum: u32 = (-(RADIUS as isize)..=RADIUS as isize)
                .map(|i| at(line, i))
                .sum();
            for i in 0..len {
                dst[line * line_step + i * step] = (sum + window / 2) / window;
                sum += at(line, (i + RADIUS + 1) as isize);
                sum -= at(line, i as isize - RADIUS as isize);
            }
        }
    }
}
