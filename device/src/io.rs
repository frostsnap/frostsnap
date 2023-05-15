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

pub struct SerialInterface<'a, T, U> {
    pub io: SerialIo<'a, U>,
    pub read_buffer: Vec<u8>,
    pub is_upstream: bool,
    timer: Timer<T>,
}

impl<'a, T, U> SerialInterface<'a, T, U> {
    pub fn new_uart(uart: uart::Uart<'a, U>, timer: Timer<T>, is_upstream: bool) -> Self {
        Self {
            io: SerialIo::Uart(uart),
            is_upstream,
            read_buffer: vec![],
            timer,
        }
    }

    pub fn new_jtag(jtag: UsbSerialJtag<'a, USB_DEVICE>, timer: Timer<T>) -> Self {
        Self {
            io: SerialIo::Jtag(jtag),
            is_upstream: true,
            read_buffer: vec![],
            timer,
        }
    }

    pub fn is_jtag(&self) -> bool {
        match self.io {
            SerialIo::Uart(_) => false,
            SerialIo::Jtag(_) => true,
        }
    }

    // pub fn flush(&mut self) -> Result<(), SerialInterfaceError>
    // where
    //     U: uart::Instance,
    // {
    //     self.io.flush()
    // }

    pub fn starts_with_magic(&self) -> bool {
        let looking_for = match (&self.io, self.is_upstream) {
            (SerialIo::Uart(_), true) => MAGICBYTES_UART,
            (SerialIo::Uart(_), false) => MAGICBYTES_UART,
            (SerialIo::Jtag(_), true) => MAGICBYTES_JTAG,
            (SerialIo::Jtag(_), false) => unreachable!("JTAG is only used for upstream"),
        };
        self.read_buffer.starts_with(&looking_for)
    }
}

pub enum SerialIo<'a, U> {
    Uart(uart::Uart<'a, U>),
    Jtag(UsbSerialJtag<'a, USB_DEVICE>),
}

impl<'a, U> SerialIo<'a, U> {
    fn read_byte(&mut self) -> Result<u8, SerialInterfaceError>
    where
        U: uart::Instance,
    {
        match self {
            SerialIo::Jtag(jtag) => jtag
                .read_byte()
                .map_err(|_| SerialInterfaceError::JtagError),
            SerialIo::Uart(uart) => uart.read().map_err(|_| SerialInterfaceError::UartReadError),
        }
    }

    pub fn write_bytes(&mut self, words: &[u8]) -> Result<(), SerialInterfaceError>
    where
        U: uart::Instance,
    {
        match self {
            SerialIo::Jtag(jtag) => jtag
                .write_bytes(words)
                .map_err(|_| SerialInterfaceError::JtagError),
            SerialIo::Uart(uart) => uart
                .write_bytes(words)
                .map_err(|e| SerialInterfaceError::UartWriteError(e)),
        }
    }

    // fn flush(&mut self) -> Result<(), SerialInterfaceError>
    // where
    //     U: uart::Instance,
    // {
    //     match self {
    //         SerialIo::Uart(uart) => {
    //             uart.flush().map_err(|_| SerialInterfaceError::JtagError)
    //         }
    //         SerialIo::Jtag(jtag) => jtag
    //             .flush()
    //             .map_err(|_| SerialInterfaceError::UartReadError),
    //     }
    // }
}

#[derive(Debug)]
pub enum SerialInterfaceError {
    UartReadError,
    UartWriteError(uart::Error),
    JtagError,
}

impl<'a, T, U> SerialInterface<'a, T, U> {
    pub fn find_active(
        mut uart0: uart::Uart<'a, U>,
        mut jtag: UsbSerialJtag<'a, USB_DEVICE>,
        timer0: Timer<T>,
    ) -> Self
    where
        T: esp32c3_hal::timer::Instance,
        U: uart::Instance,
    {
        let mut buff = vec![];
        let io = 'outer: loop {
            // Clear the bit in order to use UART0
            let usb_device = unsafe { &*USB_DEVICE::PTR };
            usb_device
                .conf0
                .modify(|_, w| w.usb_pad_enable().clear_bit());

            // First, try and talk to another device upstream over UART0
            let mut buff = vec![];
            let start_time = timer0.now();
            loop {
                match uart0.read() {
                    Ok(c) => {
                        buff.push(c);
                        if frostsnap_comms::find_and_remove_magic_bytes(&mut buff, &MAGICBYTES_UART)
                        {
                            break 'outer SerialIo::Uart(uart0);
                        }
                    }
                    Err(_) => {
                        // every two CPU ticks the timer is incrimented by 1
                        if ((timer0.now() - start_time) / 40_000) > 1_000 {
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
                        if frostsnap_comms::find_and_remove_magic_bytes(&mut buff, &MAGICBYTES_JTAG)
                        {
                            break 'outer SerialIo::Jtag(jtag);
                        }
                    }
                    Err(_) => {
                        // every two CPU ticks the timer is incrimented by 1
                        if (timer0.now() - start_time) / 40_000 > 1_000 {
                            break;
                        }
                    }
                }
            }
        };

        Self {
            io,
            read_buffer: buff,
            is_upstream: true,
            timer: timer0,
        }
    }
}

impl<'a, T, U> SerialInterface<'a, T, U>
where
    U: uart::Instance,
    T: esp32c3_hal::timer::Instance,
{
    pub fn poll_read(&mut self) -> bool {
        while let Ok(c) = self.io.read_byte() {
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

impl<'a, T, U> Reader for SerialInterface<'a, T, U>
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

impl<'a, T, U> Writer for SerialInterface<'a, T, U>
where
    U: uart::Instance,
{
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        match self.io.write_bytes(bytes) {
            Err(e) => return Err(EncodeError::OtherString(format!("{:?}", e))),
            Ok(()) => Ok(()),
        }
    }
}
