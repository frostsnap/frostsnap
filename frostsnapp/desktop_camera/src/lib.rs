pub mod common;
pub mod decode;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxCamera as Camera;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsCamera as Camera;

pub use common::{
    CameraError, CameraSink, DeviceChange, DeviceInfo, Frame, FrameFormat, Resolution, Result,
};
