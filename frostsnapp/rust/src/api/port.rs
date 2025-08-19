use crate::frb_generated::StreamSink;
use anyhow::{anyhow, Result};
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::PortDesc;
use frostsnap_coordinator::{PortOpenError, Serial, SerialPort};
pub use std::sync::mpsc::SyncSender;
pub use std::sync::{Arc, Mutex, RwLock};
use tracing::{event, Level};

#[frb(mirror(PortDesc))]
pub struct _PortDesc {
    pub id: String,
    pub vid: u16,
    pub pid: u16,
}

#[derive(Debug)]
#[frb(opaque)]
pub struct PortOpen {
    pub id: String,
    ready: SyncSender<Result<i32, PortOpenError>>,
}

#[allow(dead_code)] // only used on android
#[derive(Clone, Default)]
#[frb(opaque)]
pub struct FfiSerial {
    pub(crate) available_ports: Arc<Mutex<Vec<PortDesc>>>,
    pub(crate) open_requests: Arc<Mutex<Option<StreamSink<PortOpen>>>>,
}

// ========== Android implementation
#[cfg(target_os = "android")]
mod android_impl {
    use super::*;
    use frostsnap_coordinator::cdc_acm_usb::CdcAcmSerial;
    use std::os::fd::OwnedFd;
    use std::os::unix::io::FromRawFd;

    impl PortOpen {
        pub fn satisfy(self, fd: i32, err: Option<String>) {
            let result = match err {
                Some(err) => Err(frostsnap_coordinator::PortOpenError::Other(err.into())),
                None => Ok(fd),
            };
            let _ = self.ready.send(result);
        }
    }

    impl FfiSerial {
        pub fn set_available_ports(&self, ports: Vec<PortDesc>) {
            event!(Level::INFO, "ports: {:?}", ports);
            *self.available_ports.lock().unwrap() = ports
        }

        pub fn sub_open_requests(&mut self, sink: StreamSink<PortOpen>) {
            if self.open_requests.lock().unwrap().replace(sink).is_some() {
                event!(Level::WARN, "resubscribing to open requests");
            }
        }
    }

    impl Serial for FfiSerial {
        fn available_ports(&self) -> Vec<PortDesc> {
            self.available_ports.lock().unwrap().clone()
        }

        fn open_device_port(&self, id: &str, baud_rate: u32) -> Result<SerialPort, PortOpenError> {
            let (tx, rx) = std::sync::mpsc::sync_channel(0);
            loop {
                let open_requests = self.open_requests.lock().unwrap();
                match &*open_requests {
                    Some(sink) => {
                        sink.add(PortOpen {
                            id: id.into(),
                            ready: tx,
                        })
                        .map_err(|e| PortOpenError::Other(anyhow!("sink error: {e}").into()))?;
                        break;
                    }
                    None => {
                        drop(open_requests);
                        event!(Level::WARN, "dart port open listener is not listening yet. blocking while waiting for it.");
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
            let raw_fd = rx.recv().map_err(|e| PortOpenError::Other(Box::new(e)))??;
            if raw_fd < 0 {
                return Err(PortOpenError::Other(
                    anyhow!("OS failed to open UBS device {id}: FD was < 0").into(),
                ));
            }
            let fd =
                // SAFETY: on the host side (e.g. android) we've dup'd and detached this file
                // descriptor. we're the only owner of it at the moment so it's fine for us to turn it
                // into an `OwnedFd` which will close it when it's dropped.
                unsafe { OwnedFd::from_raw_fd(raw_fd) };
            let cdc_acm = CdcAcmSerial::new_auto(fd, id.to_string(), baud_rate)
                .map_err(|e| PortOpenError::Other(e.into()))?;
            Ok(Box::new(cdc_acm))
        }
    }
}

// ========== Stub implementation for non-Android platforms
#[cfg(not(target_os = "android"))]
mod stub_impl {
    use super::*;

    impl PortOpen {
        pub fn satisfy(self, _fd: i32, _err: Option<String>) {
            event!(
                Level::WARN,
                "PortOpen::satisfy called on non-Android platform - this is a no-op"
            );
            // Send an error result to the channel to unblock any waiters
            let _ = self.ready.send(Err(PortOpenError::Other(
                anyhow!("PortOpen not supported on this platform").into(),
            )));
        }
    }

    impl FfiSerial {
        pub fn set_available_ports(&self, _ports: Vec<PortDesc>) {
            event!(
                Level::WARN,
                "FfiSerial::set_available_ports called on non-Android platform"
            );
        }

        pub fn sub_open_requests(&mut self, _sink: StreamSink<PortOpen>) {
            event!(
                Level::WARN,
                "FfiSerial::sub_open_requests called on non-Android platform"
            );
        }
    }

    impl Serial for FfiSerial {
        fn available_ports(&self) -> Vec<PortDesc> {
            Vec::new()
        }

        fn open_device_port(
            &self,
            _id: &str,
            _baud_rate: u32,
        ) -> Result<SerialPort, PortOpenError> {
            Err(PortOpenError::Other(
                anyhow!("FfiSerial not supported on this platform").into(),
            ))
        }
    }
}
