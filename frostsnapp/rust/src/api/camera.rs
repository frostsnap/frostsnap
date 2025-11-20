use crate::frb_generated::StreamSink;
use anyhow::{anyhow, Result};
use tracing::info;

#[cfg(any(target_os = "linux", target_os = "windows"))]
use frostsnap_desktop_camera::Camera;

#[derive(Clone, Debug)]
pub struct CameraDevice {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl CameraDevice {
    pub fn list() -> Result<Vec<CameraDevice>> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let devices =
                Camera::list_devices().map_err(|e| anyhow!("Failed to query cameras: {}", e))?;

            Ok(devices
                .into_iter()
                .map(|device| CameraDevice {
                    index: device.index as u32,
                    name: device.name,
                    width: device.resolution.width,
                    height: device.resolution.height,
                })
                .collect())
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(anyhow!(
                "Camera support only available on Linux and Windows"
            ))
        }
    }

    pub fn start(&self, sink: StreamSink<Frame>) -> Result<()> {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        {
            let devices =
                Camera::list_devices().map_err(|e| anyhow!("Failed to query cameras: {}", e))?;

            let device_info = devices
                .into_iter()
                .find(|d| d.index == self.index as usize && d.name == self.name)
                .ok_or_else(|| {
                    anyhow!("Camera '{}' at index {} not found", self.name, self.index)
                })?;

            info!(
                "Opening camera '{}' (index {}) with format: {}x{} {}",
                device_info.name,
                device_info.index,
                device_info.resolution.width,
                device_info.resolution.height,
                device_info.format
            );

            let name = device_info.name.clone();
            let camera_sink = CameraSinkAdapter { sink, name };

            let _camera = Camera::open(&device_info, camera_sink)
                .map_err(|e| anyhow!("Failed to open camera '{}': {}", self.name, e))?;

            Ok(())
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Err(anyhow!(
                "Camera support only available on Linux and Windows"
            ))
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
struct CameraSinkAdapter {
    sink: StreamSink<Frame>,
    name: String,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl frostsnap_desktop_camera::CameraSink for CameraSinkAdapter {
    fn send_frame(
        &self,
        camera_frame: frostsnap_desktop_camera::Frame,
    ) -> frostsnap_desktop_camera::Result<()> {
        let frame = Frame {
            data: camera_frame.data,
            width: camera_frame.width,
            height: camera_frame.height,
        };

        self.sink.add(frame).map_err(|_| {
            info!("Camera '{}' stream closed, stopping capture", self.name);
            frostsnap_desktop_camera::CameraError::CaptureFailed("Stream closed".to_string())
        })
    }
}
