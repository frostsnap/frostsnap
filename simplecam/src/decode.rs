use crate::common::{CameraError, FrameFormat, Result};

pub fn raw_to_jpeg(data: &[u8], format: FrameFormat, width: u32, height: u32) -> Result<Vec<u8>> {
    match format {
        FrameFormat::MJPEG => Ok(data.to_vec()),
        FrameFormat::YUYV422 => {
            let rgb = yuyv_to_rgb(data);
            encode_rgb_to_jpeg(&rgb, width, height)
        }
        FrameFormat::RGB24 => encode_rgb_to_jpeg(data, width, height),
        _ => Err(CameraError::UnsupportedFormat(format!(
            "JPEG encoding not implemented for {:?}",
            format
        ))),
    }
}

fn encode_rgb_to_jpeg(rgb_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    use image::ColorType;
    use image::codecs::jpeg::JpegEncoder;

    let mut jpeg_data = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, 85);

    encoder
        .encode(rgb_data, width, height, ColorType::Rgb8)
        .map_err(|e| CameraError::DecodeError(format!("JPEG encode failed: {:?}", e)))?;

    Ok(jpeg_data)
}

fn yuyv_to_rgb(yuyv_data: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity((yuyv_data.len() / 2) * 3);

    for chunk in yuyv_data.chunks_exact(4) {
        let y0 = chunk[0] as i32;
        let u = chunk[1] as i32 - 128;
        let y1 = chunk[2] as i32;
        let v = chunk[3] as i32 - 128;

        for &y in &[y0, y1] {
            let r = (y + ((359 * v) >> 8)).clamp(0, 255) as u8;
            let g = (y - ((88 * u + 183 * v) >> 8)).clamp(0, 255) as u8;
            let b = (y + ((454 * u) >> 8)).clamp(0, 255) as u8;

            rgb.push(r);
            rgb.push(g);
            rgb.push(b);
        }
    }

    rgb
}
