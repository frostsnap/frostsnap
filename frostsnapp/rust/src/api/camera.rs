use crate::frb_generated::StreamSink;
use anyhow::{anyhow, Result};
use tracing::info;

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use eye::hal::traits::{Context, Device, Stream};
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
use eye::hal::PlatformContext;

#[derive(Clone, Debug)]
pub struct CameraDevice {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub uri: String, // eye-rs uses URI to identify devices
}

#[derive(Clone, Debug)]
pub struct CameraFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

#[derive(Clone, Debug)]
pub enum CameraEvent {
    Frame(CameraFrame),
    QrDetected(String),
    Error(String),
}

impl CameraDevice {
    pub fn list() -> Result<Vec<CameraDevice>> {
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        {
            let ctx = PlatformContext::default();
            let devices = ctx
                .devices()
                .map_err(|e| anyhow!("Failed to query cameras: {}", e))?;

            let camera_devices: Vec<CameraDevice> = devices
                .into_iter()
                .enumerate()
                .map(|(index, dev)| {
                    let name = if dev.product.is_empty() {
                        format!("Camera {}", index)
                    } else {
                        dev.product.clone()
                    };
                    info!("Found camera {} ({}) at {}", index, name, dev.uri);

                    CameraDevice {
                        index: index as u32,
                        name,
                        uri: dev.uri.clone(),
                        width: 1280, // Default, will be determined when stream starts
                        height: 720,
                    }
                })
                .collect();

            info!(
                "Camera enumeration complete, found {} devices",
                camera_devices.len()
            );

            Ok(camera_devices)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Err(anyhow!(
                "Camera support only available on Linux, Windows, and macOS"
            ))
        }
    }

    pub fn start(&self, sink: StreamSink<CameraEvent>) -> Result<()> {
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        {
            let ctx = PlatformContext::default();

            // Open the device using the URI
            let dev = ctx
                .open_device(&self.uri)
                .map_err(|e| anyhow!("Failed to open camera '{}': {}", self.name, e))?;

            // Get available streams
            let streams = dev
                .streams()
                .map_err(|e| anyhow!("Failed to get camera streams: {}", e))?;

            if streams.is_empty() {
                return Err(anyhow!("Camera '{}' has no available streams", self.name));
            }

            // Try to find an MJPEG stream, otherwise use the first available stream
            let stream_desc = streams
                .iter()
                .find(|s| {
                    // Check if this stream supports MJPEG
                    s.pixfmt.to_string().to_lowercase().contains("mjpeg")
                        || s.pixfmt.to_string().to_lowercase().contains("jpeg")
                })
                .or_else(|| streams.first())
                .ok_or_else(|| anyhow!("No suitable stream found"))?
                .clone();

            info!(
                "Camera '{}' (index {}) using format: {}x{} {:?}",
                self.name, self.index, stream_desc.width, stream_desc.height, stream_desc.pixfmt
            );

            // Start the stream
            let mut stream = dev
                .start_stream(&stream_desc)
                .map_err(|e| anyhow!("Failed to start camera stream: {}", e))?;

            loop {
                match stream.next() {
                    Some(Ok(frame)) => {
                        let camera_frame = CameraFrame {
                            data: frame.to_vec(),
                            width: stream_desc.width,
                            height: stream_desc.height,
                            format: format!("{:?}", stream_desc.pixfmt),
                        };

                        if sink.add(CameraEvent::Frame(camera_frame)).is_err() {
                            info!("Camera '{}' stream closed, stopping capture", self.name);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let error_msg = format!("Failed to capture frame: {}", e);
                        sink.add(CameraEvent::Error(error_msg.clone())).ok();
                        return Err(anyhow!(error_msg));
                    }
                    None => {
                        info!("Camera '{}' stream ended", self.name);
                        break;
                    }
                }
            }

            Ok(())
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            sink.add(CameraEvent::Error(
                "Camera support only available on Linux, Windows, and macOS".to_string(),
            ))
            .ok();
            Err(anyhow!(
                "Camera support only available on Linux, Windows, and macOS"
            ))
        }
    }
}
