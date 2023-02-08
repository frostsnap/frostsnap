// use log::*;
use anyhow::Result;
use esp_idf_hal::units::Hertz;
use frostdevice::frost_core;

use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::{
    gpio::{self, *},
    uart,
};

pub mod http;
pub mod wifi;
pub mod ws2812;

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
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let mut button = PinDriver::input(peripherals.pins.gpio9)?;

    button.set_pull(Pull::Down)?;
    // Onboard RGB LED pin
    // ESP32-C3-DevKitC-02 gpio8, esp-rs gpio2
    let led = peripherals.pins.gpio2;
    let channel = peripherals.rmt.channel0;
    let mut neopixel = ws2812::NeoPixel::new(channel, led)?;
    neopixel.clear()?;

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
    //     peripherals.pins.gpio3,
    //     peripherals.pins.gpio4,
    //     Option::<gpio::Gpio0>::None,
    //     Option::<gpio::Gpio1>::None,
    //     &uart_config,
    // )
    // .unwrap();
    // let uarts = &mut [uart, uart2];

    frost_core::process_keygen(uarts);
    // neopixel.rainbow(0, 10, 10)?;

    Ok(())
}
