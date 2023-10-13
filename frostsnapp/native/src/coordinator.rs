use crate::api::PortEvent;
use flutter_rust_bridge::RustOpaque;
use frostsnap_coordinator::serialport;
use frostsnap_coordinator::{
    frostsnap_core, DesktopSerial, PortChanges, PortDesc, PortOpenError, Serial, SerialPort,
    UsbSerialManager,
};
use frostsnap_core::{DeviceId, FrostCoordinator};
use std::collections::VecDeque;
use std::io;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{event, Level};

pub struct FfiCoordinator {
    ffi_serial: Option<FfiSerial>,
    manager: Arc<Mutex<UsbSerialManager>>,
}

impl FfiCoordinator {
    pub fn new(host_handles_serial: bool) -> Self {
        let mut core_coordinator = FrostCoordinator::new();
        let (manager, ffi_serial) = if host_handles_serial {
            let ffi_serial = FfiSerial::default();
            (
                UsbSerialManager::new(Box::new(ffi_serial.clone())),
                Some(ffi_serial),
            )
        } else {
            (UsbSerialManager::new(Box::new(DesktopSerial)), None)
        };

        let my_manager = Arc::new(Mutex::new(manager));
        let loop_manager = Arc::clone(&my_manager);

        let _handle = std::thread::spawn(move || {
            let mut outbox = VecDeque::new();
            loop {
                let new_messages = {
                    let PortChanges {
                        device_changes,
                        new_messages,
                    } = { loop_manager.lock().unwrap().poll_ports() };

                    if !device_changes.is_empty() {
                        crate::api::emit_device_events(
                            device_changes
                                .into_iter()
                                .map(crate::api::DeviceChange::from)
                                .collect(),
                        );
                    }

                    new_messages
                };

                for (from, message) in new_messages {
                    match core_coordinator.recv_device_message(from, message.clone()) {
                        Ok(messages) => {
                            outbox.extend(messages);
                        }
                        Err(e) => {
                            event!(
                                Level::ERROR,
                                from = from.to_string(),
                                "Failed to process message: {}",
                                e
                            );
                            continue;
                        }
                    };
                }

                // to give time for the other threads to get a lock
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        Self {
            ffi_serial,
            manager: my_manager,
        }
    }

    pub fn set_available_ports(&self, ports: Vec<PortDesc>) {
        *self
            .ffi_serial
            .as_ref()
            .unwrap()
            .available_ports
            .lock()
            .unwrap() = ports;
    }

    pub fn update_name_preview(&self, id: DeviceId, name: &str) {
        self.manager.lock().unwrap().update_name_preview(id, name);
    }

    pub fn finish_naming(&self, id: DeviceId, name: &str) {
        self.manager.lock().unwrap().finish_naming(id, name);
    }

    pub fn send_cancel(&self, id: DeviceId) {
        self.manager.lock().unwrap().send_cancel(id)
    }
}

// Newtypes needed here because type aliases lead to weird types in the bindings
#[derive(Debug)]
pub struct PortOpenSender(pub SyncSender<Result<(), PortOpenError>>);
#[derive(Debug)]
pub struct PortWriteSender(pub SyncSender<Result<(), String>>);
#[derive(Debug)]
pub struct PortReadSender(pub SyncSender<Result<Vec<u8>, String>>);
#[derive(Debug)]
pub struct PortBytesToReadSender(pub SyncSender<u32>);

#[derive(Debug, Default, Clone)]
pub struct FfiSerial {
    available_ports: Arc<Mutex<Vec<PortDesc>>>,
}

impl Serial for FfiSerial {
    fn available_ports(&self) -> Vec<PortDesc> {
        self.available_ports.lock().unwrap().clone()
    }

    fn open_device_port(&self, id: &str, baud_rate: u32) -> Result<SerialPort, PortOpenError> {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);

        crate::api::emit_event(PortEvent::Open {
            request: crate::api::PortOpen {
                id: id.into(),
                baud_rate,
                ready: RustOpaque::new(PortOpenSender(tx)),
            },
        })
        .map_err(|e| PortOpenError::Other(e.into()))?;

        rx.recv().map_err(|e| PortOpenError::Other(Box::new(e)))??;

        let port = FfiSerialPort {
            id: id.to_string(),
            baud_rate,
        };

        Ok(Box::new(port))
    }
}

pub struct FfiSerialPort {
    id: String,
    baud_rate: u32,
}

impl io::Read for FfiSerialPort {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            let (tx, rx) = std::sync::mpsc::sync_channel::<Result<Vec<u8>, String>>(0);
            crate::api::emit_event(PortEvent::Read {
                request: crate::api::PortRead {
                    id: self.id.clone(),
                    len: buf.len(),
                    ready: RustOpaque::new(PortReadSender(tx)),
                },
            })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e.to_string()))?;

            let result = rx.recv().unwrap();
            match result {
                Ok(bytes) => {
                    if !bytes.is_empty() {
                        buf[0..bytes.len()].copy_from_slice(&bytes);
                        return Ok(bytes.len());
                    } else {
                        // we got 0 bytes so wait a little while for more data
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
                Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),
            }
        }
    }
}

impl std::io::Write for FfiSerialPort {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let (tx, rx) = std::sync::mpsc::sync_channel(0);

        crate::api::emit_event(PortEvent::Write {
            request: crate::api::PortWrite {
                id: self.id.clone(),
                bytes: buf.to_vec(),
                ready: RustOpaque::new(PortWriteSender(tx)),
            },
        })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e.to_string()))?;

        match rx.recv().unwrap() {
            Ok(()) => Ok(buf.len()),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        // assume FFI host will flush after each write
        Ok(())
    }
}

mod _impl {
    use super::serialport::*;
    use super::{PortBytesToReadSender, PortEvent, RustOpaque};

    #[allow(unused)]
    impl SerialPort for super::FfiSerialPort {
        fn name(&self) -> Option<String> {
            Some(self.id.clone())
        }
        fn baud_rate(&self) -> Result<u32> {
            Ok(self.baud_rate)
        }
        fn bytes_to_read(&self) -> Result<u32> {
            let (tx, rx) = std::sync::mpsc::sync_channel(0);

            crate::api::emit_event(PortEvent::BytesToRead {
                request: crate::api::PortBytesToRead {
                    id: self.id.clone(),
                    ready: RustOpaque::new(PortBytesToReadSender(tx)),
                },
            })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e.to_string()))?;

            Ok(rx.recv().unwrap())
        }

        fn data_bits(&self) -> Result<DataBits> {
            unimplemented!()
        }

        fn flow_control(&self) -> Result<FlowControl> {
            unimplemented!()
        }

        fn parity(&self) -> Result<Parity> {
            unimplemented!()
        }

        fn stop_bits(&self) -> Result<StopBits> {
            unimplemented!()
        }

        fn timeout(&self) -> core::time::Duration {
            unimplemented!()
        }

        fn set_baud_rate(&mut self, baud_rate: u32) -> Result<()> {
            unimplemented!()
        }

        fn set_data_bits(&mut self, data_bits: DataBits) -> Result<()> {
            unimplemented!()
        }

        fn set_flow_control(&mut self, flow_control: FlowControl) -> Result<()> {
            unimplemented!()
        }

        fn set_parity(&mut self, parity: Parity) -> Result<()> {
            unimplemented!()
        }

        fn set_stop_bits(&mut self, stop_bits: StopBits) -> Result<()> {
            unimplemented!()
        }

        fn set_timeout(&mut self, timeout: core::time::Duration) -> Result<()> {
            unimplemented!()
        }

        fn write_request_to_send(&mut self, level: bool) -> Result<()> {
            unimplemented!()
        }

        fn write_data_terminal_ready(&mut self, level: bool) -> Result<()> {
            unimplemented!()
        }

        fn read_clear_to_send(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_data_set_ready(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_ring_indicator(&mut self) -> Result<bool> {
            unimplemented!()
        }

        fn read_carrier_detect(&mut self) -> Result<bool> {
            unimplemented!()
        }
        fn bytes_to_write(&self) -> Result<u32> {
            unimplemented!()
        }

        fn clear(&self, buffer_to_clear: ClearBuffer) -> Result<()> {
            unimplemented!()
        }

        fn try_clone(&self) -> Result<Box<dyn SerialPort>> {
            unimplemented!()
        }

        fn set_break(&self) -> Result<()> {
            unimplemented!()
        }

        fn clear_break(&self) -> Result<()> {
            unimplemented!()
        }
    }
}
