//! # Firmware Reading and Parsing
//!
//! This module provides a trait-based abstraction for reading firmware from different sources
//! (flash partitions on devices, or in-memory buffers on coordinators) and parsing ESP32
//! firmware image format.

use crate::SIGNATURE_BLOCK_MAGIC;
use alloc::boxed::Box;
use alloc::string::String;

pub const SECTOR_SIZE: usize = 4096;

/// Trait for reading firmware data sector-by-sector
pub trait FirmwareReader {
    type Error: core::fmt::Debug;

    fn read_sector(&self, sector: u32) -> Result<Box<[u8; SECTOR_SIZE]>, Self::Error>;
    fn n_sectors(&self) -> u32;
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct ImageHeader {
    magic: u8,
    segment_count: u8,
    flash_mode: u8,
    flash_config: u8,
    entry: u32,
    // extended header part
    wp_pin: u8,
    clk_q_drv: u8,
    d_cs_drv: u8,
    gd_wp_drv: u8,
    chip_id: u16,
    min_rev: u8,
    min_chip_rev_full: u16,
    max_chip_rev_full: u16,
    reserved: [u8; 4],
    append_digest: u8,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct SegmentHeader {
    addr: u32,
    length: u32,
}

#[derive(Debug)]
pub enum FirmwareSizeError {
    IoError(String),
    InvalidMagic(u8),
    InvalidHeaderSize,
    InvalidSegmentCount(u8),
    SegmentTooLarge(u32),
    SectorOutOfBounds(u32),
    CorruptedSegmentHeader,
}

impl core::fmt::Display for FirmwareSizeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FirmwareSizeError::IoError(msg) => write!(f, "I/O error: {}", msg),
            FirmwareSizeError::InvalidMagic(magic) => write!(
                f,
                "Invalid firmware header magic: 0x{:02X}, expected 0xE9",
                magic
            ),
            FirmwareSizeError::InvalidHeaderSize => write!(f, "Firmware header too small"),
            FirmwareSizeError::InvalidSegmentCount(count) => {
                write!(f, "Invalid segment count: {}", count)
            }
            FirmwareSizeError::SegmentTooLarge(size) => {
                write!(f, "Segment size too large: {} bytes", size)
            }
            FirmwareSizeError::SectorOutOfBounds(sector) => {
                write!(f, "Sector {} is out of bounds", sector)
            }
            FirmwareSizeError::CorruptedSegmentHeader => write!(f, "Corrupted segment header"),
        }
    }
}

// Constants from espflash
pub const ESP_MAGIC: u8 = 0xE9;
pub const HEADER_SIZE: usize = core::mem::size_of::<ImageHeader>();
pub const SEGMENT_HEADER_SIZE: usize = core::mem::size_of::<SegmentHeader>();
pub const MAX_SEGMENTS: u8 = 16;
pub const MAX_SEGMENT_SIZE: u32 = 16 * 1024 * 1024; // 16MB

/// Find the signature sector in firmware
pub fn find_signature_sector<R: FirmwareReader>(reader: &R) -> Option<u32> {
    for i in (0..reader.n_sectors()).rev() {
        match reader.read_sector(i) {
            Ok(sector_data) => {
                if sector_data.len() >= 4 && sector_data[0..4] == SIGNATURE_BLOCK_MAGIC {
                    return Some(i);
                }
            }
            Err(_) => continue,
        }
    }
    None
}

/// Calculate the actual size of ESP32 firmware.
///
/// Returns a tuple:
/// - First value: Size of firmware content only (header + segments + padding + digest)
/// - Second value: Total size including Secure Boot v2 signature blocks if present
pub fn firmware_size<R: FirmwareReader>(reader: &R) -> Result<(u32, u32), FirmwareSizeError> {
    let first_sector_array = reader
        .read_sector(0)
        .map_err(|e| FirmwareSizeError::IoError(format!("Failed to read first sector: {:?}", e)))?;
    let first_sector = &first_sector_array[..];

    if first_sector.len() < HEADER_SIZE {
        return Err(FirmwareSizeError::InvalidHeaderSize);
    }

    let header = ImageHeader {
        magic: first_sector[0],
        segment_count: first_sector[1],
        flash_mode: first_sector[2],
        flash_config: first_sector[3],
        entry: u32::from_le_bytes([
            first_sector[4],
            first_sector[5],
            first_sector[6],
            first_sector[7],
        ]),
        wp_pin: first_sector[8],
        clk_q_drv: first_sector[9],
        d_cs_drv: first_sector[10],
        gd_wp_drv: first_sector[11],
        chip_id: u16::from_le_bytes([first_sector[12], first_sector[13]]),
        min_rev: first_sector[14],
        min_chip_rev_full: u16::from_le_bytes([first_sector[15], first_sector[16]]),
        max_chip_rev_full: u16::from_le_bytes([first_sector[17], first_sector[18]]),
        reserved: [
            first_sector[19],
            first_sector[20],
            first_sector[21],
            first_sector[22],
        ],
        append_digest: first_sector[23],
    };

    if header.magic != ESP_MAGIC {
        return Err(FirmwareSizeError::InvalidMagic(header.magic));
    }

    if header.segment_count == 0 || header.segment_count > MAX_SEGMENTS {
        return Err(FirmwareSizeError::InvalidSegmentCount(header.segment_count));
    }

    let mut current_pos = HEADER_SIZE as u32;
    let mut max_data_end = current_pos;

    for _segment_idx in 0..header.segment_count {
        let segment_header = read_segment_header_safe(reader, current_pos)?;

        if segment_header.length > MAX_SEGMENT_SIZE {
            return Err(FirmwareSizeError::SegmentTooLarge(segment_header.length));
        }

        let segment_data_end = current_pos
            .checked_add(SEGMENT_HEADER_SIZE as u32)
            .and_then(|pos| pos.checked_add(segment_header.length))
            .ok_or(FirmwareSizeError::CorruptedSegmentHeader)?;

        max_data_end = max_data_end.max(segment_data_end);
        current_pos = segment_data_end;
    }

    let unpadded_length = max_data_end;
    let length_with_checksum = unpadded_length + 1;
    let padded_length = (length_with_checksum + 15) & !15;

    let mut firmware_end = padded_length;

    if header.append_digest == 1 {
        firmware_end = firmware_end
            .checked_add(32)
            .ok_or(FirmwareSizeError::CorruptedSegmentHeader)?;
    }

    if let Some(signature_sector) = find_signature_sector(reader) {
        let total_size = (signature_sector + 1) * (SECTOR_SIZE as u32);
        Ok((firmware_end, total_size))
    } else {
        Ok((firmware_end, firmware_end))
    }
}

fn read_segment_header_safe<R: FirmwareReader>(
    reader: &R,
    pos: u32,
) -> Result<SegmentHeader, FirmwareSizeError> {
    let sector_size = SECTOR_SIZE as u32;
    let sector_num = pos / sector_size;
    let sector_offset = (pos % sector_size) as usize;

    if sector_num >= reader.n_sectors() {
        return Err(FirmwareSizeError::SectorOutOfBounds(sector_num));
    }

    let sector = reader.read_sector(sector_num).map_err(|e| {
        FirmwareSizeError::IoError(format!("Failed to read sector for segment header: {:?}", e))
    })?;

    if sector_offset + SEGMENT_HEADER_SIZE <= sector.len() {
        let end_pos = sector_offset + SEGMENT_HEADER_SIZE;
        Ok(parse_segment_header(&sector[sector_offset..end_pos]))
    } else {
        let mut header_bytes = [0u8; SEGMENT_HEADER_SIZE];
        let first_part = sector.len().saturating_sub(sector_offset);

        if first_part > 0 && sector_offset < sector.len() {
            header_bytes[..first_part].copy_from_slice(&sector[sector_offset..]);
        }

        if first_part < SEGMENT_HEADER_SIZE {
            let next_sector_num = sector_num + 1;
            if next_sector_num >= reader.n_sectors() {
                return Err(FirmwareSizeError::SectorOutOfBounds(next_sector_num));
            }

            let next_sector = reader.read_sector(next_sector_num).map_err(|e| {
                FirmwareSizeError::IoError(format!(
                    "Failed to read next sector for spanning segment header: {:?}",
                    e
                ))
            })?;

            let remaining = SEGMENT_HEADER_SIZE - first_part;
            if remaining <= next_sector.len() {
                header_bytes[first_part..].copy_from_slice(&next_sector[..remaining]);
            } else {
                return Err(FirmwareSizeError::CorruptedSegmentHeader);
            }
        }

        Ok(parse_segment_header(&header_bytes))
    }
}

fn parse_segment_header(bytes: &[u8]) -> SegmentHeader {
    SegmentHeader {
        addr: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        length: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
    }
}
