use alloc::format;
use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::DecodeError;
use bincode::error::EncodeError;
use core::convert::Infallible;
use core::marker::PhantomData;
use esp_hal::uart::{AnyUart, Uart};
use esp_hal::Blocking;
use esp_hal::{prelude::*, timer, uart, usb_serial_jtag::UsbSerialJtag};
use frostsnap_comms::Direction;
use frostsnap_comms::MagicBytes;
use frostsnap_comms::ReceiveSerial;
use frostsnap_comms::BINCODE_CONFIG;

use crate::uart_interrupt::RX_FIFO_THRESHOLD;
use crate::uart_interrupt::{fill_buffer, UartNum, UartReceiver, UartWriter};

pub struct SerialInterface<'a, T, D> {
    io: SerialIo<'a>,
    magic_bytes_progress: usize,
    timer: &'a T,
    direction: PhantomData<D>,
}

impl<'a, T, D> SerialInterface<'a, T, D> {
    pub fn new_uart(
        mut uart: Uart<'static, Blocking, AnyUart>,
        uart_num: UartNum,
        timer: &'a T,
    ) -> Self {
        // Configure UART with standard settings
        let serial_conf = uart::Config {
            baudrate: frostsnap_comms::BAUDRATE,
            rx_fifo_full_threshold: RX_FIFO_THRESHOLD,
            ..Default::default()
        };
        uart.apply_config(&serial_conf).unwrap();

        // Register UART for interrupt handling
        let (writer, consumer) = crate::uart_interrupt::register_uart(uart, uart_num);

        Self {
            io: SerialIo::Uart {
                writer,
                uart_num,
                consumer,
            },
            magic_bytes_progress: 0,
            timer,
            direction: PhantomData,
        }
    }

    pub fn is_jtag(&self) -> bool {
        matches!(self.io, SerialIo::Jtag { .. })
    }
}

impl<'a, T, D> SerialInterface<'a, T, D> {
    pub fn new_jtag(jtag: UsbSerialJtag<'a, Blocking>, timer: &'a T) -> Self {
        Self {
            io: SerialIo::Jtag {
                jtag,
                peek_byte: None,
            },
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
        // Let the SerialIo implementation handle filling its queue
        self.io.fill_queue();
    }

    pub fn find_and_remove_magic_bytes(&mut self) -> bool {
        self.fill_buffer();
        // Check if there's any data available
        if !self.io.has_data() {
            return false;
        }
        let magic_bytes_progress = self.magic_bytes_progress;
        let (progress, found) = frostsnap_comms::make_progress_on_magic_bytes::<D>(
            core::iter::from_fn(|| self.io.read_byte()),
            magic_bytes_progress,
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
        if self.io.has_data() {
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

    /// Read a single byte, polling if necessary
    /// This is used by OTA for byte-by-byte protocol handling
    pub fn read_byte_blocking(&mut self) -> u8 {
        loop {
            // First try to get a byte
            if let Some(byte) = self.io.read_byte() {
                return byte;
            }

            // If no byte available, fill the buffer and try again
            self.fill_buffer();
        }
    }

    /// Try to read a byte without blocking
    pub fn read_byte(&mut self) -> nb::Result<u8, core::convert::Infallible> {
        // Then try to read
        if let Some(byte) = self.io.read_byte() {
            Ok(byte)
        } else {
            Err(nb::Error::WouldBlock)
        }
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
                if let Some(next_byte) = self.io.read_byte() {
                    break next_byte;
                }

                self.fill_buffer();

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
        writer: UartWriter,
        uart_num: UartNum,
        consumer: UartReceiver,
    },
    Jtag {
        jtag: UsbSerialJtag<'a, Blocking>,
        peek_byte: Option<u8>,
    },
}

impl SerialIo<'_> {
    /// Check if data is available without consuming it
    pub fn has_data(&self) -> bool {
        match self {
            SerialIo::Uart { consumer, .. } => consumer.peek().is_some(),
            SerialIo::Jtag { peek_byte, .. } => peek_byte.is_some(),
        }
    }

    /// Internal method to read a byte from the appropriate source
    pub fn read_byte(&mut self) -> Option<u8> {
        match self {
            SerialIo::Uart {
                consumer, uart_num, ..
            } => match consumer.dequeue() {
                Some(byte) => Some(byte),
                None => {
                    fill_buffer(*uart_num);
                    consumer.dequeue()
                }
            },
            SerialIo::Jtag { jtag, peek_byte } => {
                // First check if we have a peeked byte
                if let Some(byte) = peek_byte.take() {
                    Some(byte)
                } else {
                    // Otherwise try to read directly
                    jtag.read_byte().ok()
                }
            }
        }
    }

    pub fn change_baud(&mut self, baudrate: u32) {
        self.flush();
        match self {
            SerialIo::Uart { writer, .. } => {
                writer.change_baud(baudrate);
            }
            SerialIo::Jtag { .. } => { /* no baud rate for USB jtag */ }
        }
    }
    pub fn fill_queue(&mut self) {
        match self {
            SerialIo::Uart { uart_num, .. } => {
                // Fill buffer with any bytes that haven't triggered an interrupt (< threshold)
                fill_buffer(*uart_num);
            }
            SerialIo::Jtag { jtag, peek_byte } => {
                // For JTAG, fill the peek byte if empty
                if peek_byte.is_none() {
                    *peek_byte = jtag.read_byte().ok();
                }
            }
        }
    }

    pub fn write_byte_nb(&mut self, byte: u8) -> nb::Result<(), Infallible> {
        match self {
            SerialIo::Jtag { jtag, .. } => jtag.write_byte_nb(byte),
            SerialIo::Uart { writer, .. } => {
                // write_bytes is blocking, so we need to check if there's space first
                // For now, use write_bytes and convert the error
                match writer.write_bytes(&[byte]) {
                    Ok(_) => Ok(()),
                    Err(_) => Err(nb::Error::WouldBlock), // Assume any error is WouldBlock
                }
            }
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), SerialInterfaceError> {
        match self {
            SerialIo::Uart { writer, .. } => {
                writer
                    .write_bytes(bytes)
                    .map_err(SerialInterfaceError::UartWriteError)?;
            }
            SerialIo::Jtag { jtag, .. } => {
                let _ = jtag.write_bytes(bytes);
            }
        }
        Ok(())
    }

    pub fn nb_flush(&mut self) {
        match self {
            SerialIo::Uart { .. } => {
                // there is no reason to call this on uart. It will just block until data is
                // actually written.
            }
            SerialIo::Jtag { jtag, .. } => {
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
            SerialIo::Uart { writer, .. } => {
                // just waits until evertything has been written
                while let Err(nb::Error::WouldBlock) = writer.flush_tx() {
                    // wait
                }
            }
            SerialIo::Jtag { jtag, .. } => {
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
