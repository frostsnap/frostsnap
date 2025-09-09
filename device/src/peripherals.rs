//! Device peripheral initialization and management

use alloc::{boxed::Box, rc::Rc};
use core::cell::RefCell;
use display_interface_spi::SPIInterface;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use esp_hal::{
    delay::Delay,
    gpio::{AnyPin, Input, Io, Output, Pull},
    hmac::Hmac,
    i2c::master::{Config as I2cConfig, I2c},
    ledc::{
        channel::{self, ChannelIFace},
        timer::{self as timerledc, LSClockSource, TimerIFace},
        LSGlobalClkSource, Ledc, LowSpeed,
    },
    peripherals::{Peripherals, DS, RSA, TIMG0, TIMG1},
    prelude::*,
    spi::master::Spi,
    timer::timg::{Timer, Timer0, TimerGroup},
    uart::Uart,
    usb_serial_jtag::UsbSerialJtag,
    Blocking,
};
use frostsnap_cst816s::CST816S;
use mipidsi::models::ST7789;
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use crate::efuse::EfuseController;

#[macro_export]
macro_rules! init_display {
    (peripherals: $peripherals:expr, delay: $delay:expr) => {{
        use display_interface_spi::SPIInterface;
        use esp_hal::{
            gpio::{Level, Output},
            prelude::*,
            spi::{
                master::{Config as SpiConfig, Spi},
                SpiMode,
            },
        };
        use mipidsi::{models::ST7789, options::ColorInversion};

        let spi = Spi::new_with_config(
            &mut $peripherals.SPI2,
            SpiConfig {
                frequency: 80.MHz(),
                mode: SpiMode::Mode2,
                ..SpiConfig::default()
            },
        )
        .with_sck(&mut $peripherals.GPIO8)
        .with_mosi(&mut $peripherals.GPIO7);

        let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new_no_delay(spi, NoCs);
        let di = SPIInterface::new(spi_device, Output::new(&mut $peripherals.GPIO9, Level::Low));

        let display = mipidsi::Builder::new(ST7789, di)
            .display_size(240, 280)
            .display_offset(0, 20) // 240*280 panel
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(Output::new(&mut $peripherals.GPIO6, Level::Low))
            .init($delay)
            .unwrap();

        display
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
    /// Shared timer for timing operations (leaked to 'static for SerialInterface)
    pub timer: &'static Timer<Timer0<TIMG0>, Blocking>,

    /// UI timer for display and touch operations
    pub ui_timer: Timer<Timer0<TIMG1>, Blocking>,

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
    pub upstream_detect: Input<'a, AnyPin>,

    /// Pin to detect downstream device connection
    pub downstream_detect: Input<'a, AnyPin>,

    /// SHA256 hardware accelerator
    pub sha256: esp_hal::sha::Sha<'a>,

    /// HMAC hardware module (Rc for shared ownership)
    pub hmac: Rc<RefCell<Hmac<'a>>>,

    /// Digital Signature peripheral
    pub ds: &'a mut DS,

    /// RSA hardware accelerator
    pub rsa: &'a mut RSA,

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

// Static storage for peripherals to enable 'static references
// This is safe because peripherals are initialized once at startup and never dropped
static mut PERIPHERALS_SINGLETON: Option<Peripherals> = None;

impl<'a> DevicePeripherals<'a> {
    /// Check if the device needs factory provisioning
    pub fn needs_factory_provisioning(&self) -> bool {
        !self.efuse.has_hmac_keys_initialized()
    }

    /// Initialize all device peripherals including initial RNG
    pub fn init(peripherals: Peripherals) -> Box<Self> {
        // SAFETY: We can store peripherals in static storage and get a 'static reference
        // since we're never passing it on to anyone else.
        let peripherals = unsafe {
            PERIPHERALS_SINGLETON = Some(peripherals);
            // Use a raw pointer to avoid the mutable static warning
            let ptr = &raw mut PERIPHERALS_SINGLETON;
            (*ptr).as_mut().unwrap()
        };

        // Initialize JTAG
        let jtag = UsbSerialJtag::new(&mut peripherals.USB_DEVICE);

        let upstream_detect = Input::new(&mut peripherals.GPIO0, Pull::Up);
        // Initialize upstream UART only if upstream device is detected
        let uart_upstream = if upstream_detect.is_low() {
            Some(
                Uart::new(
                    &mut peripherals.UART1,
                    &mut peripherals.GPIO18,
                    &mut peripherals.GPIO19,
                )
                .unwrap(),
            )
        } else {
            None
        };

        // Always initialize downstream UART
        let uart_downstream = Uart::new(
            &mut peripherals.UART0,
            &mut peripherals.GPIO21,
            &mut peripherals.GPIO20,
        )
        .unwrap();

        // Initialize Io for interrupt handling.
        // SAFETY: We bypass the check that esp-hal is trying to get us to do here since this function has the
        // only copy of Peripherals. Hopefully this doesn't need to happen in esp-hal v1.0+.
        let mut io = Io::new(unsafe { core::mem::zeroed() });

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

        // Extract timer0 from TIMG0 and leak it to get 'static reference for SerialInterface
        // This is safe because the timer lives for the entire program lifetime
        let timer = Box::leak(Box::new(timg0.timer0));
        let ui_timer = timg1.timer0;

        // Detection pins (using AnyPin to avoid generics)
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

        let mut display = init_display!(peripherals: peripherals, delay: &mut delay);

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

        let mut capsense = CST816S::new_esp32(i2c, &mut peripherals.GPIO2, &mut peripherals.GPIO3);
        capsense.setup(&mut delay).unwrap();

        // Register the capsense instance with the interrupt handler
        let touch_receiver = frostsnap_cst816s::interrupt::register(capsense, &mut io);

        // Clear display and turn on backlight
        let _ = display.clear(Rgb565::BLACK);
        backlight.start_duty_fade(0, 30, 500).unwrap();

        // Initialize other crypto peripherals
        let efuse = EfuseController::new(&mut peripherals.EFUSE);
        let hmac = Rc::new(RefCell::new(Hmac::new(&mut peripherals.HMAC)));

        Box::new(Self {
            timer,
            ui_timer,
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
            ds: &mut peripherals.DS,
            efuse,
            initial_rng,
            rsa: &mut peripherals.RSA,
        })
    }
}
