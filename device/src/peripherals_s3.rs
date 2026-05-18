//! ESP32-S3 peripheral initialization and management
//!
//! Pinout summary:
//! - Display (ST7789 over SPI2):
//!   - `GPIO5`  = SCL
//!   - `GPIO45`  = SDA (MOSI)
//!   - `GPIO4`  = DC
//!   - `GPIO6`  = RST
//!   - `GPIO1`  = Backlight (LEDC)
//! - Touchscreen (CST816S over I2C0):
//!   - `GPIO46`  = SDA
//!   - `GPIO7`  = SCL
//!   - `GPIO47`  = INT
//!   - `GPIO15`  = RST
//! - Detect pins:
//!   - `GPIO0`  = Upstream detect (pull-up)
//!   - `GPIO10` = Downstream detect (pull-up)
//! - UART links:
//!   - Upstream UART1: `GPIO18` RX, `GPIO19` TX
//!   - Downstream UART0: `GPIO21` RX, `GPIO20` TX

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use esp_hal::{
    gpio::{Input, InputConfig, Pull},
    hmac::Hmac,
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    peripherals::{DS, RSA},
    time::Rate,
    uart::Uart,
    usb_serial_jtag::UsbSerialJtag,
    Blocking,
};
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use crate::efuse::EfuseController;

#[cfg(not(feature = "qemu-display"))]
use esp_hal::{
    delay::Delay,
    gpio::Io,
    gpio::Output,
    i2c::master::{Config as I2cConfig, I2c},
    spi::master::Spi,
};
#[cfg(not(feature = "qemu-display"))]
use frostsnap_cst816s::CST816S;
#[cfg(not(feature = "qemu-display"))]
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
        .with_sck($peripherals.GPIO5)
        .with_mosi($peripherals.GPIO45);

        let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs).unwrap();
        let buffer: &'static mut [u8] = Box::leak(Box::new([0u8; 512]));
        let di = SpiInterface::new(
            spi_device,
            Output::new($peripherals.GPIO4, Level::Low, OutputConfig::default()),
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
#[cfg(not(feature = "qemu-display"))]
pub type Display<'a> = mipidsi::Display<
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

#[cfg(feature = "qemu-display")]
pub type Display<'a> = crate::qemu_display::VirtualDisplay<'a>;

#[cfg(not(feature = "qemu-display"))]
pub fn flush_display(_display: &mut Display<'_>) {}

#[cfg(feature = "qemu-display")]
pub fn flush_display(display: &mut Display<'_>) {
    display.flush_if_dirty();
}

#[cfg(not(feature = "qemu-display"))]
pub fn poll_touch_input() {}

#[cfg(feature = "qemu-display")]
pub fn poll_touch_input() {
    crate::qemu_touch::poll();
}

#[cfg(not(feature = "qemu-display"))]
pub fn adjust_touch_point(point: Point) -> Point {
    crate::touch_calibration::adjust_touch_point(point)
}

#[cfg(feature = "qemu-display")]
pub fn adjust_touch_point(point: Point) -> Point {
    point
}

/// All device peripherals initialized and ready to use
pub struct DevicePeripherals<'a> {
    /// Display
    pub display: Display<'a>,

    /// Touch receiver for interrupt-based touch handling
    pub touch_receiver: frostsnap_cst816s::interrupt::TouchReceiver,

    /// Display backlight
    pub backlight: channel::Channel<'a, LowSpeed>,

    /// UART for upstream device connection (if detected)
    pub uart_upstream: Option<Uart<'static, Blocking>>,

    /// UART for downstream device connection
    pub uart_downstream: Uart<'static, Blocking>,

    /// USB JTAG for debugging and upstream connection
    pub jtag: UsbSerialJtag<'a, Blocking>,

    /// Pin to detect upstream device connection
    pub upstream_detect: Input<'a>,

    /// Pin to detect downstream device connection
    pub downstream_detect: Input<'a>,

    /// SHA256 hardware accelerator
    pub sha256: esp_hal::sha::Sha<'a>,

    /// HMAC hardware module (Rc for shared ownership)
    pub hmac: Rc<RefCell<Hmac<'a>>>,

    /// Digital Signature peripheral
    pub ds: DS<'a>,

    /// RSA hardware accelerator
    pub rsa: RSA<'a>,

    /// eFuse controller
    pub efuse: EfuseController,

    /// Initial RNG seeded from hardware
    pub initial_rng: ChaCha20Rng,
}

/// Extract entropy from hardware RNG mixed with SHA256
fn extract_entropy(
    rng: &mut impl RngCore,
    sha256: &mut esp_hal::sha::Sha<'_>,
    bytes: usize,
) -> ChaCha20Rng {
    use frostsnap_core::sha2::digest::FixedOutput;

    let mut digest = sha256.start::<esp_hal::sha::Sha256>();
    for _ in 0..(bytes.div_ceil(64)) {
        let mut entropy = [0u8; 64];
        rng.fill_bytes(&mut entropy);
        frostsnap_core::sha2::digest::Update::update(&mut digest, entropy.as_ref());
    }

    let result = digest.finalize_fixed();
    ChaCha20Rng::from_seed(result.into())
}

impl<'a> DevicePeripherals<'a> {
    /// Check if the device needs factory provisioning
    pub fn needs_factory_provisioning(&self) -> bool {
        !self.efuse.has_hmac_keys_initialized()
    }

    /// Initialize all device peripherals including initial RNG
    pub fn init(mut peripherals: esp_hal::peripherals::Peripherals) -> Box<Self> {
        #[cfg(not(feature = "qemu-display"))]
        let mut io = Io::new(peripherals.IO_MUX.reborrow());
        #[cfg(not(feature = "qemu-display"))]
        let mut delay = Delay::new();

        let mut sha256 = esp_hal::sha::Sha::new(peripherals.SHA);

        let trng_source =
            esp_hal::rng::TrngSource::new(peripherals.RNG.reborrow(), peripherals.ADC1.reborrow());
        let mut trng = esp_hal::rng::Trng::try_new().expect("TRNG source should be enabled");
        let initial_rng = extract_entropy(&mut trng, &mut sha256, 1024);
        drop(trng);
        drop(trng_source);

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

        #[cfg(not(feature = "qemu-display"))]
        let mut display = init_display!(peripherals: peripherals, delay: &mut delay);

        #[cfg(feature = "qemu-display")]
        let mut display = crate::qemu_display::VirtualDisplay::new();

        #[cfg(not(feature = "qemu-display"))]
        let i2c = I2c::new(
            peripherals.I2C0,
            I2cConfig::default().with_frequency(Rate::from_khz(400)),
        )
        .unwrap()
        .with_sda(peripherals.GPIO16)
        .with_scl(peripherals.GPIO7);

        #[cfg(not(feature = "qemu-display"))]
        let mut capsense = CST816S::new_esp32(i2c, peripherals.GPIO17, peripherals.GPIO15);
        #[cfg(not(feature = "qemu-display"))]
        capsense.setup(&mut delay).unwrap();

        #[cfg(not(feature = "qemu-display"))]
        let touch_receiver = frostsnap_cst816s::interrupt::register(capsense, &mut io);
        #[cfg(feature = "qemu-display")]
        let touch_receiver = {
            crate::qemu_touch::init();
            frostsnap_cst816s::interrupt::virtual_receiver()
        };

        let _ = display.clear(Rgb565::BLACK);
        backlight.start_duty_fade(100, 0, 500).unwrap();

        let efuse = EfuseController::new();
        let hmac = Rc::new(RefCell::new(Hmac::new(peripherals.HMAC)));

        let jtag = UsbSerialJtag::new(peripherals.USB_DEVICE);

        #[cfg(not(feature = "qemu-display"))]
        let uart_upstream = if upstream_detect.is_low() {
            Some(
                Uart::new(peripherals.UART1, esp_hal::uart::Config::default())
                    .unwrap()
                    .with_rx(peripherals.GPIO18)
                    .with_tx(peripherals.GPIO19),
            )
        } else {
            None
        };
        #[cfg(feature = "qemu-display")]
        let uart_upstream = None;

        let uart_downstream = Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
            .unwrap()
            .with_rx(peripherals.GPIO21)
            .with_tx(peripherals.GPIO20);

        Box::new(Self {
            display,
            touch_receiver,
            backlight,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            sha256,
            hmac,
            ds: peripherals.DS,
            efuse,
            initial_rng,
            rsa: peripherals.RSA,
        })
    }
}
