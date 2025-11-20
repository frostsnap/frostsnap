use crate::common::{CameraError, CameraSink, DeviceInfo, Frame, FrameFormat, Resolution, Result};
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

                let mut chosen_format = None;
                for f in formats {
                    if let Some(frame_format) = fourcc_to_frame_format(f.fourcc) {
                        if frame_format == FrameFormat::MJPEG {
                            chosen_format = Some((f.fourcc, frame_format));
                            break;
                        } else if chosen_format.is_none() {
                            chosen_format = Some((f.fourcc, frame_format));
                        }
                    }
                }

                let (fourcc, frame_format) = chosen_format?;

                let framesizes = dev.enum_framesizes(fourcc).ok()?;
                let (width, height) = framesizes
                    .iter()
                    .filter_map(|fs| match &fs.size {
                        v4l::framesize::FrameSizeEnum::Discrete(size) => {
                            Some((size.width, size.height))
                        }
                        v4l::framesize::FrameSizeEnum::Stepwise(stepwise) => {
                            Some((stepwise.max_width, stepwise.max_height))
                        }
                    })
                    .max_by_key(|(w, h)| w * h)?;

                Some(DeviceInfo {
                    id: format!("/dev/video{}", index),
                    index,
                    name: caps.card,
                    description: format!("{} ({})", caps.driver, caps.bus),
                    format: frame_format,
                    resolution: Resolution::new(width, height),
                })
            })
            .collect())
    }

    pub fn open(device_info: &DeviceInfo, sink: impl CameraSink) -> Result<Self> {
        let mut device = Device::new(device_info.index).map_err(|e| {
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
                match MmapStream::with_buffers(&mut device, v4l::buffer::Type::VideoCapture, 4) {
                    Ok(s) => s,
                    Err(_) => return,
                };

            loop {
                match stream.next() {
                    Ok((buf, _meta)) => {
                        let jpeg_data =
                            match crate::decode::raw_to_jpeg(&buf, format, width, height) {
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
                    Err(_) => break,
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
