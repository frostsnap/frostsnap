//! Portable serial framing: bincode `ReceiveSerial<D>` over a byte transport,
//! with the magic-bytes handshake. Lifted from the esp `SerialInterface`; the
//! esp-specific transport (UART/JTAG, interrupt queue) implements `ByteIo`, and
//! the per-byte read timeout reads a `Clock` instead of a TIMG timer.

use crate::device_hal::Clock;
use bincode::de::read::Reader;
use bincode::enc::write::Writer;
use bincode::error::{DecodeError, EncodeError};
use core::marker::PhantomData;
use frostsnap_comms::{Direction, MagicBytes, ReceiveSerial, BINCODE_CONFIG};

/// The transport failed to accept the bytes (link gone or its queue wedged).
/// Carries no detail: no caller can act on more than message-fatal.
#[derive(Debug, Clone, Copy)]
pub struct WriteError;

/// Byte-level transport the framing rides on. esp: UART interrupt queue / USB-JTAG;
/// host/sim: an in-memory pipe.
pub trait ByteIo {
    /// Pop one buffered byte, if any (non-blocking).
    fn read_byte(&mut self) -> Option<u8>;
    /// Whether a byte is currently buffered (may top up an internal queue first).
    fn has_data(&mut self) -> bool;
    /// Top up any internal receive buffer from the underlying device.
    fn fill(&mut self);
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), WriteError>;
    /// Non-blocking best-effort flush (esp JTAG needs it; UART is a no-op).
    fn nb_flush(&mut self);
    /// Blocking flush — only before a reset, to ensure bytes left the wire.
    fn flush(&mut self);
    /// Switch line rate (esp OTA baud change; sim/JTAG no-op).
    fn set_baud(&mut self, baud: u32);
}

/// The framing surface the run loop drives. `H::Upstream`/`H::Downstream` are
/// bound on this (not on the raw byte transport) because the loop calls framing
/// ops and the magic-bytes progress is persistent state owned by the serial.
/// `FramedSerial` is the implementation; the esp `SerialIo` (and a sim pipe) are
/// the `ByteIo` transports underneath it.
pub trait SerialPort<D: Direction> {
    fn find_and_remove_magic_bytes(&mut self) -> bool;
    fn send(&mut self, message: <D::Opposite as Direction>::RecvType) -> Result<(), EncodeError>;
    fn receive(&mut self) -> Option<Result<ReceiveSerial<D>, DecodeError>>
    where
        ReceiveSerial<D>: bincode::Decode<()>;
    fn write_magic_bytes(&mut self) -> Result<(), EncodeError>;
    fn write_conch(&mut self) -> Result<(), EncodeError>;
    fn send_reset_signal(&mut self) -> Result<(), EncodeError>;
    fn flush(&mut self);
    /// Switch the underlying line rate (esp OTA baud change; sim no-op).
    fn set_baud(&mut self, baud: u32);
    /// The raw byte transport, for the firmware-upgrade takeover (which streams
    /// raw/control bytes outside the bincode framing). `dyn` so `FirmwareServices`
    /// can drive it without depending on concrete `FramedSerial`/transport types.
    fn raw(&mut self) -> &mut dyn ByteIo;
}

pub struct FramedSerial<IO, C, D> {
    io: IO,
    clock: C,
    magic_bytes_progress: usize,
    direction: PhantomData<D>,
}

impl<IO: ByteIo, C: Clock, D: Direction> FramedSerial<IO, C, D> {
    pub fn new(io: IO, clock: C) -> Self {
        Self {
            io,
            clock,
            magic_bytes_progress: 0,
            direction: PhantomData,
        }
    }

    pub fn inner_mut(&mut self) -> &mut IO {
        &mut self.io
    }

    pub fn fill_buffer(&mut self) {
        self.io.fill();
    }

    pub fn find_and_remove_magic_bytes(&mut self) -> bool {
        self.fill_buffer();
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
    ) -> Result<(), EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::Message(message),
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.io.nb_flush();
        Ok(())
    }

    pub fn receive(&mut self) -> Option<Result<ReceiveSerial<D>, DecodeError>>
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

    pub fn write_magic_bytes(&mut self) -> Result<(), EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::MagicBytes(MagicBytes::default()),
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.io.nb_flush();
        Ok(())
    }

    pub fn write_conch(&mut self) -> Result<(), EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::Conch,
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.io.nb_flush();
        Ok(())
    }

    pub fn send_reset_signal(&mut self) -> Result<(), EncodeError> {
        bincode::encode_into_writer(
            ReceiveSerial::<D::Opposite>::Reset,
            &mut *self,
            BINCODE_CONFIG,
        )?;
        self.flush();
        Ok(())
    }

    /// Blocking flush.
    pub fn flush(&mut self) {
        self.io.flush();
    }
}

impl<IO: ByteIo, C: Clock, D: Direction> SerialPort<D> for FramedSerial<IO, C, D> {
    fn find_and_remove_magic_bytes(&mut self) -> bool {
        FramedSerial::find_and_remove_magic_bytes(self)
    }
    fn send(&mut self, message: <D::Opposite as Direction>::RecvType) -> Result<(), EncodeError> {
        FramedSerial::send(self, message)
    }
    fn receive(&mut self) -> Option<Result<ReceiveSerial<D>, DecodeError>>
    where
        ReceiveSerial<D>: bincode::Decode<()>,
    {
        FramedSerial::receive(self)
    }
    fn write_magic_bytes(&mut self) -> Result<(), EncodeError> {
        FramedSerial::write_magic_bytes(self)
    }
    fn write_conch(&mut self) -> Result<(), EncodeError> {
        FramedSerial::write_conch(self)
    }
    fn send_reset_signal(&mut self) -> Result<(), EncodeError> {
        FramedSerial::send_reset_signal(self)
    }
    fn flush(&mut self) {
        FramedSerial::flush(self)
    }
    fn set_baud(&mut self, baud: u32) {
        self.io.set_baud(baud);
    }
    fn raw(&mut self) -> &mut dyn ByteIo {
        &mut self.io
    }
}

impl<IO: ByteIo, C: Clock, D: Direction> Reader for FramedSerial<IO, C, D> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        for (i, target_byte) in bytes.iter_mut().enumerate() {
            let start_ms = self.clock.now_ms();
            *target_byte = loop {
                if let Some(next_byte) = self.io.read_byte() {
                    break next_byte;
                }
                self.io.fill();
                if self.clock.now_ms().saturating_sub(start_ms) > 1_000 {
                    return Err(DecodeError::UnexpectedEnd {
                        additional: bytes.len() - i + 1,
                    });
                }
            };
        }
        Ok(())
    }
}

impl<IO: ByteIo, C, D> Writer for FramedSerial<IO, C, D> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.io
            .write_bytes(bytes)
            .map_err(|WriteError| EncodeError::Other("serial write failed"))
    }
}
