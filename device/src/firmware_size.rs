//! # ESP32 Firmware Size Calculation
//!
//! This module implements an algorithm to determine the actual size of ESP32 firmware
//! stored in a flash partition. This is necessary because ESP32 firmware partitions
//! are fixed-size containers filled with 0xFF padding, but we need to know where the
//! actual firmware content ends.
//!
//! ## Background
//!
//! When firmware is written to an ESP32 partition:
//! - The partition has a fixed size (e.g., 1.25MB)
//! - The actual firmware might be much smaller (e.g., 500KB)
//! - The remaining space is filled with 0xFF bytes (erased flash state)
//! - The device has no filesystem to track the actual firmware size
//!
//! When Secure Boot v2 is enabled, firmware includes additional signature blocks:
//! - Signature blocks are 4KB sectors containing RSA signatures and public keys
//! - They are identified by magic bytes: `0xE7, 0x02, 0x00, 0x00`
//! - The algorithm scans all sectors to locate signature blocks rather than
//!   trying to predict complex MMU page alignment schemes
//!
//! ## Algorithm
//!
//! The algorithm mimics what `espflash` does to calculate firmware size:
//!
//! 1. **Read the image header** (24 bytes) containing magic byte 0xE9 and segment count
//! 2. **Parse all segments** sequentially to find where the last one ends
//! 3. **Apply 16-byte alignment** as required by the ESP32 bootloader
//! 4. **Add 32 bytes if a SHA256 digest is appended** (`append_digest == 1`)
//! 5. **Scan for Secure Boot v2 signature blocks** and include their size if present
//!
//! ## Why This Is Needed
//!
//! This allows us to:
//! - Calculate the same SHA256 hash that external tools like `sha256sum` would produce
//! - Verify firmware integrity by comparing device-calculated and externally-calculated hashes
//! - Support deterministic/reproducible builds where the same source produces identical binaries
//! - Most importantly, implement secure boot.
//!
//! ## Future Improvements
//!
//! ESP-HAL v1.0 will provide APIs for parsing ESP32 image formats, but we cannot use
//! them yet as v1.0 hasn't been properly released. Once a stable release is available
//! and we upgrade, we should consider using the official parsing utilities instead of
//! this implementation.

use crate::partitions::EspFlashPartition;
use alloc::string::String;
use frostsnap_embedded::{FlashPartition, SECTOR_SIZE};

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ImageHeader {
    pub magic: u8,
    pub segment_count: u8,
    pub flash_mode: u8,
    pub flash_config: u8,
    pub entry: u32,
    // extended header part
    pub wp_pin: u8,
    pub clk_q_drv: u8,
    pub d_cs_drv: u8,
    pub gd_wp_drv: u8,
    pub chip_id: u16,
    pub min_rev: u8,
    pub min_chip_rev_full: u16,
    pub max_chip_rev_full: u16,
    pub reserved: [u8; 4],
    pub append_digest: u8,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct SegmentHeader {
    pub addr: u32,
    pub length: u32,
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
pub const MAX_SEGMENTS: u8 = 16; // Reasonable limit
pub const MAX_SEGMENT_SIZE: u32 = 16 * 1024 * 1024; // 16MB limit

/// Calculate the actual size of ESP32 firmware in a partition.
///
/// This function parses the ESP32 firmware image format to determine where the
/// actual firmware content ends within the partition.
///
/// # Arguments
///
/// * `partition` - The flash partition containing the firmware
///
/// # Returns
///
/// A tuple containing:
/// - First value: Size of firmware content only (header + segments + padding + digest)
/// - Second value: Total size including Secure Boot v2 signature blocks if present
///
/// The first value is suitable for:
/// - Calculating SHA256 hashes for secure boot
/// - Determining firmware content size
///
/// The second value is suitable for:
/// - Determining total bytes written to flash partition
/// - Calculating SHA256 hashes to present to user and compare against deterministic builds
/// - OTA update size validation
///
/// # Errors
///
/// Returns an error if:
/// - The partition cannot be read
/// - The firmware header is invalid or corrupted
/// - Segment headers are malformed
pub fn firmware_size(partition: &EspFlashPartition) -> Result<(u32, u32), FirmwareSizeError> {
    // Read and validate the first sector
    let first_sector_array = FlashPartition::read_sector(partition, 0)
        .map_err(|e| FirmwareSizeError::IoError(format!("Failed to read first sector: {:?}", e)))?;
    let first_sector = &first_sector_array[..];

    if first_sector.len() < HEADER_SIZE {
        return Err(FirmwareSizeError::InvalidHeaderSize);
    }

    // Safe header parsing - manual field extraction
    //
    // TODO: Use esp-hal v1.0 has a lib to do this for us so we leave this spaghetti here for now.
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

    // Validate magic number
    if header.magic != ESP_MAGIC {
        return Err(FirmwareSizeError::InvalidMagic(header.magic));
    }

    // Validate segment count
    if header.segment_count == 0 || header.segment_count > MAX_SEGMENTS {
        return Err(FirmwareSizeError::InvalidSegmentCount(header.segment_count));
    }

    // Process segments safely
    let mut current_pos = HEADER_SIZE as u32;
    let mut max_data_end = current_pos;

    // read all the segments to find where the last one ends.
    for _segment_idx in 0..header.segment_count {
        let segment_header = read_segment_header_safe(partition, current_pos)?;

        // Validate segment length
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

    // Calculate firmware end with padding (following ESP-IDF bootloader logic)
    // The bootloader's process_checksum function handles padding after all segments

    // First: segments are already processed, max_data_end is the end of all segment data
    let unpadded_length = max_data_end;

    // Add space for checksum byte
    let length_with_checksum = unpadded_length + 1;

    // Pad to next full 16 byte block (matching bootloader's logic)
    let padded_length = (length_with_checksum + 15) & !15;

    let mut firmware_end = padded_length;

    // Add digest if present (following espflash logic)
    if header.append_digest == 1 {
        firmware_end = firmware_end
            .checked_add(32)
            .ok_or(FirmwareSizeError::CorruptedSegmentHeader)?;
    }

    // Look for Secure Boot v2 signature block by scanning sectors
    if let Some((signature_sector, _signature_block)) =
        crate::secure_boot::find_signature_sector(partition)
    {
        // Found signature block, firmware ends at end of signature sector
        let total_size = (signature_sector + 1) * (SECTOR_SIZE as u32);
        Ok((firmware_end, total_size))
    } else {
        Ok((firmware_end, firmware_end))
    }
}

fn read_segment_header_safe(
    partition: &EspFlashPartition,
    pos: u32,
) -> Result<SegmentHeader, FirmwareSizeError> {
    let sector_size = SECTOR_SIZE as u32;
    let sector_num = pos / sector_size;
    let sector_offset = (pos % sector_size) as usize;

    // Check bounds
    if sector_num >= partition.n_sectors() {
        return Err(FirmwareSizeError::SectorOutOfBounds(sector_num));
    }

    // Read the sector containing the header
    let sector = FlashPartition::read_sector(partition, sector_num).map_err(|e| {
        FirmwareSizeError::IoError(format!("Failed to read sector for segment header: {:?}", e))
    })?;

    if sector_offset + SEGMENT_HEADER_SIZE <= sector.len() {
        // Header fits in current sector
        let end_pos = sector_offset + SEGMENT_HEADER_SIZE;
        Ok(parse_segment_header(&sector[sector_offset..end_pos]))
    } else {
        // Header spans sectors - reconstruct safely
        let mut header_bytes = [0u8; SEGMENT_HEADER_SIZE];
        let first_part = sector.len().saturating_sub(sector_offset);

        if first_part > 0 && sector_offset < sector.len() {
            header_bytes[..first_part].copy_from_slice(&sector[sector_offset..]);
        }

        if first_part < SEGMENT_HEADER_SIZE {
            let next_sector_num = sector_num + 1;
            if next_sector_num >= partition.n_sectors() {
                return Err(FirmwareSizeError::SectorOutOfBounds(next_sector_num));
            }

            let next_sector =
                FlashPartition::read_sector(partition, next_sector_num).map_err(|e| {
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
