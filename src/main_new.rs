use esp_idf_hal::gpio;
use esp_idf_hal::prelude::*;
use esp_idf_hal::uart;
use frostdevice::frost_core;

fn main() {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    // If you see all zeros then the baudrate is wrong
    let config = uart::config::Config::default().baudrate(Hertz(9600));

    // let mut button = PinDriver::input(peripherals.pins.gpio9).unwrap();
    // button.set_pull(esp_idf_hal::gpio::Pull::Down).unwrap();

    // connect tx to rx on UART device
    let uart: uart::UartDriver = uart::UartDriver::new(
        peripherals.uart1,
        peripherals.pins.gpio7,
        peripherals.pins.gpio8,
        Option::<gpio::Gpio0>::None,
        Option::<gpio::Gpio1>::None,
        &config,
    )
    .unwrap();
    // let uarts = &mut [uart];

    let uart2: uart::UartDriver = uart::UartDriver::new(
        peripherals.uart0,
        peripherals.pins.gpio3,
        peripherals.pins.gpio4,
        Option::<gpio::Gpio0>::None,
        Option::<gpio::Gpio1>::None,
        &config,
    )
    .unwrap();
    let uarts = &mut [uart, uart2];
    frost_core::process_keygen(uarts);
}
