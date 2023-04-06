extern crate alloc;
use alloc::format;
use alloc::vec::Vec;

use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use esp32c3_hal::peripherals::USB_DEVICE;
use esp32c3_hal::prelude::*;
use esp32c3_hal::timer::Timer;
use esp32c3_hal::uart;
use esp32c3_hal::UsbSerialJtag;

use frostsnap_comms::MAGICBYTES_JTAG;
use frostsnap_comms::MAGICBYTES_UART;

pub struct BufferedSerialInterface<'a, T, U> {
    pub interface: SerialInterface<'a, U>,
    pub read_buffer: Vec<u8>,
    timer: Timer<T>,
}

impl<'a, T, U> BufferedSerialInterface<'a, T, U> {
    pub fn find_active(
        uart: uart::Uart<'a, U>,
        jtag: UsbSerialJtag<'a, USB_DEVICE>,
        timer: Timer<T>,
    ) -> Self
    where
        T: esp32c3_hal::timer::Instance,
        U: uart::Instance,
    {
        let (interface, read_buffer) = SerialInterface::find_active(uart, jtag, &timer);
        Self {
            interface,
            read_buffer,
            timer,
        }
    }
    pub fn new_uart(uart: uart::Uart<'a, U>, timer: Timer<T>) -> Self {
        Self {
            interface: SerialInterface::Uart(uart),
            read_buffer: vec![],
            timer,
        }
    }

    pub fn new_jtag(jtag: UsbSerialJtag<'a, USB_DEVICE>, timer: Timer<T>) -> Self {
        Self {
            interface: SerialInterface::Jtag(jtag),
            read_buffer: vec![],
            timer,
        }
    }

    pub fn is_jtag(&self) -> bool {
        match self.interface {
            SerialInterface::Uart(_) => false,
            SerialInterface::Jtag(_) => true,
        }
    }

    pub fn flush(&mut self) -> Result<(), SerialInterfaceError>
    where
        U: uart::Instance,
    {
        self.interface.flush()
    }
}

pub enum SerialInterface<'a, U> {
    Uart(uart::Uart<'a, U>),
    Jtag(UsbSerialJtag<'a, USB_DEVICE>),
}

#[derive(Debug)]
pub enum SerialInterfaceError {
    UartReadError,
    UartWriteError(uart::Error),
    JtagError,
}

impl<'a, U> SerialInterface<'a, U> {
    pub fn find_active<T>(
        mut uart0: uart::Uart<'a, U>,
        mut jtag: UsbSerialJtag<'a, USB_DEVICE>,
        timer0: &Timer<T>,
    ) -> (Self, Vec<u8>)
    where
        T: esp32c3_hal::timer::Instance,
        U: uart::Instance,
    {
        loop {
            // Clear the bit in order to use UART0
            let usb_device = unsafe { &*USB_DEVICE::PTR };
            usb_device
                .conf0
                .modify(|_, w| w.usb_pad_enable().clear_bit());

            // First, try and talk to another device upstream over UART0
            // uart0.write_bytes(&MAGICBYTES_UART);
            let mut buff = vec![];
            let start_time = timer0.now();
            loop {
                match uart0.read() {
                    Ok(c) => {
                        buff.push(c);
                        let position = buff
                            .windows(MAGICBYTES_UART.len())
                            .position(|window| window == &MAGICBYTES_UART[..]);
                        if let Some(position) = position {
                            // uart0.write_bytes(&MAGICBYTES_UART).unwrap();
                            return (
                                Self::Uart(uart0),
                                buff[(position + MAGICBYTES_UART.len())..].to_vec(),
                            );
                        }
                    }
                    Err(_) => {
                        // every two CPU ticks the timer is incrimented by 1
                        if ((timer0.now() - start_time) / 40_000) > 100 {
                            break;
                        }
                    }
                }
            }

            // If we did not read MAGICBYTES on UART0, try JTAG
            // reset the USB device bit
            let usb_device = unsafe { &*USB_DEVICE::PTR };
            usb_device.conf0.modify(|_, w| w.usb_pad_enable().set_bit());

            // jtag.write_bytes(&MAGICBYTES_JTAG);
            let mut buff = vec![];
            let start_time = timer0.now();
            loop {
                match jtag.read_byte() {
                    Ok(c) => {
                        buff.push(c);
                        let position = buff
                            .windows(MAGICBYTES_JTAG.len())
                            .position(|window| window == &MAGICBYTES_JTAG[..]);
                        if let Some(position) = position {
                            // jtag.write_bytes(&MAGICBYTES_JTAG).unwrap();
                            return (
                                Self::Jtag(jtag),
                                buff[(position + MAGICBYTES_JTAG.len())..].to_vec(),
                            );
                        }
                    }
                    Err(_) => {
                        // every two CPU ticks the timer is incrimented by 1
                        if (timer0.now() - start_time) / 40_000 > 100 {
                            break;
                        }
                    }
                }
            }
        }
    }

    fn read(&mut self) -> Result<u8, SerialInterfaceError>
    where
        U: uart::Instance,
    {
        match self {
            SerialInterface::Jtag(jtag) => jtag
                .read_byte()
                .map_err(|_| SerialInterfaceError::JtagError),
            SerialInterface::Uart(uart) => {
                uart.read().map_err(|_| SerialInterfaceError::UartReadError)
            }
        }
    }

    pub fn write_bytes(&mut self, words: &[u8]) -> Result<(), SerialInterfaceError>
    where
        U: uart::Instance,
    {
        match self {
            SerialInterface::Jtag(jtag) => jtag
                .write_bytes(words)
                .map_err(|_| SerialInterfaceError::JtagError),
            SerialInterface::Uart(uart) => uart
                .write_bytes(words)
                .map_err(|e| SerialInterfaceError::UartWriteError(e)),
        }
    }

    fn flush(&mut self) -> Result<(), SerialInterfaceError>
    where
        U: uart::Instance,
    {
        match self {
            SerialInterface::Uart(uart) => {
                uart.flush().map_err(|_| SerialInterfaceError::JtagError)
            }
            SerialInterface::Jtag(jtag) => jtag
                .flush()
                .map_err(|_| SerialInterfaceError::UartReadError),
        }
    }
}

impl<'a, T, U> BufferedSerialInterface<'a, T, U>
where
    U: uart::Instance,
    T: esp32c3_hal::timer::Instance,
{
    pub fn poll_read(&mut self) -> bool {
        while let Ok(c) = self.interface.read() {
            self.read_buffer.push(c);
        }
        !self.read_buffer.is_empty()
    }

    pub fn read_for_magic_bytes(&mut self, magic_bytes: &[u8]) -> bool {
        if !self.poll_read() {
            return false;
        };

        let position = self
            .read_buffer
            .windows(magic_bytes.len())
            .position(|window| window == magic_bytes);
        match position {
            Some(position) => {
                self.read_buffer = self.read_buffer.split_off(position + magic_bytes.len());
                return true;
            }
            None => {
                self.read_buffer = self
                    .read_buffer
                    .split_off(self.read_buffer.len().saturating_sub(magic_bytes.len() + 1));
                return false;
            }
        }
    }
}

impl<'a, T, U> Reader for BufferedSerialInterface<'a, T, U>
where
    U: uart::Instance,
    T: esp32c3_hal::timer::Instance,
{
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        let start_time = self.timer.now();

        while self.read_buffer.len() < bytes.len() {
            self.poll_read();
            if (self.timer.now() - start_time) / 40_000 > 1_000 {
                return Err(DecodeError::UnexpectedEnd {
                    additional: bytes.len() - self.read_buffer.len(),
                });
            }
        }
        let extra_bytes = self.read_buffer.split_off(bytes.len());

        bytes.copy_from_slice(&self.read_buffer);
        self.read_buffer = extra_bytes;
        Ok(())
    }
}

impl<'a, T, U> Writer for BufferedSerialInterface<'a, T, U>
where
    U: uart::Instance,
{
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        match self.interface.write_bytes(bytes) {
            Err(e) => return Err(EncodeError::OtherString(format!("{:?}", e))),
            Ok(()) => Ok(()),
        }
    }
}
