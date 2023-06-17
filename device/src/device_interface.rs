use crate::alloc::string::ToString;
use crate::{io, oled, state, storage};

use esp32c3_hal::peripherals::TIMG0;
use esp32c3_hal::peripherals::TIMG1;
use esp32c3_hal::peripherals::UART0;
use esp32c3_hal::peripherals::UART1;
use esp32c3_hal::Rng;
use esp32c3_hal::Timer;
use io::SerialInterface;
use io::UpstreamDetector;

use esp32c3_hal::peripherals::I2C0;
use esp32c3_hal::timer::Timer0;
use esp_hal_smartled::{smartLedAdapter, SmartLedsAdapter};
use esp_storage::FlashStorage;

use alloc::{collections::VecDeque, vec::Vec};
use esp32c3_hal::{
    clock::ClockControl,
    peripherals::Peripherals,
    prelude::*,
    pulse_control::ClockSource,
    timer::TimerGroup,
    uart::{config, TxRxPins},
    Delay, PulseControl, Rtc, Uart, UsbSerialJtag, IO,
};
use smart_leds::{brightness, colors, SmartLedsWrite, RGB};

use frostsnap_comms::{DeviceReceiveMessage, DeviceSendSerial, Downstream, Upstream};
use frostsnap_core::schnorr_fun::fun::marker::Public;

pub trait Device {
    type DeviceError;
    type UpstreamSerialInterface;
    type DownstreamSerialInterface;

    fn print(&self, str: &str) -> Result<(), Self::DeviceError>;
    fn print_header(&self, str: &str) -> Result<(), Self::DeviceError>;
    fn read_rng(&self, buff: &mut [u8; 32]) -> Result<(), Self::DeviceError>;
    fn delay_ms(&self, delay: u32);
    fn flash_load(&self) -> Result<(), Self::DeviceError>;
    fn flash_save(&self) -> Result<(), Self::DeviceError>;
    fn led_write(&self, color: RGB<u8>) -> Result<(), Self::DeviceError>;

    fn poll_read_downstream(&self) -> bool;
    fn receive_from_downstream(
        &self,
    ) -> Result<DeviceSendSerial<Downstream>, bincode::error::DecodeError>;
    fn upstream_serial_interface(&self) -> Option<&mut Self::UpstreamSerialInterface>;
    fn upstream_serial_detector_status(&self) -> bool;
    fn downstream_serial_interface(&self) -> Self::DownstreamSerialInterface;
    fn now(&self) -> u64;
}

pub struct PurpleDevice<'a, 'b, 'c, 'd, 'e, T, U> {
    pub upstream_detector: UpstreamDetector<'c, T, U>,
    pub downstream_serial: SerialInterface<'d, Timer0<TIMG1>, UART0, Downstream>,
    rng: Rng<'e>,
    display: oled::SSD1306<'a, I2C0>,
    led: SmartLedsAdapter<PulseControl<'b>, 25>,
    flash: storage::DeviceStorage,
    delay: Delay,
    timer0: Timer<Timer0<TIMG0>>,
}

#[derive(Debug)]
struct PurpleDeviceError {}

impl<
        'a,
        'b,
        'c,
        'd,
        'e,
        T: esp32c3_hal::prelude::_esp_hal_timer_Instance,
        U: esp32c3_hal::prelude::_esp_hal_uart_Instance,
    > Device for PurpleDevice<'a, 'b, 'c, 'd, 'e, T, U>
{
    type DeviceError = PurpleDeviceError;
    type UpstreamSerialInterface = Option<&'f mut SerialInterface<T, U, Upstream>>;
    type DownstreamSerialInterface = SerialInterface<'d, Timer0<TIMG1>, UART0, Downstream>;

    fn print(&self, str: &str) -> Result<(), PurpleDeviceError> {
        self.display.print(str).unwrap();
        Ok(())
    }

    fn print_header(&self, str: &str) -> Result<(), PurpleDeviceError> {
        self.display.print_header(str).unwrap();
        Ok(())
    }

    fn read_rng(&self, buff: &mut [u8; 32]) -> Result<(), PurpleDeviceError> {
        self.rng.read(buff).unwrap();
        Ok(())
    }

    fn delay_ms(&self, delay: u32) {
        self.delay.delay_ms(delay);
    }

    fn flash_load(&self) -> Result<(), PurpleDeviceError> {
        todo!()
    }

    fn flash_save(&self) -> Result<(), PurpleDeviceError> {
        todo!()
    }

    fn led_write(&self, color: RGB<u8>) -> Result<(), PurpleDeviceError> {
        // self.led
        //     .write(brightness([color].iter().cloned(), 10))
        //     .unwrap();
        todo!();
        Ok(())
    }

    fn poll_read_downstream(&self) -> bool {
        self.downstream_serial.poll_read()
    }

    fn receive_from_downstream(
        &self,
    ) -> Result<DeviceSendSerial<Downstream>, bincode::error::DecodeError> {
        self.downstream_serial.receive_from_downstream()
    }

    fn upstream_serial_interface(&self) -> Self::UpstreamSerialInterface {
        self.upstream_detector.serial_interface()
    }

    fn upstream_serial_detector_status(&self) -> bool {
        self.upstream_detector.switched
    }

    fn downstream_serial_interface(&self) -> Self::DownstreamSerialInterface {
        self.downstream_serial
    }

    fn now(&self) -> u64 {
        self.timer0.now()
    }
}

impl<'a, 'b, 'c, 'd, 'e, T, U> PurpleDevice<'a, 'b, 'c, 'd, 'e, T, U> {
    pub fn new() -> Self {
        let peripherals = Peripherals::take();
        let mut system = peripherals.SYSTEM.split();
        let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

        // Disable the RTC and TIMG watchdog timers
        let mut rtc = Rtc::new(peripherals.RTC_CNTL);
        let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
        let mut wdt0 = timer_group0.wdt;
        let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks);
        let mut wdt1 = timer_group1.wdt;
        let mut timer0 = timer_group0.timer0;
        timer0.start(1u64.secs());
        let mut timer1 = timer_group1.timer0;
        timer1.start(1u64.secs());

        rtc.swd.disable();
        rtc.rwdt.disable();
        wdt0.disable();
        wdt1.disable();

        let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

        let mut delay = Delay::new(&clocks);

        // let button = io.pins.gpio9.into_pull_up_input();
        // let wait_button = || {
        //     // Ensure button is not pressed
        //     while button.is_high().unwrap() {}
        //     // Wait for press
        //     while button.is_low().unwrap() {}
        // };

        let mut display = oled::SSD1306::new(
            peripherals.I2C0,
            io.pins.gpio5,
            io.pins.gpio6,
            400u32.kHz(),
            &mut system.peripheral_clock_control,
            &clocks,
        )
        .unwrap();

        // RGB LED
        // White: found coordinator
        // Blue: found another device upstream
        let pulse = PulseControl::new(
            peripherals.RMT,
            &mut system.peripheral_clock_control,
            ClockSource::APB,
            0,
            0,
            0,
        )
        .unwrap();
        let mut led = <smartLedAdapter!(1)>::new(pulse.channel0, io.pins.gpio2);

        let flash = FlashStorage::new();
        let mut flash = storage::DeviceStorage::new(flash, storage::NVS_PARTITION_START);

        // Simulate factory reset
        // For now we are going to factory reset the storage on boot for easier testing and debugging.
        // Comment out if you want the frost key to persist across reboots
        // flash.erase().unwrap();
        // delay.delay_ms(2000u32);

        delay.delay_ms(1_000u32);

        let mut downstream_serial = {
            let serial_conf = config::Config {
                baudrate: frostsnap_comms::BAUDRATE,
                ..Default::default()
            };
            let txrx0 = TxRxPins::new_tx_rx(
                io.pins.gpio21.into_push_pull_output(),
                io.pins.gpio20.into_floating_input(),
            );
            let uart0 =
                Uart::new_with_config(peripherals.UART0, Some(serial_conf), Some(txrx0), &clocks);
            io::SerialInterface::<_, _, Downstream>::new_uart(uart0, &timer1)
        };

        let upstream_uart = {
            let serial_conf = config::Config {
                baudrate: frostsnap_comms::BAUDRATE,
                ..Default::default()
            };
            let txrx1 = TxRxPins::new_tx_rx(
                io.pins.gpio18.into_push_pull_output(),
                io.pins.gpio19.into_floating_input(),
            );
            Uart::new_with_config(peripherals.UART1, Some(serial_conf), Some(txrx1), &clocks)
        };
        let upstream_jtag = UsbSerialJtag::new(peripherals.USB_DEVICE);

        let mut upstream_detector = UpstreamDetector::new(upstream_uart, upstream_jtag, &timer0);

        let mut rng = esp32c3_hal::Rng::new(peripherals.RNG);

        Self {
            upstream_detector,
            downstream_serial,
            rng,
            display,
            led,
            flash,
            delay,
            timer0,
        }
    }
}
