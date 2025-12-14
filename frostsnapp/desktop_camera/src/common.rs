use std::fmt;

/// Preferred pixel count for MJPEG (hardware decoded, can handle higher res)
pub const MJPEG_PREFERRED_PIXELS: u32 = 1920 * 1080;

/// Preferred pixel count for raw formats like NV12/YUYV (CPU decoded, prefer lower res)
pub const RAW_PREFERRED_PIXELS: u32 = 1280 * 720;

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

impl Resolution {
    pub fn pixels(&self) -> u32 {
        self.width * self.height
    }

    /// Distance from the preferred pixel count for this format
    pub fn distance_from_preferred(&self, format: FrameFormat) -> u32 {
        let preferred = if format == FrameFormat::MJPEG {
            MJPEG_PREFERRED_PIXELS
        } else {
            RAW_PREFERRED_PIXELS
        };
        self.pixels().abs_diff(preferred)
    }
}

/// A camera format/resolution candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateFormat {
    pub format: FrameFormat,
    pub resolution: Resolution,
}

impl CandidateFormat {
    pub fn new(format: FrameFormat, resolution: Resolution) -> Self {
        Self { format, resolution }
    }

    /// Select the best candidate from a list.
    /// Preference order: (1) MJPEG over raw, (2) closest to preferred pixel count
    pub fn select_best(candidates: impl IntoIterator<Item = Self>) -> Option<Self> {
        candidates.into_iter().max_by_key(|c| {
            (
                c.format == FrameFormat::MJPEG,
                std::cmp::Reverse(c.resolution.distance_from_preferred(c.format)),
            )
        })
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
