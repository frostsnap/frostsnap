//! A `Serial` that reaches a device through a remote hardware lab's serial bridge, so a
//! coordinator on this machine can drive the lab's device. `delay` holds back every byte
//! in each direction, to see how much latency the protocol tolerates.

use crate::usb_serial_manager::{USB_PID, USB_VID};
use crate::{PortDesc, PortOpenError, Serial, SerialPort};
use anyhow::{bail, Result};
use frostsnap_comms::BAUDRATE;
use serialport::{ClearBuffer, DataBits, FlowControl, Parity, StopBits};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Message;

#[derive(Default)]
struct Link {
    /// Counts the times the lab has opened the device's port, so a reopened port looks new
    generation: u64,
    open: bool,
    /// The WebSocket is gone for good
    closed: bool,
    delay: Duration,
    /// Bytes from the device, each chunk readable from its instant on
    incoming: VecDeque<(Instant, Vec<u8>)>,
    /// Bytes for the device, each chunk sent from its instant on
    outgoing: VecDeque<(Instant, Vec<u8>)>,
}

#[derive(Clone)]
pub struct RemoteSerial(Arc<(Mutex<Link>, Condvar)>);

impl RemoteSerial {
    /// `url` is the bridge, e.g. `wss://lab.example.com/<path>/api/serial?control=...`.
    pub fn connect(url: &str, delay: Duration) -> Result<Self> {
        let (mut socket, _) = tungstenite::connect(url)?;
        let stream = match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => stream,
            MaybeTlsStream::Rustls(stream) => stream.get_mut(),
            _ => bail!("unsupported bridge stream"),
        };
        // Short reads so queued bytes go out promptly
        stream.set_read_timeout(Some(Duration::from_millis(2)))?;
        stream.set_nodelay(true)?;
        let shared = Arc::new((
            Mutex::new(Link {
                delay,
                ..Default::default()
            }),
            Condvar::new(),
        ));
        std::thread::spawn({
            let shared = shared.clone();
            move || pump(socket, &shared)
        });
        Ok(Self(shared))
    }
}

fn pump(
    mut socket: tungstenite::WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    shared: &(Mutex<Link>, Condvar),
) {
    let (lock, cvar) = shared;
    let close = |reason: String| {
        tracing::warn!("bridge: {reason}");
        lock.lock().unwrap().closed = true;
        cvar.notify_all();
    };
    loop {
        let due: Vec<Vec<u8>> = {
            let mut link = lock.lock().unwrap();
            let now = Instant::now();
            let mut due = vec![];
            while link.outgoing.front().is_some_and(|(at, _)| *at <= now) {
                due.push(link.outgoing.pop_front().unwrap().1);
            }
            due
        };
        for bytes in due {
            if let Err(e) = socket.send(Message::Binary(bytes.into())) {
                return close(e.to_string());
            }
        }
        match socket.read() {
            Ok(Message::Binary(bytes)) => {
                let mut link = lock.lock().unwrap();
                let at = Instant::now() + link.delay;
                link.incoming.push_back((at, bytes.to_vec()));
                cvar.notify_all();
            }
            Ok(Message::Text(text)) => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                let mut link = lock.lock().unwrap();
                match value["type"].as_str() {
                    Some("open") => {
                        link.generation += 1;
                        link.open = true;
                        link.incoming.clear();
                        link.outgoing.clear();
                    }
                    Some("closed") => link.open = false,
                    _ => {
                        drop(link);
                        return close(text.to_string());
                    }
                }
                cvar.notify_all();
            }
            Ok(Message::Close(_)) => return close("the lab closed the bridge".into()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return close(e.to_string()),
        }
    }
}

impl Serial for RemoteSerial {
    fn available_ports(&self) -> Vec<PortDesc> {
        let link = self.0 .0.lock().unwrap();
        if !link.open || link.closed {
            return vec![];
        }
        vec![PortDesc {
            id: format!("lab-bridge-{}", link.generation),
            vid: USB_VID,
            pid: USB_PID,
        }]
    }

    fn open_device_port(&self, id: &str, _baud_rate: u32) -> Result<SerialPort, PortOpenError> {
        let link = self.0 .0.lock().unwrap();
        if id != format!("lab-bridge-{}", link.generation) || !link.open || link.closed {
            return Err(PortOpenError::Other("the lab's port is gone".into()));
        }
        Ok(Box::new(RemotePort {
            shared: self.0.clone(),
            generation: link.generation,
            timeout: Duration::from_secs(5),
        }))
    }
}

struct RemotePort {
    shared: Arc<(Mutex<Link>, Condvar)>,
    generation: u64,
    timeout: Duration,
}

impl RemotePort {
    fn gone(&self, link: &Link) -> bool {
        link.closed || !link.open || link.generation != self.generation
    }
}

impl Read for RemotePort {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let (lock, cvar) = &*self.shared;
        let deadline = Instant::now() + self.timeout;
        let mut link = lock.lock().unwrap();
        loop {
            if self.gone(&link) {
                return Err(std::io::ErrorKind::BrokenPipe.into());
            }
            let now = Instant::now();
            let mut wake = deadline;
            if let Some((at, chunk)) = link.incoming.front_mut() {
                if *at <= now {
                    let n = buf.len().min(chunk.len());
                    buf[..n].copy_from_slice(&chunk[..n]);
                    chunk.drain(..n);
                    if chunk.is_empty() {
                        link.incoming.pop_front();
                    }
                    return Ok(n);
                }
                wake = wake.min(*at);
            }
            if now >= deadline {
                return Err(std::io::ErrorKind::TimedOut.into());
            }
            link = cvar.wait_timeout(link, wake - now).unwrap().0;
        }
    }
}

impl Write for RemotePort {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut link = self.shared.0.lock().unwrap();
        if self.gone(&link) {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        let at = Instant::now() + link.delay;
        link.outgoing.push_back((at, buf.to_vec()));
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn unsupported<T>() -> serialport::Result<T> {
    Err(serialport::Error::new(
        serialport::ErrorKind::Unknown,
        "not supported over the lab bridge",
    ))
}

impl serialport::SerialPort for RemotePort {
    fn name(&self) -> Option<String> {
        Some(format!("lab-bridge-{}", self.generation))
    }
    fn baud_rate(&self) -> serialport::Result<u32> {
        Ok(BAUDRATE)
    }
    fn data_bits(&self) -> serialport::Result<DataBits> {
        Ok(DataBits::Eight)
    }
    fn flow_control(&self) -> serialport::Result<FlowControl> {
        Ok(FlowControl::None)
    }
    fn parity(&self) -> serialport::Result<Parity> {
        Ok(Parity::None)
    }
    fn stop_bits(&self) -> serialport::Result<StopBits> {
        Ok(StopBits::One)
    }
    fn timeout(&self) -> Duration {
        self.timeout
    }
    fn set_baud_rate(&mut self, _: u32) -> serialport::Result<()> {
        Ok(())
    }
    fn set_data_bits(&mut self, _: DataBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_flow_control(&mut self, _: FlowControl) -> serialport::Result<()> {
        Ok(())
    }
    fn set_parity(&mut self, _: Parity) -> serialport::Result<()> {
        Ok(())
    }
    fn set_stop_bits(&mut self, _: StopBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_timeout(&mut self, timeout: Duration) -> serialport::Result<()> {
        self.timeout = timeout;
        Ok(())
    }
    fn write_request_to_send(&mut self, _: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn write_data_terminal_ready(&mut self, _: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn read_clear_to_send(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_data_set_ready(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_ring_indicator(&mut self) -> serialport::Result<bool> {
        Ok(false)
    }
    fn read_carrier_detect(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn bytes_to_read(&self) -> serialport::Result<u32> {
        let link = self.shared.0.lock().unwrap();
        if self.gone(&link) {
            // The coordinator then reads, and the read reports the port gone
            return Ok(1);
        }
        let now = Instant::now();
        Ok(link
            .incoming
            .iter()
            .take_while(|(at, _)| *at <= now)
            .map(|(_, chunk)| chunk.len() as u32)
            .sum())
    }
    fn bytes_to_write(&self) -> serialport::Result<u32> {
        Ok(0)
    }
    fn clear(&self, _: ClearBuffer) -> serialport::Result<()> {
        Ok(())
    }
    fn try_clone(&self) -> serialport::Result<Box<dyn serialport::SerialPort>> {
        unsupported()
    }
    fn set_break(&self) -> serialport::Result<()> {
        unsupported()
    }
    fn clear_break(&self) -> serialport::Result<()> {
        unsupported()
    }
}
