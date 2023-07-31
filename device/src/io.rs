extern crate alloc;
use core::marker::PhantomData;

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
use frostsnap_comms::DeviceReceiveSerial;
use frostsnap_comms::DeviceSendSerial;
use frostsnap_comms::Direction;
use frostsnap_comms::Downstream;
use frostsnap_comms::MagicBytes;
use frostsnap_comms::Upstream;
use ringbuffer::RingBufferExt;
use ringbuffer::RingBufferRead;
use ringbuffer::RingBufferWrite;
use ringbuffer::{AllocRingBuffer, RingBuffer};

pub const RING_BUFFER_SIZE: usize = 2usize.pow(13);

pub struct SerialInterface<'a, T, U, D> {
    io: SerialIo<'a, U>,
    ring_buffer: AllocRingBuffer<u8>,
    magic_bytes_progress: usize,
    timer: &'a Timer<T>,
    direction: PhantomData<D>,
}

impl<'a, T, U, D> SerialInterface<'a, T, U, D> {
    pub fn new_uart(uart: uart::Uart<'a, U>, timer: &'a Timer<T>) -> Self {
        Self {
            io: SerialIo::Uart(uart),
            ring_buffer: AllocRingBuffer::with_capacity(RING_BUFFER_SIZE),
            magic_bytes_progress: 0,
            timer,
            direction: PhantomData,
        }
    }

    pub fn clone_buffer_to_vec(&self) -> Vec<u8> {
        self.ring_buffer.clone().drain().collect::<Vec<u8>>()
    }
}

impl<'a, T, U> SerialInterface<'a, T, U, Upstream> {
    pub fn new_jtag(jtag: UsbSerialJtag<'a>, timer: &'a Timer<T>) -> Self {
        Self {
            io: SerialIo::Jtag(jtag),
            ring_buffer: AllocRingBuffer::with_capacity(RING_BUFFER_SIZE),
            magic_bytes_progress: 0,
            timer,
            direction: PhantomData,
        }
    }
}

impl<'a, T, U, D> SerialInterface<'a, T, U, D>
where
    U: uart::Instance,
    T: esp32c3_hal::timer::Instance,
{
    pub fn poll_read(&mut self) -> bool {
        let mut read_something = false;
        while let Ok(c) = self.io.read_byte() {
            self.ring_buffer.push(c);
            read_something = true;
        }
        read_something
    }

    pub fn find_and_remove_magic_bytes(&mut self) -> bool
    where
        D: Direction,
    {
        self.poll_read();
        if self.ring_buffer.len() == 0 {
            return false;
        }

        let (consume, progress, found) = frostsnap_comms::make_progress_on_magic_bytes::<D>(
            &self.clone_buffer_to_vec()[..],
            self.magic_bytes_progress,
        );
        self.magic_bytes_progress = progress;
        for _ in 0..consume {
            self.ring_buffer.skip();
        }
        found
    }
}

impl<'a, T, U> SerialInterface<'a, T, U, Downstream>
where
    U: uart::Instance,
    T: esp32c3_hal::timer::Instance,
{
    pub fn forward_downstream(
        &mut self,
        message: DeviceReceiveSerial<Downstream>,
    ) -> Result<(), bincode::error::EncodeError> {
        assert!(
            !matches!(message, DeviceReceiveSerial::MagicBytes(_)),
            "we never forward magic bytes"
        );
        bincode::encode_into_writer(&message, self, bincode::config::standard())
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            &DeviceReceiveSerial::<Downstream>::MagicBytes(MagicBytes::default()),
            self,
            bincode::config::standard(),
        )
    }

    pub fn receive_from_downstream(
        &mut self,
    ) -> Result<DeviceSendSerial<Downstream>, bincode::error::DecodeError> {
        bincode::decode_from_reader(self, bincode::config::standard())
    }
}

impl<'a, T, U> SerialInterface<'a, T, U, Upstream>
where
    U: uart::Instance,
    T: esp32c3_hal::timer::Instance,
{
    pub fn send_to_coodinator(
        &mut self,
        message: DeviceSendSerial<Upstream>,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(&message, self, bincode::config::standard())
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            &DeviceSendSerial::<Upstream>::MagicBytes(MagicBytes::default()),
            self,
            bincode::config::standard(),
        )
    }

    pub fn receive_from_coordinator(
        &mut self,
    ) -> Result<DeviceReceiveSerial<Upstream>, bincode::error::DecodeError> {
        bincode::decode_from_reader(self, bincode::config::standard())
    }
}

impl<'a, T, U, D> Reader for SerialInterface<'a, T, U, D>
where
    U: uart::Instance,
    T: esp32c3_hal::timer::Instance,
{
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecodeError> {
        let start_time = self.timer.now();

        while self.ring_buffer.len() < bytes.len() {
            self.poll_read();
            if (self.timer.now() - start_time) / 40_000 > 1_000 {
                return Err(DecodeError::UnexpectedEnd {
                    additional: bytes.len() - self.ring_buffer.len(),
                });
            }
        }

        for target_byte in bytes.iter_mut() {
            if let Some(byte) = self.ring_buffer.dequeue() {
                *target_byte = byte;
            } else {
                panic!("Less bytes exist than we just read into buffer!");
            }
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
            Err(e) => return Err(EncodeError::OtherString(format!("{:?}", e))),
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

    // NOTE: flush is useless on these devices except for blocking until writing is finished.
    // This comment is here to stop you thinking it's useful and re-implementing it.
    // fn flush(&mut self)
    // where
    //     U: uart::Instance,
    // {
    //     match self {
    //         SerialIo::Uart(uart) => {
    //             while let Err(_) = uart.flush() {}
    //         }
    //         SerialIo::Jtag(jtag) => {
    //             let _ = jtag.flush().unwrap();
    //         },
    //     }
    // }
}

#[derive(Debug)]
pub enum SerialInterfaceError {
    UartReadError,
    UartWriteError(uart::Error),
    JtagError,
}

pub struct UpstreamDetector<'a, T, U> {
    timer: &'a Timer<T>,
    switch_time: Option<u64>,
    pub switched: bool,
    state: DetectorState<'a, T, U>,
    magic_bytes_freq: u64,
}

pub enum DetectorState<'a, T, U> {
    Unreachable,
    Detected(SerialInterface<'a, T, U, Upstream>),
    NotDetected {
        jtag: SerialInterface<'a, T, U, Upstream>,
        uart: SerialInterface<'a, T, U, Upstream>,
    },
}

impl<'a, T, U> UpstreamDetector<'a, T, U> {
    pub fn new(
        uart: uart::Uart<'a, U>,
        jtag: UsbSerialJtag<'a>,
        timer: &'a Timer<T>,
        magic_bytes_period: u64, // after how many ms is magic bytes sent again
    ) -> Self {
        Self {
            timer,
            switch_time: None,
            switched: false,
            state: DetectorState::NotDetected {
                jtag: SerialInterface::new_jtag(jtag, timer),
                uart: SerialInterface::new_uart(uart, timer),
            },
            magic_bytes_freq: magic_bytes_period,
        }
    }

    pub fn looking_at_jtag(&self) -> bool {
        self.switched
    }

    pub fn serial_interface(&mut self) -> Option<&mut SerialInterface<'a, T, U, Upstream>>
    where
        T: esp32c3_hal::timer::Instance,
        U: uart::Instance,
    {
        self.poll();
        match &mut self.state {
            DetectorState::Detected(serial_interface) => Some(serial_interface),
            _ => None,
        }
    }

    pub fn poll(&mut self)
    where
        T: esp32c3_hal::timer::Instance,
        U: uart::Instance,
    {
        let state = core::mem::replace(&mut self.state, DetectorState::Unreachable);

        match state {
            DetectorState::Unreachable => unreachable!(),
            DetectorState::Detected(_) => {
                self.state = state;
            }
            DetectorState::NotDetected { mut jtag, mut uart } => {
                let now = self.timer.now();
                let switch_time = self.switch_time.get_or_insert_with(|| {
                    // Frist time it's run we set it the interface to uart
                    let usb_device = unsafe { &*USB_DEVICE::PTR };

                    usb_device
                        .conf0
                        .modify(|_, w| w.usb_pad_enable().clear_bit());
                    now + 40_000 * (self.magic_bytes_freq + self.magic_bytes_freq / 2)
                });

                self.state = if now > *switch_time {
                    if !self.switched {
                        let usb_device = unsafe { &*USB_DEVICE::PTR };
                        usb_device.conf0.modify(|_, w| w.usb_pad_enable().set_bit());
                        self.switched = true;
                    }
                    if jtag.find_and_remove_magic_bytes() {
                        DetectorState::Detected(jtag)
                    } else {
                        DetectorState::NotDetected { uart, jtag }
                    }
                } else {
                    if uart.find_and_remove_magic_bytes() {
                        DetectorState::Detected(uart)
                    } else {
                        DetectorState::NotDetected { uart, jtag }
                    }
                };
            }
        };
    }
}
