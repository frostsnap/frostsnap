use crate::frb_generated::StreamSink;
use anyhow::{anyhow, Result};
use tracing::info;

#[cfg(any(target_os = "linux", target_os = "windows"))]
use nokhwa::utils::{ApiBackend, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};
#[cfg(any(target_os = "linux", target_os = "windows"))]
use nokhwa::Camera;

#[derive(Clone, Debug)]
pub struct CameraDevice {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
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
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let cameras = nokhwa::query(ApiBackend::Auto)
                .map_err(|e| anyhow!("Failed to query cameras: {}", e))?;

            let mut devices = Vec::new();

            for info in cameras {
                let index = match info.index().as_index() {
                    Ok(idx) => idx,
                    Err(_) => continue,
                };
                let name = info.human_name().to_string();

                // Probe camera to verify MJPEG support and get resolution
                let camera_index = CameraIndex::Index(index);
                let requested = RequestedFormat::with_formats(
                    RequestedFormatType::AbsoluteHighestResolution,
                    &[FrameFormat::MJPEG],
                );

                match Camera::new(camera_index, requested) {
                    Ok(camera) => {
                        let format = camera.camera_format();

                        if format.format() != FrameFormat::MJPEG {
                            info!(
                                "Skipping camera {} ({}): does not support MJPEG (got {:?})",
                                index,
                                name,
                                format.format()
                            );
                            continue;
                        }

                        let resolution = format.resolution();
                        devices.push(CameraDevice {
                            index,
                            name,
                            width: resolution.width(),
                            height: resolution.height(),
                        });
                    }
                    Err(e) => {
                        info!("Skipping camera {} ({}): {}", index, name, e);
                    }
                }
            }

            // Give nokhwa time to clean up all dropped cameras from its global device list
            // Yes. Things do crash without this due to some erroneous `assert!` within nokhwa.
            std::thread::sleep(std::time::Duration::from_millis(100));

            Ok(devices)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(anyhow!(
                "Camera support only available on Linux and Windows"
            ))
        }
    }

    pub fn start(&self, sink: StreamSink<CameraEvent>) -> Result<()> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            // Re-query cameras to verify our device is still at the expected index
            let cameras = nokhwa::query(ApiBackend::Auto)
                .map_err(|e| anyhow!("Failed to query cameras: {}", e))?;

            let camera_info = cameras
                .into_iter()
                .find(|info| info.index().as_index().ok() == Some(self.index))
                .ok_or_else(|| anyhow!("Camera at index {} not found", self.index))?;

            // Verify the camera name matches what we expect
            if camera_info.human_name() != self.name {
                return Err(anyhow!(
                    "Camera at index {} changed: expected '{}', found '{}'",
                    self.index,
                    self.name,
                    camera_info.human_name()
                ));
            }

            // Open camera with MJPEG format
            let requested = RequestedFormat::with_formats(
                RequestedFormatType::AbsoluteHighestResolution,
                &[FrameFormat::MJPEG],
            );

            let mut camera = Camera::new(CameraIndex::Index(self.index), requested)
                .map_err(|e| anyhow!("Failed to open camera '{}': {}", self.name, e))?;

            camera
                .open_stream()
                .map_err(|e| anyhow!("Failed to open camera stream: {}", e))?;

            let resolution = camera.resolution();
            let format_str = format!("{:?}", camera.camera_format().format());

            info!(
                "Camera '{}' (index {}) using format: {}x{} {}",
                self.name,
                self.index,
                resolution.width(),
                resolution.height(),
                format_str
            );

            loop {
                let frame = camera
                    .frame()
                    .map_err(|e| anyhow!("Failed to capture frame: {}", e))?;

                // Get raw buffer - for MJPEG this should be JPEG bytes
                let buffer = frame.buffer_bytes();

                let camera_frame = CameraFrame {
                    data: buffer.to_vec(),
                    width: resolution.width(),
                    height: resolution.height(),
                    format: format_str.clone(),
                };

                if sink.add(CameraEvent::Frame(camera_frame)).is_err() {
                    info!("Camera '{}' stream closed, stopping capture", self.name);
                    break;
                }
            }

            Ok(())
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            sink.add(CameraEvent::Error(
                "Camera support only available on Linux and Windows".to_string(),
            ))
            .ok();
            Err(anyhow!(
                "Camera support only available on Linux and Windows"
            ))
        }
    }
}
