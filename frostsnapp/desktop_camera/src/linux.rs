use crate::common::{
    CameraError, CameraSink, CandidateFormat, DeviceInfo, Frame, FrameFormat, Resolution, Result,
};
use std::thread::{self, JoinHandle};
use v4l::Device;
use v4l::context::enum_devices;
use v4l::io::traits::CaptureStream as V4lCaptureStream;
use v4l::prelude::MmapStream;
use v4l::video::Capture;

pub struct LinuxCamera {
    _thread: JoinHandle<()>,
}

impl LinuxCamera {
    pub fn list_devices() -> Result<Vec<DeviceInfo>> {
        Ok(enum_devices()
            .into_iter()
            .filter_map(|node| {
                let index = node.index();
                let dev = Device::new(index).ok()?;
                let caps = dev.query_caps().ok()?;

                let formats = dev.enum_formats().ok()?;

                let candidates: Vec<_> = formats
                    .into_iter()
                    .filter_map(|f| fourcc_to_frame_format(f.fourcc).map(|fmt| (f.fourcc, fmt)))
                    .flat_map(|(fourcc, frame_format)| {
                        dev.enum_framesizes(fourcc)
                            .into_iter()
                            .flatten()
                            .map(move |fs| {
                                let (width, height) = match &fs.size {
                                    v4l::framesize::FrameSizeEnum::Discrete(size) => {
                                        (size.width, size.height)
                                    }
                                    v4l::framesize::FrameSizeEnum::Stepwise(stepwise) => {
                                        (stepwise.max_width, stepwise.max_height)
                                    }
                                };
                                CandidateFormat::new(frame_format, Resolution::new(width, height))
                            })
                    })
                    .collect();

                let best = CandidateFormat::select_best(candidates)?;

                Some(DeviceInfo {
                    id: format!("/dev/video{}", index),
                    index,
                    name: caps.card,
                    description: format!("{} ({})", caps.driver, caps.bus),
                    format: best.format,
                    resolution: best.resolution,
                })
            })
            .collect())
    }

    pub fn open(device_info: &DeviceInfo, sink: impl CameraSink) -> Result<Self> {
        let device = Device::new(device_info.index).map_err(|e| {
            CameraError::OpenFailed(format!(
                "Failed to open device {}: {}",
                device_info.index, e
            ))
        })?;

        let mut fmt = device
            .format()
            .map_err(|e| CameraError::OpenFailed(format!("Failed to get format: {}", e)))?;

        fmt.fourcc = frame_format_to_fourcc(device_info.format);
        fmt.width = device_info.resolution.width;
        fmt.height = device_info.resolution.height;

        let actual_fmt = device
            .set_format(&fmt)
            .map_err(|e| CameraError::OpenFailed(format!("Failed to set format: {}", e)))?;

        let format = device_info.format;
        let width = actual_fmt.width;
        let height = actual_fmt.height;

        let thread = thread::spawn(move || {
            let mut stream =
                match MmapStream::with_buffers(&device, v4l::buffer::Type::VideoCapture, 4) {
                    Ok(s) => s,
                    Err(_) => return,
                };

            while let Ok((buf, _meta)) = stream.next() {
                let jpeg_data = match crate::decode::raw_to_jpeg(buf, format, width, height) {
                    Ok(data) => data,
                    Err(_) => continue,
                };

                let frame = Frame {
                    data: jpeg_data,
                    width,
                    height,
                };

                if sink.send_frame(frame).is_err() {
                    break;
                }
            }
        });

        Ok(Self { _thread: thread })
    }
}

fn fourcc_to_frame_format(fourcc: v4l::FourCC) -> Option<FrameFormat> {
    let bytes = fourcc.repr;
    match &bytes {
        b"YUYV" => Some(FrameFormat::YUYV422),
        b"MJPG" => Some(FrameFormat::MJPEG),
        b"NV12" => Some(FrameFormat::NV12),
        b"YU12" => Some(FrameFormat::YUV420),
        b"RGB3" => Some(FrameFormat::RGB24),
        _ => None,
    }
}

fn frame_format_to_fourcc(format: FrameFormat) -> v4l::FourCC {
    let bytes = match format {
        FrameFormat::YUYV422 => *b"YUYV",
        FrameFormat::MJPEG => *b"MJPG",
        FrameFormat::NV12 => *b"NV12",
        FrameFormat::YUV420 => *b"YU12",
        FrameFormat::RGB24 => *b"RGB3",
    };
    v4l::FourCC { repr: bytes }
}
