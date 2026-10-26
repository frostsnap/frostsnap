use frostsnap_comms::{
    BINCODE_CONFIG, DeviceSendBody, Downstream, ReceiveSerial, Sha256Digest, Upstream,
};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_simulator(args: &[&str]) -> (ChildGuard, String) {
    let bin = env!("CARGO_BIN_EXE_device_simulator");
    let mut child = Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn device_simulator");

    let stdout = child.stdout.take().expect("capture child stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let slave_path = loop {
        line.clear();
        let n = reader.read_line(&mut line).expect("read line from child");
        if n == 0 {
            panic!("child stdout closed before printing SIM PORT");
        }
        let trimmed = line.trim();
        if let Some(path) = trimmed.strip_prefix("SIM PORT ") {
            let path = path.trim().to_string();
            eprintln!("Got slave path: '{path}'");
            break path;
        }
    };

    (ChildGuard(child), slave_path)
}

fn read_until<T>(
    port: &mut Box<dyn serialport::SerialPort>,
    recv_buf: &mut Vec<u8>,
    timeout: Duration,
    mut decode_fn: impl FnMut(&mut Vec<u8>) -> Option<T>,
) -> T {
    let start = Instant::now();
    let mut buf = [0u8; 512];
    loop {
        if let Some(val) = decode_fn(recv_buf) {
            return val;
        }
        if start.elapsed() > timeout {
            panic!("timed out waiting for expected message (buffer: {recv_buf:02x?})");
        }
        match port.read(&mut buf) {
            Ok(n) if n > 0 => {
                recv_buf.extend_from_slice(&buf[..n]);
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("error reading serial port: {e}"),
        }
    }
}

#[test]
fn test_headless_handshake_with_name() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let state_file = temp_dir.path().join("device.state");

    let (_guard, slave_path) = spawn_simulator(&[
        "--state",
        state_file.to_str().unwrap(),
        "--name",
        "SimDevice1",
        "--headless",
    ]);

    let mut port = serialport::new(&slave_path, 0)
        .timeout(Duration::from_secs(3))
        .open()
        .expect("open serial port to simulator slave");

    let magic = ReceiveSerial::<Upstream>::MagicBytes(frostsnap_comms::MagicBytes::default());
    let magic_bytes = bincode::encode_to_vec(magic, BINCODE_CONFIG).unwrap();
    port.write_all(&magic_bytes).expect("write magic bytes");
    port.flush().expect("flush magic bytes");

    let mut recv_buf = Vec::new();

    let _version = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
        frostsnap_comms::find_and_remove_magic_bytes::<Downstream>(buf)
    });

    let (from1, body1) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
        match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
            Ok((ReceiveSerial::Message(msg), consumed)) => {
                buf.drain(..consumed);
                Some((from1_id(msg.from), msg.body.decode().expect("valid body")))
            }
            _ => None,
        }
    });

    assert!(matches!(
        body1,
        DeviceSendBody::Announce { firmware_digest } if firmware_digest == Sha256Digest([0u8; 32])
    ));

    let (from2, body2) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
        match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
            Ok((ReceiveSerial::Message(msg), consumed)) => {
                buf.drain(..consumed);
                Some((msg.from, msg.body.decode().expect("valid body")))
            }
            _ => None,
        }
    });

    assert_eq!(from1, from2);
    match body2 {
        DeviceSendBody::SetName { name } => {
            assert_eq!(name.as_str(), "SimDevice1");
        }
        other => panic!("expected SetName, got {other:?}"),
    }
}

fn from1_id(id: frostsnap_core::DeviceId) -> frostsnap_core::DeviceId {
    id
}

#[test]
fn test_headless_handshake_without_name() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let state_file = temp_dir.path().join("device.state");

    let (_guard, slave_path) =
        spawn_simulator(&["--state", state_file.to_str().unwrap(), "--headless"]);

    let mut port = serialport::new(&slave_path, 0)
        .timeout(Duration::from_secs(3))
        .open()
        .expect("open serial port to simulator slave");

    let magic = ReceiveSerial::<Upstream>::MagicBytes(frostsnap_comms::MagicBytes::default());
    let magic_bytes = bincode::encode_to_vec(magic, BINCODE_CONFIG).unwrap();
    port.write_all(&magic_bytes).expect("write magic bytes");
    port.flush().expect("flush magic bytes");

    let mut recv_buf = Vec::new();

    let _version = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
        frostsnap_comms::find_and_remove_magic_bytes::<Downstream>(buf)
    });

    let (_from1, body1) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
        match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
            Ok((ReceiveSerial::Message(msg), consumed)) => {
                buf.drain(..consumed);
                Some((msg.from, msg.body.decode().expect("valid body")))
            }
            _ => None,
        }
    });

    assert!(matches!(
        body1,
        DeviceSendBody::Announce { firmware_digest } if firmware_digest == Sha256Digest([0u8; 32])
    ));

    let (_from2, body2) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
        match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
            Ok((ReceiveSerial::Message(msg), consumed)) => {
                buf.drain(..consumed);
                Some((msg.from, msg.body.decode().expect("valid body")))
            }
            _ => None,
        }
    });

    assert!(matches!(body2, DeviceSendBody::NeedName));
}

#[test]
fn test_state_persistence_across_restarts() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let state_file = temp_dir.path().join("device.state");

    let first_device_id = {
        let (_guard, slave_path) = spawn_simulator(&[
            "--state",
            state_file.to_str().unwrap(),
            "--name",
            "PersistentSim",
            "--headless",
        ]);

        let mut port = serialport::new(&slave_path, 0)
            .timeout(Duration::from_secs(3))
            .open()
            .expect("open serial port to simulator slave");

        let magic = ReceiveSerial::<Upstream>::MagicBytes(frostsnap_comms::MagicBytes::default());
        let magic_bytes = bincode::encode_to_vec(magic, BINCODE_CONFIG).unwrap();
        port.write_all(&magic_bytes).expect("write magic bytes");
        port.flush().expect("flush magic bytes");

        let mut recv_buf = Vec::new();

        let _version = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
            frostsnap_comms::find_and_remove_magic_bytes::<Downstream>(buf)
        });

        let (from1, body1) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
            match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
                Ok((ReceiveSerial::Message(msg), consumed)) => {
                    buf.drain(..consumed);
                    Some((msg.from, msg.body.decode().expect("valid body")))
                }
                _ => None,
            }
        });

        assert!(matches!(
            body1,
            DeviceSendBody::Announce { firmware_digest } if firmware_digest == Sha256Digest([0u8; 32])
        ));

        let (from2, body2) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
            match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
                Ok((ReceiveSerial::Message(msg), consumed)) => {
                    buf.drain(..consumed);
                    Some((msg.from, msg.body.decode().expect("valid body")))
                }
                _ => None,
            }
        });

        assert_eq!(from1, from2);
        match body2 {
            DeviceSendBody::SetName { name } => {
                assert_eq!(name.as_str(), "PersistentSim");
            }
            other => panic!("expected SetName, got {other:?}"),
        }

        from1
    };

    // Restart without --name flag, using the same state file
    {
        let (_guard, slave_path) =
            spawn_simulator(&["--state", state_file.to_str().unwrap(), "--headless"]);

        let mut port = serialport::new(&slave_path, 0)
            .timeout(Duration::from_secs(3))
            .open()
            .expect("open serial port to simulator slave");

        let magic = ReceiveSerial::<Upstream>::MagicBytes(frostsnap_comms::MagicBytes::default());
        let magic_bytes = bincode::encode_to_vec(magic, BINCODE_CONFIG).unwrap();
        port.write_all(&magic_bytes).expect("write magic bytes");
        port.flush().expect("flush magic bytes");

        let mut recv_buf = Vec::new();

        let _version = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
            frostsnap_comms::find_and_remove_magic_bytes::<Downstream>(buf)
        });

        let (from1, body1) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
            match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
                Ok((ReceiveSerial::Message(msg), consumed)) => {
                    buf.drain(..consumed);
                    Some((msg.from, msg.body.decode().expect("valid body")))
                }
                _ => None,
            }
        });

        assert_eq!(from1, first_device_id);
        assert!(matches!(
            body1,
            DeviceSendBody::Announce { firmware_digest } if firmware_digest == Sha256Digest([0u8; 32])
        ));

        let (from2, body2) = read_until(&mut port, &mut recv_buf, Duration::from_secs(5), |buf| {
            match bincode::decode_from_slice::<ReceiveSerial<Downstream>, _>(buf, BINCODE_CONFIG) {
                Ok((ReceiveSerial::Message(msg), consumed)) => {
                    buf.drain(..consumed);
                    Some((msg.from, msg.body.decode().expect("valid body")))
                }
                _ => None,
            }
        });

        assert_eq!(from2, first_device_id);
        match body2 {
            DeviceSendBody::SetName { name } => {
                assert_eq!(name.as_str(), "PersistentSim");
            }
            other => panic!("expected SetName, got {other:?}"),
        }
    }
}

#[test]
fn test_desktop_serial_open_sim_port() {
    use frostsnap_coordinator::Serial;

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let state_file = temp_dir.path().join("device.state");

    let (_guard, slave_path) = spawn_simulator(&[
        "--state",
        state_file.to_str().unwrap(),
        "--name",
        "DesktopSerialSim",
        "--headless",
    ]);

    let desktop_serial = frostsnap_coordinator::DesktopSerial;
    let port = desktop_serial
        .open_device_port(&slave_path, frostsnap_comms::BAUDRATE)
        .expect("DesktopSerial should open simulated port");
    drop(port);
}
