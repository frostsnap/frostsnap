use std::fmt;

pub const PREFERRED_MAX_WIDTH: u32 = 1920;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub index: usize,
    pub name: String,
    pub description: String,
    pub format: FrameFormat,
    pub resolution: Resolution,
}

impl fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}): {}", self.name, self.id, self.description)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameFormat {
    YUYV422,
    NV12,
    YUV420,
    MJPEG,
    RGB24,
}

impl fmt::Display for FrameFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameFormat::YUYV422 => write!(f, "YUYV422"),
            FrameFormat::NV12 => write!(f, "NV12"),
            FrameFormat::YUV420 => write!(f, "YUV420"),
            FrameFormat::MJPEG => write!(f, "MJPEG"),
            FrameFormat::RGB24 => write!(f, "RGB24"),
        }
    }
}

pub struct Frame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceChange {
    Added(DeviceInfo),
    Removed(String),
}

#[derive(Debug)]
pub enum CameraError {
    DeviceNotFound(String),
    OpenFailed(String),
    CaptureFailed(String),
    UnsupportedFormat(String),
    InvalidParameter(String),
    DecodeError(String),
    IoError(std::io::Error),
    DeviceError(String),
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CameraError::DeviceNotFound(msg) => write!(f, "Device not found: {}", msg),
            CameraError::OpenFailed(msg) => write!(f, "Failed to open device: {}", msg),
            CameraError::CaptureFailed(msg) => write!(f, "Failed to start capture: {}", msg),
            CameraError::UnsupportedFormat(msg) => write!(f, "Format not supported: {}", msg),
            CameraError::InvalidParameter(msg) => write!(f, "Invalid parameter: {}", msg),
            CameraError::DecodeError(msg) => write!(f, "Decode error: {}", msg),
            CameraError::IoError(e) => write!(f, "IO error: {}", e),
            CameraError::DeviceError(msg) => write!(f, "Device error: {}", msg),
        }
    }
}

impl std::error::Error for CameraError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CameraError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CameraError {
    fn from(e: std::io::Error) -> Self {
        CameraError::IoError(e)
    }
}

pub type Result<T> = std::result::Result<T, CameraError>;

pub trait CameraSink: Send + 'static {
    fn send_frame(&self, frame: Frame) -> Result<()>;
}
