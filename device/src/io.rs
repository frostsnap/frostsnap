extern crate alloc;
use core::marker::PhantomData;

use alloc::collections::VecDeque;
use alloc::format;
use alloc::vec::Vec;

use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use esp_hal::{
    peripherals::USB_DEVICE,
    prelude::*,
    timer::{self, Timer},
    uart, UsbSerialJtag,
};
use frostsnap_comms::Direction;
use frostsnap_comms::MagicBytes;
use frostsnap_comms::ReceiveSerial;
use frostsnap_comms::Upstream;

const RING_BUFFER_SIZE: usize = 2_usize.pow(8); // i.e. 256 bytes

pub struct SerialInterface<'a, T, U, D> {
    io: SerialIo<'a, U>,
    ring_buffer: VecDeque<u8>,
    magic_bytes_progress: usize,
    timer: &'a Timer<T>,
    direction: PhantomData<D>,
}

impl<'a, T, U, D> SerialInterface<'a, T, U, D> {
    pub fn new_uart(uart: uart::Uart<'a, U>, timer: &'a Timer<T>) -> Self {
        Self {
            io: SerialIo::Uart(uart),
            ring_buffer: VecDeque::with_capacity(RING_BUFFER_SIZE),
            magic_bytes_progress: 0,
            timer,
            direction: PhantomData,
        }
    }

    pub fn clone_buffer_to_vec(&self) -> Vec<u8> {
        self.ring_buffer.clone().into()
    }

    pub fn is_jtag(&self) -> bool {
        match self.io {
            SerialIo::Uart(_) => false,
            SerialIo::Jtag(_) => true,
        }
    }
}

impl<'a, T, U> SerialInterface<'a, T, U, Upstream> {
    pub fn new_jtag(jtag: UsbSerialJtag<'a>, timer: &'a Timer<T>) -> Self {
        Self {
            io: SerialIo::Jtag(jtag),
            ring_buffer: VecDeque::with_capacity(RING_BUFFER_SIZE),
            magic_bytes_progress: 0,
            timer,
            direction: PhantomData,
        }
    }
}

impl<'a, T, U, D> SerialInterface<'a, T, U, D>
where
    U: uart::Instance,
    T: timer::Instance,
    D: Direction,
{
    fn fill_buffer(&mut self) {
        while let Ok(c) = self.io.read_byte() {
            self.ring_buffer.push_back(c);
            if self.ring_buffer.len() == RING_BUFFER_SIZE {
                break;
            }
        }
    }

    pub fn find_and_remove_magic_bytes(&mut self) -> bool {
        self.fill_buffer();
        if self.ring_buffer.is_empty() {
            return false;
        }
        let (progress, found) = frostsnap_comms::make_progress_on_magic_bytes::<D>(
            core::iter::from_fn(|| self.ring_buffer.pop_front()),
            self.magic_bytes_progress,
        );
        self.magic_bytes_progress = progress;
        found
    }

    pub fn send(
        &mut self,
        message: <D::Opposite as Direction>::RecvType,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            &ReceiveSerial::<D::Opposite>::Message(message),
            &mut *self,
            bincode::config::standard(),
        )?;
        self.io.flush();
        Ok(())
    }

    pub fn receive(&mut self) -> Option<Result<ReceiveSerial<D>, bincode::error::DecodeError>> {
        self.fill_buffer();
        if !self.ring_buffer.is_empty() {
            Some(bincode::decode_from_reader(
                self,
                bincode::config::standard(),
            ))
        } else {
            None
        }
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            &ReceiveSerial::<D::Opposite>::MagicBytes(MagicBytes::default()),
            &mut *self,
            bincode::config::standard(),
        )?;
        self.io.flush();
        Ok(())
    }
}

impl<'a, T, U, D> Reader for SerialInterface<'a, T, U, D>
where
    U: uart::Instance,
    T: timer::Instance,
    D: Direction,
{
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        for (i, target_byte) in bytes.iter_mut().enumerate() {
            let start_time = self.timer.now();

            *target_byte = loop {
                // eagerly fill the buffer so we pull bytes from the hardware serial buffer as fast
                // as possible.
                self.fill_buffer();

                if let Some(next_byte) = self.ring_buffer.pop_front() {
                    break next_byte;
                }

                if (self.timer.now() - start_time) / 80_000 > 1_000 {
                    return Err(DecodeError::UnexpectedEnd {
                        additional: bytes.len() - i + 1,
                    });
                }
            };
        }
        Ok(())
    }
}

impl<'a, T, U, D> Writer for SerialInterface<'a, T, U, D>
where
    U: uart::Instance,
{
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        match self.io.write_bytes(bytes) {
            Err(e) => Err(EncodeError::OtherString(format!("{:?}", e))),
            Ok(()) => Ok(()),
        }
    }
}

pub enum SerialIo<'a, U> {
    Uart(uart::Uart<'a, U>),
    Jtag(UsbSerialJtag<'a>),
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

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerialInterfaceError>
    where
        U: uart::Instance,
    {
        match self {
            SerialIo::Jtag(jtag) => {
                for byte in bytes {
                    while jtag.write_byte_nb(*byte).is_err() {}
                }
            }
            SerialIo::Uart(uart) => {
                for byte in bytes {
                    while let Err(e) = uart.write(*byte) {
                        match e {
                            nb::Error::Other(e) => {
                                return Err(SerialInterfaceError::UartWriteError(e))
                            }
                            nb::Error::WouldBlock => { /* keep going! */ }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn flush(&mut self)
    where
        U: uart::Instance,
    {
        match self {
            SerialIo::Uart(_) => {
                // there is no reason to call this on uart. The hardware doesn't have a "flush"
                // operation. Things that are in the write buffer will get written
                // uart.flush();
            }
            SerialIo::Jtag(jtag) => {
                // JTAG actually does need to get flushed sometimes. We don't need to block on it
                // though so ignore return value.
                let _ = jtag.flush();
            }
        }
    }
}

#[derive(Debug)]
pub enum SerialInterfaceError {
    UartReadError,
    UartWriteError(uart::Error),
    JtagError,
}

pub fn set_upstream_port_mode_jtag() {
    let usb_device = unsafe { &*USB_DEVICE::PTR };
    usb_device
        .conf0()
        .modify(|_, w| w.usb_pad_enable().set_bit());
}

pub fn set_upstream_port_mode_uart() {
    let usb_device = unsafe { &*USB_DEVICE::PTR };
    usb_device
        .conf0()
        .modify(|_, w| w.usb_pad_enable().clear_bit());
}
