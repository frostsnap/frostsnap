// use log::*;
use anyhow::Result;
use esp_idf_hal::units::Hertz;

use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::{
    gpio::{self, *},
    i2c, uart,
    units::*,
};

use device::LOG_LEVEL;

use esp_idf_svc::log::EspLogger;
use log::info;
static LOGGER: EspLogger = EspLogger;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
    #[default("")]
    frost_server: &'static str,
    #[default("2")]
    threshold: &'static str,
    #[default("2")]
    n_parties: &'static str,
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    log::set_logger(&LOGGER).map(|()| LOGGER.initialize())?;
    LOGGER.set_target_level("*", LOG_LEVEL);
    info!("Log level set to {}", LOGGER.get_max_level());

    let peripherals = Peripherals::take().unwrap();
    let mut button = PinDriver::input(peripherals.pins.gpio9)?;

    button.set_pull(Pull::Down)?;
    // Onboard RGB LED pin
    // ESP32-C3-DevKitC-02 gpio8, esp-rs gpio2
    let led = peripherals.pins.gpio2;
    let channel = peripherals.rmt.channel0;

    // If you see all zeros then the baudrate is wrong
    let uart_config = uart::config::Config::default().baudrate(Hertz(9600));

    // connect tx to rx on UART device
    let uart: uart::UartDriver = uart::UartDriver::new(
        peripherals.uart1,
        peripherals.pins.gpio7,
        peripherals.pins.gpio8,
        Option::<gpio::Gpio0>::None,
        Option::<gpio::Gpio1>::None,
        &uart_config,
    )
    .unwrap();
    let uarts = &mut [uart];

    // let uart2: uart::UartDriver = uart::UartDriver::new(
    //     peripherals.uart0,
    //     peripherals.pins.gpio0,
    //     peripherals.pins.gpio1,
    //     Option::<gpio::Gpio0>::None,
    //     Option::<gpio::Gpio1>::None,
    //     &uart_config,
    // )
    // .unwrap();
    // let uarts = &mut [uart, uart2];

    let i2c = peripherals.i2c0;
    let sda = peripherals.pins.gpio0;
    let scl = peripherals.pins.gpio1;
    // === MASTER ===
    let config = i2c::I2cConfig::new().baudrate(400.kHz().into());
    let i2c = i2c::I2cDriver::new(i2c, sda, scl, &config)?;
    // // i2c proxy for every slave participant
    // let bus = shared_bus::BusManagerSimple::new(i2c);
    // let i2c_1 = bus.acquire_i2c();
    // let i2c_2 = bus.acquire_i2c();

    // // === SLAVE ===
    // let config = i2c::I2cSlaveConfig::new()
    //     .rx_buffer_length(1024)
    //     .tx_buffer_length(1024);
    // let i2c = i2c::I2cSlaveDriver::new(i2c, sda, scl, 0x21, &config)?;

    let i2cs = &mut [i2c];
    // frost_core::process_keygen(uarts);

    Ok(())
}
