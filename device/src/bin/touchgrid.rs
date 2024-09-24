#![no_std]
#![no_main]

use core::mem::MaybeUninit;
use cst816s::CST816S;
use display_interface_spi::SPIInterface;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyleBuilder, Rectangle},
};
use esp_hal::{
    clock::ClockControl,
    delay::Delay,
    entry,
    gpio::{Input, Io, Level, Output, Pull},
    i2c::I2C,
    peripherals::Peripherals,
    prelude::*,
    spi::{master::Spi, SpiMode},
    system::SystemControl,
    timer::timg::TimerGroup,
};
use frostsnap_device::graphics;
use fugit::Duration;
use mipidsi::{models::ST7789, options::ColorInversion};

#[global_allocator]
static ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap() {
    const HEAP_SIZE: usize = 128 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        ALLOCATOR.init(HEAP.as_mut_ptr() as *mut u8, HEAP_SIZE);
    }
}

#[entry]
fn main() -> ! {
    init_heap();
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::max(system.clock_control).freeze();
    let timg = TimerGroup::new(peripherals.TIMG0, &clocks, None);
    let timer = timg.timer0;

    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    let mut delay = Delay::new(&clocks);

    // Initialize SPI for the display
    let spi = Spi::new(peripherals.SPI2, 80u32.MHz(), SpiMode::Mode2, &clocks)
        .with_sck(io.pins.gpio8)
        .with_mosi(io.pins.gpio7);
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
    let di = SPIInterface::new(spi_device, Output::new(io.pins.gpio9, Level::Low));
    let mut display = mipidsi::Builder::new(ST7789, di)
        .display_size(240, 280)
        .display_offset(0, 20) // 240x280 panel
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(io.pins.gpio6, Level::Low))
        .init(&mut delay)
        .unwrap();

    // Define grid properties and colors
    let grid_spacing = 30;
    let grid_colors = [
        Rgb565::RED,
        Rgb565::GREEN,
        Rgb565::BLUE,
        Rgb565::YELLOW,
        Rgb565::MAGENTA,
        Rgb565::CSS_AQUAMARINE,
        Rgb565::CSS_HOT_PINK,
    ];
    let mut current_color = 0; // Tracks the current draw color
    let default_color = Rgb565::BLACK;
    let grid_line_color = Rgb565::WHITE;

    let highlight_square = (0, 8);

    // Draw the grid with white lines and the highlighted square
    for x in 0..8 {
        for y in 0..9 {
            let square_color = if (x, y) == highlight_square {
                grid_colors[current_color]
            } else {
                default_color
            };

            // Fill the grid squares
            Rectangle::new(
                Point::new(x * grid_spacing as i32, y * grid_spacing as i32),
                Size::new(grid_spacing as u32, grid_spacing as u32),
            )
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(square_color)
                    .build(),
            )
            .draw(&mut display)
            .unwrap();
        }
    }

    // Draw the grid lines
    for x in (0..240).step_by(grid_spacing) {
        Rectangle::new(Point::new(x, 0), Size::new(1, 280))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(grid_line_color)
                    .build(),
            )
            .draw(&mut display)
            .unwrap();
    }
    for y in (0..280).step_by(grid_spacing) {
        Rectangle::new(Point::new(0, y), Size::new(240, 1))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(grid_line_color)
                    .build(),
            )
            .draw(&mut display)
            .unwrap();
    }

    // Initialize I2C for CST816S
    let i2c = I2C::new(
        peripherals.I2C0,
        io.pins.gpio4,
        io.pins.gpio5,
        400u32.kHz(),
        &clocks,
        None,
    );
    let mut capsense = CST816S::new(
        i2c,
        Input::new(io.pins.gpio2, Pull::Down),
        Output::new(io.pins.gpio3, Level::Low),
    );
    capsense.setup(&mut delay).unwrap();
    let mut last_color_change = timer.now();
    let color_change_break_duration: Duration<u64, 1, 1_000_000> = 700.millis();
    // Main loop: detect touch events and handle color changes
    loop {
        if let Some(touch_event) = capsense.read_one_touch_event(true) {
            let (x, y) = (
                touch_event.x / grid_spacing as i32,
                touch_event.y / grid_spacing as i32,
            );
            let now = timer.now();

            // Check if the touch is within the highlighted square
            if x == highlight_square.0
                && y == highlight_square.1
                && now.checked_duration_since(last_color_change).unwrap()
                    > color_change_break_duration
            {
                last_color_change = now;
                // Cycle to the next color
                current_color = (current_color + 1) % grid_colors.len();

                // Redraw the highlight square with the next draw color
                Rectangle::new(
                    Point::new(x * grid_spacing as i32, y * grid_spacing as i32),
                    Size::new(grid_spacing as u32, grid_spacing as u32),
                )
                .into_styled(
                    PrimitiveStyleBuilder::new()
                        .fill_color(grid_colors[current_color])
                        .build(),
                )
                .draw(&mut display)
                .unwrap();
            } else {
                // Draw the touch point using the current color
                Rectangle::new(Point::new(touch_event.x, touch_event.y), Size::new(2, 2))
                    .into_styled(
                        PrimitiveStyleBuilder::new()
                            .fill_color(grid_colors[current_color])
                            .build(),
                    )
                    .draw(&mut display)
                    .unwrap();
            }
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    let peripherals = unsafe { Peripherals::steal() };
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    // Disable the RTC and TIMG watchdog timers
    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    let mut bl = Output::new(io.pins.gpio1, Level::Low);

    let mut delay = Delay::new(&clocks);
    let mut panic_buf = frostsnap_device::panic::PanicBuffer::<512>::default();

    let _ = match info.location() {
        Some(location) => write!(
            &mut panic_buf,
            "{}:{} {}",
            location.file().split('/').last().unwrap_or(""),
            location.line(),
            info
        ),
        None => write!(&mut panic_buf, "{}", info),
    };

    let spi = Spi::new(peripherals.SPI2, 80u32.MHz(), SpiMode::Mode2, &clocks)
        .with_sck(io.pins.gpio8)
        .with_mosi(io.pins.gpio7);
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);

    let di = SPIInterface::new(spi_device, Output::new(io.pins.gpio9, Level::Low));
    let mut display = mipidsi::Builder::new(ST7789, di)
        .display_size(240, 280)
        .display_offset(0, 20) // 240*280 panel
        .invert_colors(ColorInversion::Inverted)
        .reset_pin(Output::new(io.pins.gpio6, Level::Low))
        .init(&mut delay)
        .unwrap();
    graphics::error_print(&mut display, panic_buf.as_str());
    bl.set_high();

    loop {}
}

/// Dummy CS pin for our display
struct NoCs;

impl embedded_hal::digital::OutputPin for NoCs {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl embedded_hal::digital::ErrorType for NoCs {
    type Error = core::convert::Infallible;
}
