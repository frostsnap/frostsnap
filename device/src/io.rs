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

pub struct SerialInterface<'a, T, U, D> {
    io: SerialIo<'a, U>,
    read_buffer: Vec<u8>,
    timer: &'a Timer<T>,
    direction: PhantomData<D>,
}

impl<'a, T, U, D> SerialInterface<'a, T, U, D> {
    pub fn new_uart(uart: uart::Uart<'a, U>, timer: &'a Timer<T>) -> Self {
        Self {
            io: SerialIo::Uart(uart),
            read_buffer: vec![],
            timer,
            direction: PhantomData,
        }
    }

    pub fn read_buffer(&self) -> &[u8] {
        &self.read_buffer[..]
    }
}

impl<'a, T, U> SerialInterface<'a, T, U, Upstream> {
    pub fn new_jtag(jtag: UsbSerialJtag<'a, USB_DEVICE>, timer: &'a Timer<T>) -> Self {
        Self {
            io: SerialIo::Jtag(jtag),
            read_buffer: vec![],
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
        let read_buffer = &mut self.read_buffer;
        while let Ok(c) = self.io.read_byte() {
            read_buffer.push(c);
        }
        !read_buffer.is_empty()
    }

    pub fn find_and_remove_magic_bytes(&mut self) -> bool
    where
        D: Direction,
    {
        self.poll_read();
        frostsnap_comms::find_and_remove_magic_bytes::<D>(&mut self.read_buffer)
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
    ) -> Result<DeviceSendSerial, bincode::error::DecodeError> {
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
        message: DeviceSendSerial,
    ) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(&message, self, bincode::config::standard())
    }

    pub fn write_magic_bytes(&mut self) -> Result<(), bincode::error::EncodeError> {
        bincode::encode_into_writer(
            &DeviceSendSerial::MagicBytes(MagicBytes::default()),
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

pub struct UpstreamDetector<'a, T, U> {
    timer: &'a Timer<T>,
    switch_time: Option<u64>,
    pub switched: bool,
    state: DetectorState<'a, T, U>,
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
        jtag: UsbSerialJtag<'a, USB_DEVICE>,
        timer: &'a Timer<T>,
    ) -> Self {
        Self {
            timer,
            switch_time: None,
            switched: false,
            state: DetectorState::NotDetected {
                jtag: SerialInterface::new_jtag(jtag, timer),
                uart: SerialInterface::new_uart(uart, timer),
            },
        }
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

                    now + 40_000 * 1_000
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
