use crate::common::{CameraError, FrameFormat, Result};

pub fn raw_to_jpeg(data: &[u8], format: FrameFormat, width: u32, height: u32) -> Result<Vec<u8>> {
    match format {
        FrameFormat::MJPEG => Ok(data.to_vec()),
        FrameFormat::YUYV422 => {
            let rgb = yuyv_to_rgb(data, width, height);
            encode_rgb_to_jpeg(&rgb, width, height)
        }
        FrameFormat::NV12 => {
            let rgb = nv12_to_rgb(data, width, height);
            encode_rgb_to_jpeg(&rgb, width, height)
        }
        FrameFormat::YUV420 => {
            let rgb = yuv420_to_rgb(data, width, height);
            encode_rgb_to_jpeg(&rgb, width, height)
        }
        FrameFormat::RGB24 => encode_rgb_to_jpeg(data, width, height),
    }
}

fn encode_rgb_to_jpeg(rgb_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    use image::ExtendedColorType;
    use image::codecs::jpeg::JpegEncoder;

    let mut jpeg_data = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_data, 85);

    encoder
        .encode(rgb_data, width, height, ExtendedColorType::Rgb8)
        .map_err(|e| CameraError::DecodeError(format!("JPEG encode failed: {:?}", e)))?;

    Ok(jpeg_data)
}

fn yuyv_to_rgb(yuyv_data: &[u8], _width: u32, _height: u32) -> Vec<u8> {
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

fn nv12_to_rgb(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let width = width as usize;
    let height = height as usize;
    let y_plane_size = width * height;
    let mut rgb = Vec::with_capacity(width * height * 3);

    let y_plane = &data[..y_plane_size];
    let uv_plane = &data[y_plane_size..];

    for row in 0..height {
        for col in 0..width {
            let y = y_plane[row * width + col] as i32;

            let uv_row = row / 2;
            let uv_col = (col / 2) * 2;
            let uv_index = uv_row * width + uv_col;

            let u = uv_plane.get(uv_index).copied().unwrap_or(128) as i32 - 128;
            let v = uv_plane.get(uv_index + 1).copied().unwrap_or(128) as i32 - 128;

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

fn yuv420_to_rgb(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let width = width as usize;
    let height = height as usize;
    let y_plane_size = width * height;
    let u_plane_size = (width / 2) * (height / 2);
    let mut rgb = Vec::with_capacity(width * height * 3);

    let y_plane = &data[..y_plane_size];
    let u_plane = &data[y_plane_size..y_plane_size + u_plane_size];
    let v_plane = &data[y_plane_size + u_plane_size..];

    for row in 0..height {
        for col in 0..width {
            let y = y_plane[row * width + col] as i32;

            let uv_row = row / 2;
            let uv_col = col / 2;
            let uv_index = uv_row * (width / 2) + uv_col;

            let u = u_plane.get(uv_index).copied().unwrap_or(128) as i32 - 128;
            let v = v_plane.get(uv_index).copied().unwrap_or(128) as i32 - 128;

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
