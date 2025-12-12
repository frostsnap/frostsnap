use crate::common::{
    CameraError, CameraSink, CandidateFormat, DeviceInfo, Frame, FrameFormat, Resolution, Result,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread::{self, JoinHandle};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{COINIT, CoInitializeEx, CoUninitialize};
use windows::core::GUID;

static INITIALIZED: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));
static CAMERA_REFCNT: LazyLock<Arc<AtomicUsize>> = LazyLock::new(|| Arc::new(AtomicUsize::new(0)));

const CO_INIT_APARTMENT_THREADED: COINIT = COINIT(0x2);
const CO_INIT_DISABLE_OLE1DDE: COINIT = COINIT(0x4);

const MF_VIDEO_FORMAT_YUY2: GUID = GUID::from_values(
    0x32595559,
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
);
const MF_VIDEO_FORMAT_MJPEG: GUID = GUID::from_values(
    0x47504A4D,
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
);
const MF_VIDEO_FORMAT_NV12: GUID = GUID::from_values(
    0x3231564E,
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
);
const MF_VIDEO_FORMAT_RGB24: GUID = GUID::from_values(
    0x00000014,
    0x0000,
    0x0010,
    [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
);

const MEDIA_FOUNDATION_FIRST_VIDEO_STREAM: u32 = 0xFFFFFFFC;

fn guid_to_frameformat(guid: GUID) -> Option<FrameFormat> {
    match guid {
        MF_VIDEO_FORMAT_NV12 => Some(FrameFormat::NV12),
        MF_VIDEO_FORMAT_RGB24 => Some(FrameFormat::RGB24),
        MF_VIDEO_FORMAT_YUY2 => Some(FrameFormat::YUYV422),
        MF_VIDEO_FORMAT_MJPEG => Some(FrameFormat::MJPEG),
        _ => None,
    }
}

fn frameformat_to_guid(format: FrameFormat) -> GUID {
    match format {
        FrameFormat::NV12 => MF_VIDEO_FORMAT_NV12,
        FrameFormat::RGB24 => MF_VIDEO_FORMAT_RGB24,
        FrameFormat::YUYV422 => MF_VIDEO_FORMAT_YUY2,
        FrameFormat::MJPEG => MF_VIDEO_FORMAT_MJPEG,
        FrameFormat::YUV420 => MF_VIDEO_FORMAT_NV12,
    }
}

fn initialize_mf() -> Result<()> {
    if !INITIALIZED.load(Ordering::SeqCst) {
        unsafe {
            let hr = CoInitializeEx(None, CO_INIT_APARTMENT_THREADED | CO_INIT_DISABLE_OLE1DDE);
            if hr.is_err() {
                return Err(CameraError::OpenFailed("CoInitializeEx failed".to_string()));
            }

            let hr = MFStartup(MF_API_VERSION, MFSTARTUP_NOSOCKET);
            if hr.is_err() {
                CoUninitialize();
                return Err(CameraError::OpenFailed("MFStartup failed".to_string()));
            }
        }
        INITIALIZED.store(true, Ordering::SeqCst);
    }
    CAMERA_REFCNT.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

fn de_initialize_mf() {
    if CAMERA_REFCNT.fetch_sub(1, Ordering::SeqCst) == 1 {
        if INITIALIZED.load(Ordering::SeqCst) {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
            INITIALIZED.store(false, Ordering::SeqCst);
        }
    }
}

pub struct WindowsCamera {
    _thread: JoinHandle<()>,
}

impl WindowsCamera {
    pub fn list_devices() -> Result<Vec<DeviceInfo>> {
        initialize_mf()?;

        let attributes = unsafe {
            let mut attr = None;
            MFCreateAttributes(&mut attr, 1).map_err(|e| {
                CameraError::DeviceError(format!("MFCreateAttributes failed: {}", e))
            })?;
            attr.ok_or_else(|| CameraError::DeviceError("Failed to create attributes".to_string()))?
        };

        unsafe {
            attributes
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| CameraError::DeviceError(format!("SetGUID failed: {}", e)))?;
        }

        let mut count = 0_u32;
        let mut devices_ptr: *mut Option<IMFActivate> = std::ptr::null_mut();

        unsafe {
            MFEnumDeviceSources(&attributes, &mut devices_ptr, &mut count).map_err(|e| {
                CameraError::DeviceError(format!("MFEnumDeviceSources failed: {}", e))
            })?;
        }

        if count == 0 {
            de_initialize_mf();
            return Ok(vec![]);
        }

        let devices = unsafe { std::slice::from_raw_parts(devices_ptr, count as usize) };
        let mut device_list = Vec::new();

        for (index, activate_opt) in devices.iter().enumerate() {
            if let Some(activate) = activate_opt {
                let name = unsafe {
                    let mut pwstr = windows::core::PWSTR::null();
                    let mut len = 0;
                    activate
                        .GetAllocatedString(
                            &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                            &mut pwstr,
                            &mut len,
                        )
                        .ok()
                        .and_then(|_| pwstr.to_string().ok())
                        .unwrap_or_else(|| format!("Camera {}", index))
                };

                let symlink = unsafe {
                    let mut pwstr = windows::core::PWSTR::null();
                    let mut len = 0;
                    activate
                        .GetAllocatedString(
                            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                            &mut pwstr,
                            &mut len,
                        )
                        .ok()
                        .and_then(|_| pwstr.to_string().ok())
                        .unwrap_or_else(|| format!("device_{}", index))
                };

                if let Ok((format, resolution)) = get_preferred_format(activate) {
                    device_list.push(DeviceInfo {
                        id: symlink.clone(),
                        index,
                        name,
                        description: symlink,
                        format,
                        resolution,
                    });
                }
            }
        }

        de_initialize_mf();
        Ok(device_list)
    }

    pub fn open(device_info: &DeviceInfo, sink: impl CameraSink) -> Result<Self> {
        initialize_mf()?;

        let format = device_info.format;
        let width = device_info.resolution.width;
        let height = device_info.resolution.height;
        let index = device_info.index;

        let thread = thread::spawn(move || {
            if let Err(_) = capture_thread(index, format, width, height, sink) {
                de_initialize_mf();
            }
        });

        Ok(Self { _thread: thread })
    }
}

impl Drop for WindowsCamera {
    fn drop(&mut self) {
        de_initialize_mf();
    }
}

fn get_preferred_format(activate: &IMFActivate) -> Result<(FrameFormat, Resolution)> {
    unsafe {
        let media_source: IMFMediaSource = activate
            .ActivateObject()
            .map_err(|e| CameraError::OpenFailed(format!("ActivateObject failed: {}", e)))?;

        let mut attr = None;
        MFCreateAttributes(&mut attr, 1)
            .map_err(|e| CameraError::OpenFailed(format!("MFCreateAttributes failed: {}", e)))?;

        let attr =
            attr.ok_or_else(|| CameraError::OpenFailed("Failed to create attributes".to_string()))?;

        attr.SetUINT32(&MF_READWRITE_DISABLE_CONVERTERS, 1)
            .map_err(|e| CameraError::OpenFailed(format!("SetUINT32 failed: {}", e)))?;

        let source_reader: IMFSourceReader =
            MFCreateSourceReaderFromMediaSource(&media_source, &attr).map_err(|e| {
                CameraError::OpenFailed(format!(
                    "MFCreateSourceReaderFromMediaSource failed: {}",
                    e
                ))
            })?;

        let mut candidates = Vec::new();
        let mut index = 0;

        while let Ok(media_type) =
            source_reader.GetNativeMediaType(MEDIA_FOUNDATION_FIRST_VIDEO_STREAM, index)
        {
            index += 1;

            let fourcc = media_type.GetGUID(&MF_MT_SUBTYPE).ok();
            let frame_format = fourcc.and_then(guid_to_frameformat);

            if let Ok(res_u64) = media_type.GetUINT64(&MF_MT_FRAME_SIZE) {
                let width = (res_u64 >> 32) as u32;
                let height = res_u64 as u32;

                if let Some(ff) = frame_format {
                    candidates.push(CandidateFormat::new(ff, Resolution::new(width, height)));
                }
            }
        }

        let best = CandidateFormat::select_best(candidates).ok_or_else(|| {
            CameraError::UnsupportedFormat("No supported format found".to_string())
        })?;
        Ok((best.format, best.resolution))
    }
}

fn capture_thread(
    index: usize,
    format: FrameFormat,
    width: u32,
    height: u32,
    sink: impl CameraSink,
) -> Result<()> {
    unsafe {
        let attributes = {
            let mut attr = None;
            MFCreateAttributes(&mut attr, 1).map_err(|e| {
                CameraError::DeviceError(format!("MFCreateAttributes failed: {}", e))
            })?;
            attr.ok_or_else(|| CameraError::DeviceError("Failed to create attributes".to_string()))?
        };

        attributes
            .SetGUID(
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            )
            .map_err(|e| CameraError::DeviceError(format!("SetGUID failed: {}", e)))?;

        let mut count = 0_u32;
        let mut devices_ptr: *mut Option<IMFActivate> = std::ptr::null_mut();

        MFEnumDeviceSources(&attributes, &mut devices_ptr, &mut count)
            .map_err(|e| CameraError::DeviceError(format!("MFEnumDeviceSources failed: {}", e)))?;

        if index >= count as usize {
            return Err(CameraError::DeviceNotFound(format!(
                "Device {} not found",
                index
            )));
        }

        let devices = std::slice::from_raw_parts(devices_ptr, count as usize);
        let activate = devices[index]
            .as_ref()
            .ok_or_else(|| CameraError::DeviceNotFound(format!("Device {} is null", index)))?;

        let media_source: IMFMediaSource = activate
            .ActivateObject()
            .map_err(|e| CameraError::OpenFailed(format!("ActivateObject failed: {}", e)))?;

        let mut attr = None;
        MFCreateAttributes(&mut attr, 1)
            .map_err(|e| CameraError::OpenFailed(format!("MFCreateAttributes failed: {}", e)))?;

        let attr =
            attr.ok_or_else(|| CameraError::OpenFailed("Failed to create attributes".to_string()))?;

        attr.SetUINT32(&MF_READWRITE_DISABLE_CONVERTERS, 1)
            .map_err(|e| CameraError::OpenFailed(format!("SetUINT32 failed: {}", e)))?;

        let source_reader: IMFSourceReader =
            MFCreateSourceReaderFromMediaSource(&media_source, &attr).map_err(|e| {
                CameraError::OpenFailed(format!(
                    "MFCreateSourceReaderFromMediaSource failed: {}",
                    e
                ))
            })?;

        // Find and set the matching format
        let guid = frameformat_to_guid(format);
        let mut index = 0;
        let mut found = false;

        while let Ok(media_type) =
            source_reader.GetNativeMediaType(MEDIA_FOUNDATION_FIRST_VIDEO_STREAM, index)
        {
            index += 1;

            if let (Ok(fourcc), Ok(res_u64)) = (
                media_type.GetGUID(&MF_MT_SUBTYPE),
                media_type.GetUINT64(&MF_MT_FRAME_SIZE),
            ) {
                let type_width = (res_u64 >> 32) as u32;
                let type_height = res_u64 as u32;

                if fourcc == guid && type_width == width && type_height == height {
                    source_reader
                        .SetCurrentMediaType(MEDIA_FOUNDATION_FIRST_VIDEO_STREAM, None, &media_type)
                        .map_err(|e| {
                            CameraError::OpenFailed(format!("SetCurrentMediaType failed: {}", e))
                        })?;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            return Err(CameraError::UnsupportedFormat(
                "Failed to set format".to_string(),
            ));
        }

        source_reader
            .SetStreamSelection(MEDIA_FOUNDATION_FIRST_VIDEO_STREAM, true)
            .map_err(|e| CameraError::CaptureFailed(format!("SetStreamSelection failed: {}", e)))?;

        loop {
            let mut sample_opt = None;
            let mut stream_flags = 0;

            loop {
                source_reader
                    .ReadSample(
                        MEDIA_FOUNDATION_FIRST_VIDEO_STREAM,
                        0,
                        None,
                        Some(&mut stream_flags),
                        None,
                        Some(&mut sample_opt),
                    )
                    .map_err(|e| CameraError::CaptureFailed(format!("ReadSample failed: {}", e)))?;

                if sample_opt.is_some() {
                    break;
                }
            }

            let sample =
                sample_opt.ok_or_else(|| CameraError::CaptureFailed("No sample".to_string()))?;

            let buffer = sample.ConvertToContiguousBuffer().map_err(|e| {
                CameraError::CaptureFailed(format!("ConvertToContiguousBuffer failed: {}", e))
            })?;

            let mut buffer_ptr = std::ptr::null_mut::<u8>();
            let mut buffer_len = 0_u32;

            buffer
                .Lock(&mut buffer_ptr, None, Some(&mut buffer_len))
                .map_err(|e| CameraError::CaptureFailed(format!("Lock failed: {}", e)))?;

            if !buffer_ptr.is_null() && buffer_len > 0 {
                let data = std::slice::from_raw_parts(buffer_ptr, buffer_len as usize).to_vec();

                let jpeg_data = match crate::decode::raw_to_jpeg(&data, format, width, height) {
                    Ok(data) => data,
                    Err(_) => {
                        let _ = buffer.Unlock();
                        continue;
                    }
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

            let _ = buffer.Unlock();
        }
    }

    de_initialize_mf();
    Ok(())
}
