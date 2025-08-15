extern crate alloc;
use alloc::collections::VecDeque;
use alloc::format;
use alloc::vec::Vec;
use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal_nb::serial::{Read, Write};
use esp_hal::uart::AnyUart;
use esp_hal::Blocking;
use esp_hal::{prelude::*, timer, uart, usb_serial_jtag::UsbSerialJtag};
use frostsnap_comms::Direction;
use frostsnap_comms::MagicBytes;
use frostsnap_comms::ReceiveSerial;
use frostsnap_comms::Upstream;
use frostsnap_comms::BINCODE_CONFIG;

const RING_BUFFER_SIZE: usize = 256;

pub struct SerialInterface<'a, T, D> {
    io: SerialIo<'a>,
    ring_buffer: VecDeque<u8>,
    magic_bytes_progress: usize,
    timer: &'a T,
    direction: PhantomData<D>,
}

impl<'a, T, D> SerialInterface<'a, T, D> {
    pub fn new_uart(uart: uart::Uart<'a, Blocking, AnyUart>, timer: &'a T) -> Self {
        Self {
            io: SerialIo::Uart { uart },
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
        matches!(self.io, SerialIo::Jtag(_))
    }
}

impl<'a, T> SerialInterface<'a, T, Upstream> {
    pub fn new_jtag(jtag: UsbSerialJtag<'a, Blocking>, timer: &'a T) -> Self {
        Self {
            io: SerialIo::Jtag(jtag),
            ring_buffer: VecDeque::with_capacity(RING_BUFFER_SIZE),
            magic_bytes_progress: 0,
            timer,
            direction: PhantomData,
        }
    }
}

impl<'a, T, D> SerialInterface<'a, T, D>
where
    T: timer::Timer,
    D: Direction,
{
    pub fn fill_buffer(&mut self) {
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
        found.is_some()
    }

    pub fn send(
        &mut self,
        message: <D::Opposite as Direction>::RecvType,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::Message(message),
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.io.nb_flush();
        Ok(())
    }

    pub fn receive(&mut self) -> Option<Result<ReceiveSerial<D>, bincode::error::DecodeError>>
    where
        ReceiveSerial<D>: bincode::Decode<()>,
    {
        self.fill_buffer();
        if !self.ring_buffer.is_empty() {
            Some(bincode::decode_from_reader(self, BINCODE_CONFIG))
        } else {
            None
        }
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::MagicBytes(MagicBytes::default()),
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.io.nb_flush();
        Ok(())
    }
    pub fn write_conch(&mut self) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::Conch,
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.io.nb_flush();

        Ok(())
    }

    pub fn send_reset_signal(&mut self) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::Reset,
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.flush();

        Ok(())
    }

    /// Blocking flush
    pub fn flush(&mut self) {
        self.io.flush()
    }

    pub fn inner_mut(&mut self) -> &mut SerialIo<'a> {
        &mut self.io
    }
}

impl<T, D> Reader for SerialInterface<'_, T, D>
where
    T: timer::Timer,
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

                if self
                    .timer
                    .now()
                    .checked_duration_since(start_time)
                    .unwrap()
                    .to_millis()
                    > 1_000
                {
                    return Err(DecodeError::UnexpectedEnd {
                        additional: bytes.len() - i + 1,
                    });
                }
            };
        }
        Ok(())
    }
}

impl<T, D> Writer for SerialInterface<'_, T, D> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        match self.io.write_bytes(bytes) {
            Err(e) => Err(EncodeError::OtherString(format!("{e:?}"))),
            Ok(()) => Ok(()),
        }
    }
}

pub enum SerialIo<'a> {
    Uart {
        uart: uart::Uart<'a, Blocking, AnyUart>,
    },
    Jtag(UsbSerialJtag<'a, Blocking>),
}

impl SerialIo<'_> {
    pub fn change_baud(&mut self, baudrate: u32) {
        self.flush();
        match self {
            SerialIo::Uart { uart } => {
                uart.apply_config(&uart::Config {
                    baudrate,
                    ..uart::Config::default()
                })
                .unwrap();
            }
            SerialIo::Jtag(_) => { /* no baud rate for USB jtag */ }
        }
    }
    pub fn read_byte(&mut self) -> nb::Result<u8, Infallible> {
        match self {
            SerialIo::Jtag(jtag) => jtag.read_byte(),
            SerialIo::Uart { uart, .. } => uart.read().map_err(|e| match e {
                nb::Error::Other(_) => unreachable!("we have not set a timeout"),
                nb::Error::WouldBlock => nb::Error::WouldBlock,
            }),
        }
    }

    pub fn write_byte_nb(&mut self, byte: u8) -> nb::Result<(), Infallible> {
        match self {
            SerialIo::Jtag(jtag) => jtag.write_byte_nb(byte),
            SerialIo::Uart { uart, .. } => uart.write(byte).map_err(|e| match e {
                nb::Error::Other(_) => unreachable!("we have not set a timeout"),
                nb::Error::WouldBlock => nb::Error::WouldBlock,
            }),
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerialInterfaceError> {
        for byte in bytes {
            while self.write_byte_nb(*byte).is_err() {}
        }
        Ok(())
    }

    pub fn nb_flush(&mut self) {
        match self {
            SerialIo::Uart { .. } => {
                // there is no reason to call this on uart. It will just block until data is
                // actually written.
            }
            SerialIo::Jtag(jtag) => {
                // JTAG actually does need to get flushed sometimes. We don't need to block on it
                // though so ignore return value.
                let _ = jtag.flush_tx_nb();
            }
        }
    }

    // Blocking flush. The only time to use this is to make sure everything has been written before
    // moving onto something else. Usually you don't want this but it's necessary to do if you write
    // something before resetting.
    pub fn flush(&mut self) {
        match self {
            SerialIo::Uart { uart, .. } => {
                // just waits until evertything has been written
                while let Err(nb::Error::WouldBlock) = uart.flush() {
                    // wait
                }
            }
            SerialIo::Jtag(jtag) => {
                // flushes and waits until everything has been written
                let _ = jtag.flush_tx();
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
