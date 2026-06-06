use chrono::Local;
use frostsnap_coordinator::{DesktopSerial, Serial};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const ESP32C3_USB_JTAG_VID: u16 = 0x303a;
const ESP32C3_USB_JTAG_PID: u16 = 0x1001;
const BAUD: &str = "921600";
const BOOTLOADER_OFFSET: usize = 0x0;
const POLL_INTERVAL: Duration = Duration::from_millis(200);

pub fn run(
    concurrency: usize,
    env_name: &str,
    output: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    if concurrency == 0 {
        return Err("concurrency must be greater than zero".into());
    }

    let image = FlashImage::from_env(env_name, output)?;
    let image_size = image.write_merged()?;

    println!("Prepared merged flash image:");
    println!("  {}", image.output.display());
    println!(
        "  bootloader: 0x{:x} {}",
        BOOTLOADER_OFFSET,
        image.bootloader.display()
    );
    println!(
        "  partitions: 0x{:x} {}",
        image.partition_offset,
        image.partitions.display()
    );
    println!(
        "  firmware:   0x{:x} {}",
        image.app_offset,
        image.firmware.display()
    );
    println!();
    println!(
        "Waiting for ESP32-C3 USB Serial/JTAG devices ({:04x}:{:04x}); flashing up to {concurrency} at a time...",
        ESP32C3_USB_JTAG_VID, ESP32C3_USB_JTAG_PID
    );

    let desktop_serial = DesktopSerial;
    let (tx, rx) = mpsc::channel();
    let mut workers: HashMap<String, DeviceStatus> = HashMap::new();
    let mut waiting_for_disconnect: HashSet<String> = HashSet::new();
    let mut completed = 0usize;
    let mut next_slot = 1usize;
    let started_at = Local::now().format("%b %-e %-I.%M%P").to_string();
    let started = Instant::now();

    draw(&workers, completed, concurrency, &started_at, started)?;

    loop {
        let present: HashSet<String> = desktop_serial
            .available_ports()
            .into_iter()
            .filter(|port| port.vid == ESP32C3_USB_JTAG_VID && port.pid == ESP32C3_USB_JTAG_PID)
            .map(|port| port.id)
            .collect();

        workers.retain(|port, status| {
            matches!(status.state, WorkerState::Flashing) || present.contains(port)
        });
        waiting_for_disconnect.retain(|port| present.contains(port));

        for port in present {
            if active_count(&workers) >= concurrency {
                break;
            }
            if workers.contains_key(&port) || waiting_for_disconnect.contains(&port) {
                continue;
            }

            let slot = next_slot;
            next_slot += 1;
            workers.insert(
                port.clone(),
                DeviceStatus {
                    slot,
                    port: port.clone(),
                    state: WorkerState::Flashing,
                    last_line: "starting espflash".to_string(),
                },
            );
            spawn_espflash(port, image.output.clone(), image_size, tx.clone());
        }

        while let Ok(event) = rx.try_recv() {
            match event {
                WorkerEvent::Line { port, line } => {
                    if let Some(status) = workers.get_mut(&port) {
                        status.last_line = trim_status_line(&line);
                    }
                }
                WorkerEvent::Finished {
                    port,
                    success,
                    summary,
                } => {
                    if let Some(status) = workers.get_mut(&port) {
                        status.last_line = summary;
                        if success {
                            status.state = WorkerState::Complete;
                            completed += 1;
                        } else {
                            status.state = WorkerState::Failed;
                            waiting_for_disconnect.insert(port);
                        }
                    }
                }
            }
        }

        draw(&workers, completed, concurrency, &started_at, started)?;
        thread::sleep(POLL_INTERVAL);
    }
}

fn active_count(workers: &HashMap<String, DeviceStatus>) -> usize {
    workers
        .values()
        .filter(|status| matches!(status.state, WorkerState::Flashing))
        .count()
}

fn spawn_espflash(port: String, image: PathBuf, image_size: usize, tx: mpsc::Sender<WorkerEvent>) {
    thread::spawn(move || {
        let mut child = match Command::new("espflash")
            .arg("write-bin")
            .arg("--chip")
            .arg("esp32c3")
            .arg("--baud")
            .arg(BAUD)
            .arg("--no-stub")
            .arg("--port")
            .arg(&port)
            .arg("0x0")
            .arg(&image)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                let _ = tx.send(WorkerEvent::Finished {
                    port,
                    success: false,
                    summary: format!("failed to start espflash: {error}"),
                });
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            spawn_reader(port.clone(), stdout, tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(port.clone(), stderr, tx.clone());
        }

        match child.wait() {
            Ok(status) if status.success() => {
                let _ = tx.send(WorkerEvent::Finished {
                    port,
                    success: true,
                    summary: format!("flash complete ({image_size} bytes)"),
                });
            }
            Ok(status) => {
                let _ = tx.send(WorkerEvent::Finished {
                    port,
                    success: false,
                    summary: format!("espflash exited with {status} - unplug/replug to retry"),
                });
            }
            Err(error) => {
                let _ = tx.send(WorkerEvent::Finished {
                    port,
                    success: false,
                    summary: format!("failed waiting for espflash: {error}"),
                });
            }
        }
    });
}

fn spawn_reader(
    port: String,
    mut stream: impl Read + Send + 'static,
    tx: mpsc::Sender<WorkerEvent>,
) {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) if byte[0] == b'\n' || byte[0] == b'\r' => {
                    if !buf.is_empty() {
                        let line = String::from_utf8_lossy(&buf).trim().to_string();
                        let _ = tx.send(WorkerEvent::Line {
                            port: port.clone(),
                            line,
                        });
                        buf.clear();
                    }
                }
                Ok(_) => buf.push(byte[0]),
                Err(_) => break,
            }
        }
        if !buf.is_empty() {
            let line = String::from_utf8_lossy(&buf).trim().to_string();
            let _ = tx.send(WorkerEvent::Line { port, line });
        }
    });
}

fn draw(
    workers: &HashMap<String, DeviceStatus>,
    completed: usize,
    concurrency: usize,
    started_at: &str,
    started: Instant,
) -> io::Result<()> {
    print!("\x1b[2J\x1b[H");
    println!("Frostsnap batch flash");
    println!(
        "Started: {started_at} | elapsed: {} | complete: {completed} | active: {} | concurrency: {concurrency}",
        format_elapsed(started.elapsed()),
        active_count(workers),
    );
    println!();

    let mut statuses: Vec<_> = workers.values().collect();
    statuses.sort_by_key(|status| status.slot);

    if statuses.is_empty() {
        println!("No devices flashing yet. Plug in blank Frostsnap devices.");
    } else {
        for status in statuses {
            println!(
                "Device {:>2}  {:<18}  {:<9}  {}",
                status.slot,
                display_port(&status.port),
                status.state.label(),
                status.last_line
            );
        }
    }
    io::stdout().flush()
}

fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn display_port(port: &str) -> String {
    Path::new(port)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(port)
        .to_string()
}

fn trim_status_line(line: &str) -> String {
    let line = line.trim();
    if line.len() <= 100 {
        line.to_string()
    } else {
        format!("...{}", &line[line.len() - 97..])
    }
}

#[derive(Debug)]
struct FlashImage {
    bootloader: PathBuf,
    partitions: PathBuf,
    firmware: PathBuf,
    output: PathBuf,
    partition_offset: usize,
    app_offset: usize,
}

impl FlashImage {
    fn from_env(
        env_name: &str,
        output: Option<PathBuf>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let bootloader = PathBuf::from(format!(
            "frostsnap_factory/bootloader/{env_name}/signed-bootloader.bin"
        ));
        let partitions = PathBuf::from("device/partitions.bin");
        let firmware = PathBuf::from(format!(
            "target/riscv32imc-unknown-none-elf/release/{env_name}-frontier.bin"
        ));
        let output = output.unwrap_or_else(|| {
            PathBuf::from(format!(
                "target/riscv32imc-unknown-none-elf/release/{env_name}-factory-flash.bin"
            ))
        });

        for path in [&bootloader, &partitions, &firmware] {
            if !path.is_file() {
                return Err(format!("missing required artifact: {}", path.display()).into());
            }
        }

        Ok(Self {
            bootloader,
            partitions,
            firmware,
            output,
            partition_offset: read_partition_offset()?,
            app_offset: read_app_offset()?,
        })
    }

    fn write_merged(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let inputs = [
            (
                BOOTLOADER_OFFSET,
                std::fs::read(&self.bootloader)?,
                self.bootloader.as_path(),
            ),
            (
                self.partition_offset,
                std::fs::read(&self.partitions)?,
                self.partitions.as_path(),
            ),
            (
                self.app_offset,
                std::fs::read(&self.firmware)?,
                self.firmware.as_path(),
            ),
        ];

        validate_no_overlaps(&inputs)?;

        let image_len = inputs
            .iter()
            .map(|(offset, bytes, _)| offset + bytes.len())
            .max()
            .unwrap_or(0);
        let mut image = vec![0xff; image_len];
        for (offset, bytes, _) in inputs {
            image[offset..offset + bytes.len()].copy_from_slice(&bytes);
        }

        if let Some(parent) = self.output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.output, image)?;
        Ok(image_len)
    }
}

fn validate_no_overlaps(
    inputs: &[(usize, Vec<u8>, &Path)],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ranges: Vec<_> = inputs
        .iter()
        .map(|(offset, bytes, path)| (*offset, *offset + bytes.len(), *path))
        .collect();
    ranges.sort_by_key(|(start, _, _)| *start);

    for pair in ranges.windows(2) {
        let (_, prev_end, prev_path) = pair[0];
        let (next_start, _, next_path) = pair[1];
        if prev_end > next_start {
            return Err(format!(
                "{} overlaps {} in merged flash image",
                prev_path.display(),
                next_path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn read_partition_offset() -> Result<usize, Box<dyn std::error::Error>> {
    let defaults = std::fs::read_to_string("frostsnap_factory/bootloader/sdkconfig.defaults")?;
    for line in defaults.lines() {
        if let Some(value) = line.strip_prefix("CONFIG_PARTITION_TABLE_OFFSET=") {
            return parse_hex_usize(value.trim());
        }
    }
    Err("CONFIG_PARTITION_TABLE_OFFSET not found in sdkconfig.defaults".into())
}

fn read_app_offset() -> Result<usize, Box<dyn std::error::Error>> {
    let csv = std::fs::read_to_string("device/partitions.csv")?;
    for line in csv.lines() {
        let fields: Vec<_> = line.split(',').map(str::trim).collect();
        if fields.first() == Some(&"ota_0") {
            let offset = fields
                .get(3)
                .ok_or("ota_0 row is missing offset in device/partitions.csv")?;
            return parse_hex_usize(offset);
        }
    }
    Err("ota_0 offset not found in device/partitions.csv".into())
}

fn parse_hex_usize(value: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let value = value.trim().trim_matches('"');
    let value = value.strip_prefix("0x").unwrap_or(value);
    Ok(usize::from_str_radix(value, 16)?)
}

#[derive(Debug)]
struct DeviceStatus {
    slot: usize,
    port: String,
    state: WorkerState,
    last_line: String,
}

#[derive(Debug)]
enum WorkerState {
    Flashing,
    Complete,
    Failed,
}

impl WorkerState {
    fn label(&self) -> &'static str {
        match self {
            WorkerState::Flashing => "flashing",
            WorkerState::Complete => "complete",
            WorkerState::Failed => "failed",
        }
    }
}

#[derive(Debug)]
enum WorkerEvent {
    Line {
        port: String,
        line: String,
    },
    Finished {
        port: String,
        success: bool,
        summary: String,
    },
}
