//! Talks to a device in a remote hardware lab through the lab's serial bridge, and prints
//! what the device announces. A starting point: add the flow you want to test where the
//! comment in the loop says.
//!
//!     cargo run -p frostsnap_coordinator --features remote-serial --example remote_device -- \
//!         'wss://<lab>/api/serial?control=<token>' [--verbose]
//!
//! `--verbose` also logs every protocol message.

use frostsnap_coordinator::{remote_serial::RemoteSerial, DeviceChange, UsbSerialManager};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: remote_device <wss bridge url> [--verbose]"))?;
    let verbose = args.any(|a| a == "--verbose");
    tracing_subscriber::fmt()
        .with_max_level(if verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .init();

    let serial = RemoteSerial::connect(&url, Duration::ZERO)?;
    let mut manager = UsbSerialManager::new(Box::new(serial));
    let started = Instant::now();
    println!("connected to the bridge; waiting for the device to announce itself");
    loop {
        for change in manager.poll_ports() {
            let t = started.elapsed().as_secs_f32();
            match change {
                DeviceChange::Connected { id, .. } => println!("{t:6.1}s connected: {id:?}"),
                DeviceChange::NeedsName { id } => println!("{t:6.1}s {id:?} has no name yet"),
                DeviceChange::Registered { id, name } => {
                    println!("{t:6.1}s {id:?} is ready, named {name:?}");
                    // Start the flow you are testing here, e.g.
                    // manager.usb_sender().send(CoordinatorSendMessage { .. })
                }
                DeviceChange::Disconnected { id } => println!("{t:6.1}s disconnected: {id:?}"),
                other => println!("{t:6.1}s {other:?}"),
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
