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

pub struct RingBuffer<'a> {
    buffer: &'a mut [u8],
    head: usize,
    tail: usize,
}

impl<'a> RingBuffer<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            head: 0,
            tail: 0,
        }
    }

    fn buffer_size(&self) -> usize {
        self.buffer.len()
    }

    fn buffer_filled(&self) -> usize {
        (self.buffer_size() + self.tail - self.head) % self.buffer_size()
    }

    pub fn ingest(&mut self, c: u8) {
        self.buffer[self.tail] = c;
        self.tail = (self.tail + 1) % self.buffer_size();

        if self.tail == self.head {
            self.head = (self.head + 1) % self.buffer_size();
        }
    }

    pub fn peek_buffer(&self, n: usize) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        let mut peeking_head = self.head;
        while self.buffer_filled() > 0 && bytes.len() < n {
            bytes.push(self.buffer[peeking_head]);
            peeking_head = (peeking_head + 1) % self.buffer_size();
        }
        bytes
    }

    pub fn read_out(&mut self, n: usize) -> Vec<u8> {
        let bytes = self.peek_buffer(n);
        self.head = (self.head + bytes.len()) % self.buffer_size();
        bytes
    }

    pub fn read_all(&mut self) -> Vec<u8> {
        self.read_out(self.buffer_filled())
    }
}

pub struct SerialInterface<'a, T, U, D> {
    io: SerialIo<'a, U>,
    ring_buffer: RingBuffer<'a>,
    timer: &'a Timer<T>,
    direction: PhantomData<D>,
}

impl<'a, T, U, D> SerialInterface<'a, T, U, D> {
    pub fn new_uart(uart: uart::Uart<'a, U>, timer: &'a Timer<T>, buffer: &'a mut [u8]) -> Self {
        Self {
            io: SerialIo::Uart(uart),
            ring_buffer: RingBuffer::new(buffer),
            timer,
            direction: PhantomData,
        }
    }

    pub fn peek_buffer(&mut self) -> Vec<u8> {
        self.ring_buffer
            .peek_buffer(self.ring_buffer.buffer_filled())
    }
}

impl<'a, T, U> SerialInterface<'a, T, U, Upstream> {
    pub fn new_jtag(jtag: UsbSerialJtag<'a>, timer: &'a Timer<T>, buffer: &'a mut [u8]) -> Self {
        Self {
            io: SerialIo::Jtag(jtag),
            ring_buffer: RingBuffer::new(buffer),
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
            self.ring_buffer.ingest(c);
            read_something = true;
        }
        read_something
    }

    pub fn find_and_remove_magic_bytes(&mut self) -> bool
    where
        D: Direction,
    {
        self.poll_read();
        let (consumed, found) = frostsnap_comms::find_magic_bytes::<D>(
            &mut self
                .ring_buffer
                .peek_buffer(self.ring_buffer.buffer_filled()),
        );
        // Doing consuming out here for now
        if found {
            let _magic_bytes = self.ring_buffer.read_out(consumed);
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

        while self.ring_buffer.buffer_filled() < bytes.len() {
            self.poll_read();
            if (self.timer.now() - start_time) / 40_000 > 1_000 {
                return Err(DecodeError::UnexpectedEnd {
                    additional: bytes.len() - self.ring_buffer.buffer_filled(),
                });
            }
        }

        let read_bytes = self.ring_buffer.read_out(bytes.len());
        bytes.copy_from_slice(&read_bytes);

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
        buffer_uart: &'a mut [u8],
        buffer_jtag: &'a mut [u8],
        magic_bytes_period: u64, // after how many ms is magic bytes sent again
    ) -> Self {
        Self {
            timer,
            switch_time: None,
            switched: false,
            state: DetectorState::NotDetected {
                jtag: SerialInterface::new_jtag(jtag, timer, buffer_jtag),
                uart: SerialInterface::new_uart(uart, timer, buffer_uart),
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
