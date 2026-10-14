//! Test-only result card for camera-read hardware experiments (see `hwlab/` at the repo root).
//!
//! The card shows the experiment id and state, a heartbeat counter in large digits, and a QR
//! code holding `<run>|<STATE>|<beat>|<result>`. The host decodes the QR from a camera frame.

pub mod experiment;

use crate::peripherals::NoCs;
use crate::{Duration, Instant};
use core::fmt::{self, Write};
use core::sync::atomic::{AtomicU32, Ordering};
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyleBuilder},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use qrcodegen_no_heap::{QrCode, QrCodeEcc, Version};

/// The experiment id, set by the runner at build time.
pub const ID: &str = match option_env!("HWLAB_ID") {
    Some(id) => id,
    None => "dev",
};

/// The run id, set by the runner at build time so it can tell this build from a stale one.
pub const RUN: &str = match option_env!("HWLAB_RUN") {
    Some(run) => run,
    None => "0",
};

/// A numeric build parameter, passed by the runner as `HWLAB_<NAME>`.
pub const fn param(value: Option<&str>, default: u32) -> u32 {
    let Some(value) = value else { return default };
    let bytes = value.as_bytes();
    let mut n: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        assert!(bytes[i].is_ascii_digit(), "hwlab parameters must be digits");
        n = n * 10 + (bytes[i] - b'0') as u32;
        i += 1;
    }
    n
}

const MAX_VERSION: Version = Version::new(8);
const QR_BUF: usize = MAX_VERSION.buffer_len();
/// Fits version 8 at medium error correction (152 bytes).
const PAYLOAD_MAX: usize = 150;
const HEADER: Rectangle = Rectangle::new(Point::new(0, 0), Size::new(240, 22));
const COUNTER: Rectangle = Rectangle::new(Point::new(0, 22), Size::new(240, 40));
const QR_AREA: Rectangle = Rectangle::new(Point::new(0, 62), Size::new(240, 218));
const QUIET_ZONE: i32 = 2;

static BEAT: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy)]
enum State {
    Run,
    Done,
    Panic,
}

impl State {
    fn label(self) -> &'static str {
        match self {
            State::Run => "RUN",
            State::Done => "DONE",
            State::Panic => "PANIC",
        }
    }

    fn colour(self) -> Rgb565 {
        match self {
            State::Run => Rgb565::CSS_DARK_BLUE,
            State::Done => Rgb565::CSS_DARK_GREEN,
            State::Panic => Rgb565::RED,
        }
    }
}

/// The QR payload. Truncates instead of failing, and replaces anything outside printable ASCII.
struct Payload {
    buf: [u8; PAYLOAD_MAX],
    len: usize,
}

impl Payload {
    fn new() -> Self {
        Self {
            buf: [0; PAYLOAD_MAX],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Write for Payload {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if self.len == PAYLOAD_MAX {
                break;
            }
            self.buf[self.len] = match byte {
                b' '..=b'~' => byte,
                b'\n' => b' ',
                _ => b'?',
            };
            self.len += 1;
        }
        Ok(())
    }
}

/// Draws `inner` scaled up by an integer factor, for large digits from a small font.
struct Scaled<'d, D> {
    inner: &'d mut D,
    factor: u32,
}

impl<D: DrawTarget<Color = Rgb565>> DrawTarget for Scaled<'_, D> {
    type Color = Rgb565;
    type Error = D::Error;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), D::Error> {
        let k = self.factor;
        for Pixel(point, colour) in pixels {
            let origin = Point::new(point.x * k as i32, point.y * k as i32);
            self.inner
                .fill_solid(&Rectangle::new(origin, Size::new(k, k)), colour)?;
        }
        Ok(())
    }
}

impl<D: DrawTarget<Color = Rgb565>> OriginDimensions for Scaled<'_, D> {
    fn size(&self) -> Size {
        let size = self.inner.bounding_box().size;
        Size::new(size.width / self.factor, size.height / self.factor)
    }
}

fn draw_card<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    state: State,
    beat: u32,
    result: fmt::Arguments,
) {
    let mut payload = Payload::new();
    let _ = write!(payload, "{RUN}|{}|{beat}|{result}", state.label());

    let mut header = Payload::new();
    let _ = write!(header, "{} {ID}", state.label());
    let _ = display.fill_solid(&HEADER, state.colour());
    let header_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::WHITE)
        .background_color(state.colour())
        .build();
    let _ = Text::with_baseline(
        header.as_str(),
        Point::new(4, 1),
        header_style,
        Baseline::Top,
    )
    .draw(display);

    let mut counter = Payload::new();
    let _ = write!(counter, "{beat:>10}");
    let counter_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::BLACK)
        .background_color(Rgb565::WHITE)
        .build();
    let _ = display.fill_solid(&COUNTER, Rgb565::WHITE);
    let mut big = Scaled {
        inner: display,
        factor: 2,
    };
    let _ = Text::with_text_style(
        counter.as_str(),
        Point::new(COUNTER.center().x / 2, COUNTER.top_left.y / 2),
        counter_style,
        TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Top)
            .build(),
    )
    .draw(&mut big);

    let mut temp = [0u8; QR_BUF];
    let mut out = [0u8; QR_BUF];
    let qr = QrCode::encode_text(
        payload.as_str(),
        &mut temp,
        &mut out,
        QrCodeEcc::Medium,
        Version::MIN,
        MAX_VERSION,
        None,
        true,
    );
    let (modules, side) = match &qr {
        Ok(qr) => (qr.size(), qr.size() + 2 * QUIET_ZONE),
        Err(_) => (0, 1),
    };
    let scale = (QR_AREA.size.width.min(QR_AREA.size.height) as i32 / side).max(1);
    let offset =
        QR_AREA.center() - QR_AREA.top_left - Point::new(side * scale / 2, side * scale / 2);
    // One streamed write over the whole area, so a snap never sees a half-cleared code.
    let pixels = QR_AREA.points().map(|p| {
        let p = p - QR_AREA.top_left - offset;
        let (x, y) = (
            p.x.div_euclid(scale) - QUIET_ZONE,
            p.y.div_euclid(scale) - QUIET_ZONE,
        );
        let dark = match &qr {
            Ok(qr) => (0..modules).contains(&x) && (0..modules).contains(&y) && qr.get_module(x, y),
            Err(_) => false,
        };
        if dark {
            Rgb565::BLACK
        } else {
            Rgb565::WHITE
        }
    });
    let _ = display.fill_contiguous(&QR_AREA, pixels);
}

/// The experiment's view of the screen. Takes the display over from the UI.
pub struct Card<D> {
    display: D,
    period: Duration,
    last_draw: Option<Instant>,
}

impl Card<()> {
    /// Steals the display pins the way the panic handler does. Only call this after the UI has
    /// drawn its last frame, since the UI's display driver is never told.
    pub fn take() -> Card<impl DrawTarget<Color = Rgb565>> {
        use esp_hal::{
            delay::Delay,
            gpio::{Level, Output, OutputConfig},
            peripherals::Peripherals,
        };
        let peripherals = unsafe { Peripherals::steal() };
        let buffer = alloc::boxed::Box::leak(alloc::boxed::Box::new([0u8; 512]));
        let mut display = crate::init_display!(peripherals: peripherals, delay: &mut Delay::new(), buffer: buffer);
        let _ = display.clear(Rgb565::WHITE);
        core::mem::forget(Output::new(
            peripherals.GPIO1,
            Level::High,
            OutputConfig::default(),
        ));
        let period = Duration::from_millis(param(option_env!("HWLAB_BEAT_MS"), 1000) as u64);
        Card {
            display,
            period,
            last_draw: None,
        }
    }
}

impl<D: DrawTarget<Color = Rgb565>> Card<D> {
    /// Bumps the heartbeat and redraws with `result`, at most once per beat period, so it is
    /// cheap to call from a hot loop. Returns whether it drew.
    pub fn beat(&mut self, result: fmt::Arguments) -> bool {
        if self
            .last_draw
            .is_some_and(|last| last.elapsed() < self.period)
        {
            return false;
        }
        self.last_draw = Some(Instant::now());
        // No atomic read-modify-write on this core; single-threaded, so load and store.
        let beat = BEAT.load(Ordering::Relaxed) + 1;
        BEAT.store(beat, Ordering::Relaxed);
        draw_card(&mut self.display, State::Run, beat, result);
        true
    }

    /// Shows the final result and stops.
    pub fn done(mut self, result: fmt::Arguments) -> ! {
        let beat = BEAT.load(Ordering::Relaxed);
        draw_card(&mut self.display, State::Done, beat, result);
        #[allow(clippy::empty_loop)]
        loop {}
    }
}

/// Draws the panic card. Called from the panic handler, so it must not allocate.
pub fn panic_card(info: &core::panic::PanicInfo) -> ! {
    use esp_hal::{
        delay::Delay,
        gpio::{Level, Output, OutputConfig},
        peripherals::Peripherals,
    };
    let peripherals = unsafe { Peripherals::steal() };
    let mut buffer = [0u8; 64];
    let mut display = crate::init_display!(peripherals: peripherals, delay: &mut Delay::new(), buffer: &mut buffer);
    let _ = display.clear(Rgb565::WHITE);
    let beat = BEAT.load(Ordering::Relaxed);
    match info.location() {
        Some(location) => draw_card(
            &mut display,
            State::Panic,
            beat,
            format_args!(
                "{}:{} {}",
                location.file().split('/').next_back().unwrap_or(""),
                location.line(),
                info.message()
            ),
        ),
        None => draw_card(
            &mut display,
            State::Panic,
            beat,
            format_args!("{}", info.message()),
        ),
    }
    let _bl = Output::new(peripherals.GPIO1, Level::High, OutputConfig::default());
    #[allow(clippy::empty_loop)]
    loop {}
}
