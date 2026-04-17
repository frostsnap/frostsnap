//! ESP32-S3 widget bring-up peripheral initialization and management
//!
//! Pinout summary (ESP32-S3 widget bring-up):
//! - Display (ST7789 over SPI2):
//!   - `GPIO8`  = SCK
//!   - `GPIO7`  = MOSI
//!   - `GPIO9`  = DC
//!   - `GPIO6`  = RST
//!   - `GPIO1`  = Backlight (LEDC)
//! - Touchscreen (CST816S over I2C0):
//!   - `GPIO4`  = SDA
//!   - `GPIO5`  = SCL
//!   - `GPIO2`  = INT
//!   - `GPIO3`  = RST
//! - Detect pins:
//!   - `GPIO0`  = Upstream detect (pull-up)
//!   - `GPIO10` = Downstream detect (pull-up)

use alloc::boxed::Box;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Io, Output, Pull},
    i2c::master::{Config as I2cConfig, I2c},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    spi::master::Spi,
    time::Rate,
    Blocking,
};
use frostsnap_cst816s::CST816S;
use mipidsi::{interface::SpiInterface, models::ST7789};

#[macro_export]
macro_rules! init_display {
    (peripherals: $peripherals:expr, delay: $delay:expr) => {{
        use alloc::boxed::Box;
        use esp_hal::{
            gpio::{Level, Output, OutputConfig},
            spi::{
                master::{Config as SpiConfig, Spi},
                Mode,
            },
            time::Rate,
        };
        use mipidsi::{interface::SpiInterface, models::ST7789, options::ColorInversion};

        let spi = Spi::new(
            $peripherals.SPI2,
            SpiConfig::default()
                .with_frequency(Rate::from_mhz(80))
                .with_mode(Mode::_2),
        )
        .unwrap()
        .with_sck($peripherals.GPIO8)
        .with_mosi($peripherals.GPIO7);

        let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs).unwrap();
        let buffer: &'static mut [u8] = Box::leak(Box::new([0u8; 512]));
        let di = SpiInterface::new(
            spi_device,
            Output::new($peripherals.GPIO9, Level::Low, OutputConfig::default()),
            buffer,
        );

        mipidsi::Builder::new(ST7789, di)
            .display_size(240, 280)
            .display_offset(0, 20)
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(Output::new(
                $peripherals.GPIO6,
                Level::Low,
                OutputConfig::default(),
            ))
            .init($delay)
            .unwrap()
    }};
}

/// Dummy CS pin for our display
pub struct NoCs;

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

/// Type alias for the display to reduce complexity
type Display<'a> = mipidsi::Display<
    SpiInterface<
        'a,
        embedded_hal_bus::spi::ExclusiveDevice<
            Spi<'a, Blocking>,
            NoCs,
            embedded_hal_bus::spi::NoDelay,
        >,
        Output<'a>,
    >,
    ST7789,
    Output<'a>,
>;

/// All device peripherals initialized and ready to use (widget bring-up subset)
pub struct DevicePeripherals<'a> {
    /// Display
    pub display: Display<'a>,

    /// Touch receiver for interrupt-based touch handling
    pub touch_receiver: frostsnap_cst816s::interrupt::TouchReceiver,

    /// Display backlight
    pub backlight: channel::Channel<'a, LowSpeed>,

    /// Pin to detect upstream device connection
    pub upstream_detect: Input<'a>,

    /// Pin to detect downstream device connection
    pub downstream_detect: Input<'a>,
}

impl<'a> DevicePeripherals<'a> {
    /// Widget bring-up mode never requires provisioning.
    pub fn needs_factory_provisioning(&self) -> bool {
        false
    }

    /// Initialize all widget-dev peripherals.
    pub fn init(mut peripherals: esp_hal::peripherals::Peripherals) -> Box<Self> {
        let mut io = Io::new(peripherals.IO_MUX.reborrow());
        let mut delay = Delay::new();

        let upstream_detect = Input::new(
            peripherals.GPIO0,
            InputConfig::default().with_pull(Pull::Up),
        );
        let downstream_detect = Input::new(
            peripherals.GPIO10,
            InputConfig::default().with_pull(Pull::Up),
        );

        let mut ledc = Ledc::new(peripherals.LEDC);
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
        let mut lstimer0 = ledc.timer::<LowSpeed>(timerledc::Number::Timer0);
        lstimer0
            .configure(timerledc::config::Config {
                duty: timerledc::config::Duty::Duty10Bit,
                clock_source: LSClockSource::APBClk,
                frequency: Rate::from_khz(24),
            })
            .unwrap();
        let lstimer0 = Box::leak(Box::new(lstimer0));
        let mut backlight = ledc.channel(channel::Number::Channel0, peripherals.GPIO1);
        backlight
            .configure(channel::config::Config {
                timer: lstimer0,
                duty_pct: 0,
                drive_mode: esp_hal::gpio::DriveMode::PushPull,
            })
            .unwrap();

        let mut display = init_display!(peripherals: peripherals, delay: &mut delay);

        let i2c = I2c::new(
            peripherals.I2C0,
            I2cConfig::default().with_frequency(Rate::from_khz(400)),
        )
        .unwrap()
        .with_sda(peripherals.GPIO4)
        .with_scl(peripherals.GPIO5);

        let mut capsense = CST816S::new_esp32(i2c, peripherals.GPIO2, peripherals.GPIO3);
        capsense.setup(&mut delay).unwrap();

        let touch_receiver = frostsnap_cst816s::interrupt::register(capsense, &mut io);

        let _ = display.clear(Rgb565::BLACK);
        backlight.start_duty_fade(0, 100, 500).unwrap();

        Box::new(Self {
            display,
            touch_receiver,
            backlight,
            upstream_detect,
            downstream_detect,
        })
    }
}
