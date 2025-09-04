//! Device peripheral initialization and management

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;
use cst816s::CST816S;
use display_interface_spi::SPIInterface;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use esp_hal::{
    delay::Delay,
    gpio::{AnyPin, Input, Level, Output, Pull},
    hmac::Hmac,
    i2c::master::{Config as I2cConfig, I2c},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    peripherals::{Peripherals, DS, TIMG0, TIMG1},
    prelude::*,
    spi::{
        master::{Config as SpiConfig, Spi},
        SpiMode,
    },
    timer::timg::{Timer, Timer0, TimerGroup},
    uart::{self, Uart},
    usb_serial_jtag::UsbSerialJtag,
    Blocking,
};
use mipidsi::{models::ST7789, options::ColorInversion};
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use crate::efuse::EfuseController;

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
    SPIInterface<
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

/// All device peripherals initialized and ready to use
pub struct DevicePeripherals<'a> {
    /// Shared timer for timing operations
    pub timer: Timer<Timer0<TIMG0>, Blocking>,

    /// UI timer for display and touch operations
    pub ui_timer: Timer<Timer0<TIMG1>, Blocking>,

    /// Display
    pub display: Display<'a>,

    /// Touch sensor (keeping concrete types for factory compatibility)
    pub capsense: CST816S<I2c<'a, Blocking>, Input<'a>, Output<'a>>,

    /// Display backlight
    pub backlight: channel::Channel<'a, LowSpeed>,

    /// UART for upstream device connection (if detected)
    pub uart_upstream: Option<Uart<'a, Blocking>>,

    /// UART for downstream device connection
    pub uart_downstream: Uart<'a, Blocking>,

    /// USB JTAG for debugging and upstream connection
    pub jtag: UsbSerialJtag<'a, Blocking>,

    /// Pin to detect upstream device connection
    pub upstream_detect: Input<'a, AnyPin>,

    /// Pin to detect downstream device connection
    pub downstream_detect: Input<'a, AnyPin>,

    /// SHA256 hardware accelerator
    pub sha256: esp_hal::sha::Sha<'a>,

    /// HMAC hardware module (Rc for shared ownership)
    pub hmac: Rc<RefCell<Hmac<'a>>>,

    /// Digital Signature peripheral (reference)
    pub ds: &'a mut DS,

    /// eFuse controller
    pub efuse: EfuseController<'a>,

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
    /// Initialize all device peripherals including initial RNG
    pub fn init(peripherals: &'a mut Peripherals) -> Box<Self> {
        // Enable stack guard if feature is enabled
        #[cfg(feature = "stack_guard")]
        crate::stack_guard::enable_stack_guard(&mut peripherals.ASSIST_DEBUG);

        let mut delay = Delay::new();

        // Initialize SHA256 early for entropy extraction
        let mut sha256 = esp_hal::sha::Sha::new(&mut peripherals.SHA);

        // Get initial entropy from hardware RNG mixed with SHA256
        let mut trng = esp_hal::rng::Trng::new(&mut peripherals.RNG, &mut peripherals.ADC1);
        let initial_rng = extract_entropy(&mut trng, &mut sha256, 1024);

        // Initialize timers
        let timg0 = TimerGroup::new(&mut peripherals.TIMG0);
        let timg1 = TimerGroup::new(&mut peripherals.TIMG1);
        let timer = timg0.timer0;
        let ui_timer = timg1.timer0;

        // Detection pins (using AnyPin to avoid generics)
        let upstream_detect = Input::new(&mut peripherals.GPIO0, Pull::Up);
        let downstream_detect = Input::new(&mut peripherals.GPIO10, Pull::Up);

        // Initialize backlight control
        let mut ledc = Ledc::new(&mut peripherals.LEDC);
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
        let mut lstimer0 = ledc.timer::<LowSpeed>(timerledc::Number::Timer0);
        lstimer0
            .configure(timerledc::config::Config {
                duty: timerledc::config::Duty::Duty10Bit,
                clock_source: LSClockSource::APBClk,
                frequency: 24u32.kHz(),
            })
            .unwrap();
        // Leak the timer so it lives forever (we never need to drop it)
        let lstimer0 = Box::leak(Box::new(lstimer0));
        let mut backlight = ledc.channel(channel::Number::Channel0, &mut peripherals.GPIO1);
        backlight
            .configure(channel::config::Config {
                timer: lstimer0,
                duty_pct: 0, // Start with backlight off
                pin_config: channel::config::PinConfig::PushPull,
            })
            .unwrap();

        // Initialize display SPI
        let spi = Spi::new_with_config(
            &mut peripherals.SPI2,
            SpiConfig {
                frequency: 80.MHz(),
                mode: SpiMode::Mode2,
                ..SpiConfig::default()
            },
        )
        .with_sck(&mut peripherals.GPIO8)
        .with_mosi(&mut peripherals.GPIO7);

        let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
        let di = SPIInterface::new(spi_device, Output::new(&mut peripherals.GPIO9, Level::Low));

        let mut display = mipidsi::Builder::new(ST7789, di)
            .display_size(240, 280)
            .display_offset(0, 20)
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(Output::new(&mut peripherals.GPIO6, Level::Low))
            .init(&mut delay)
            .unwrap();

        // Initialize I2C for touch sensor
        let i2c = I2c::new(
            &mut peripherals.I2C0,
            I2cConfig {
                frequency: 400u32.kHz(),
                ..I2cConfig::default()
            },
        )
        .with_sda(&mut peripherals.GPIO4)
        .with_scl(&mut peripherals.GPIO5);

        let mut capsense = CST816S::new(
            i2c,
            Input::new(&mut peripherals.GPIO2, Pull::Down),
            Output::new(&mut peripherals.GPIO3, Level::Low),
        );
        capsense.setup(&mut delay).unwrap();

        // Clear display and turn on backlight
        let _ = display.clear(Rgb565::BLACK);
        backlight.start_duty_fade(0, 30, 500).unwrap();

        // Initialize other crypto peripherals
        let efuse = EfuseController::new(&mut peripherals.EFUSE);
        let hmac = Rc::new(RefCell::new(Hmac::new(&mut peripherals.HMAC)));

        // Initialize JTAG
        let jtag = UsbSerialJtag::new(&mut peripherals.USB_DEVICE);

        // Initialize UART configuration
        let serial_conf = uart::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            ..Default::default()
        };

        // Initialize upstream UART only if upstream device is detected
        let uart_upstream = if upstream_detect.is_low() {
            Some(
                Uart::new_with_config(
                    &mut peripherals.UART1,
                    serial_conf,
                    &mut peripherals.GPIO18,
                    &mut peripherals.GPIO19,
                )
                .unwrap(),
            )
        } else {
            None
        };

        // Always initialize downstream UART
        let uart_downstream = Uart::new_with_config(
            &mut peripherals.UART0,
            serial_conf,
            &mut peripherals.GPIO21,
            &mut peripherals.GPIO20,
        )
        .unwrap();

        Box::new(Self {
            timer,
            ui_timer,
            display,
            capsense,
            backlight,
            uart_upstream,
            uart_downstream,
            jtag,
            upstream_detect,
            downstream_detect,
            sha256,
            hmac,
            ds: &mut peripherals.DS,
            efuse,
            initial_rng,
        })
    }
}
