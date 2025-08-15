use crate::alloc::string::ToString;
use alloc::string::String;
use alloc::vec::Vec;

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
pub const SIGNATURE_MAGIC: [u8; 4] = [0xE7, 0x02, 0x00, 0x00];
pub const HEADER_SIZE: usize = core::mem::size_of::<ImageHeader>();
pub const SEGMENT_HEADER_SIZE: usize = core::mem::size_of::<SegmentHeader>();
pub const MAX_SEGMENTS: u8 = 16; // Reasonable limit
pub const MAX_SEGMENT_SIZE: u32 = 16 * 1024 * 1024; // 16MB limit
pub const SIGNATURE_SEARCH_SECTORS: u32 = 2; // Only search 2 sectors (8KB) for signatures

pub trait FirmwareReader {
    fn sector_size(&self) -> u32;
    fn n_sectors(&self) -> u32;
    fn size(&self) -> u32;
    fn read_sector(&self, sector: u32) -> Result<Vec<u8>, FirmwareSizeError>;
}

pub fn firmware_size<R: FirmwareReader>(reader: &R) -> Result<u32, FirmwareSizeError> {
    // Read and validate the first sector
    let first_sector = reader
        .read_sector(0)
        .map_err(|_| FirmwareSizeError::IoError("Failed to read first sector".to_string()))?;

    if first_sector.len() < HEADER_SIZE {
        return Err(FirmwareSizeError::InvalidHeaderSize);
    }

    // Safe header parsing - manual field extraction
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

    for _segment_idx in 0..header.segment_count {
        let segment_header = read_segment_header_safe(reader, &first_sector, current_pos)?;

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

    // Calculate firmware end with padding (following espflash logic)
    let mut firmware_end = (max_data_end + 15) & !15; // 16-byte alignment

    // Add digest if present (following espflash logic)
    if header.append_digest == 1 {
        firmware_end = firmware_end
            .checked_add(32)
            .ok_or(FirmwareSizeError::CorruptedSegmentHeader)?;
    }

    // Search for signatures (but with bounds checking)
    if let Some(signature_pos) = search_for_signature_safe(reader, firmware_end)? {
        return Ok(signature_pos);
    }

    Ok(firmware_end)
}

fn read_segment_header_safe<R: FirmwareReader>(
    reader: &R,
    first_sector: &[u8],
    pos: u32,
) -> Result<SegmentHeader, FirmwareSizeError> {
    let sector_size = reader.sector_size();
    let sector_num = pos / sector_size;
    let sector_offset = (pos % sector_size) as usize;

    // Check if we need the segment header from current or next sector
    if sector_offset + SEGMENT_HEADER_SIZE <= first_sector.len() && sector_num == 0 {
        // Header fits in first sector
        let end_pos = sector_offset + SEGMENT_HEADER_SIZE;
        if end_pos <= first_sector.len() {
            return Ok(parse_segment_header(&first_sector[sector_offset..end_pos]));
        }
    }

    // Header spans sectors or is in a different sector
    if sector_num >= reader.n_sectors() {
        return Err(FirmwareSizeError::SectorOutOfBounds(sector_num));
    }

    let sector = if sector_num == 0 {
        first_sector
    } else {
        &reader.read_sector(sector_num).map_err(|_| {
            FirmwareSizeError::IoError("Failed to read sector for segment header".to_string())
        })?
    };

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
            if next_sector_num >= reader.n_sectors() {
                return Err(FirmwareSizeError::SectorOutOfBounds(next_sector_num));
            }

            let next_sector = reader.read_sector(next_sector_num).map_err(|_| {
                FirmwareSizeError::IoError(
                    "Failed to read next sector for spanning segment header".to_string(),
                )
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

fn search_for_signature_safe<R: FirmwareReader>(
    reader: &R,
    firmware_end: u32,
) -> Result<Option<u32>, FirmwareSizeError> {
    let sector_size = reader.sector_size();
    let signature_search_start = firmware_end;

    // Limit search scope to prevent excessive I/O
    for i in 0..SIGNATURE_SEARCH_SECTORS {
        let sector_start = signature_search_start + (i * sector_size);
        if sector_start >= reader.size() {
            break;
        }

        let sector_num = sector_start / sector_size;
        if sector_num >= reader.n_sectors() {
            break;
        }

        let sector = reader.read_sector(sector_num).map_err(|_| {
            FirmwareSizeError::IoError("Failed to read sector during signature search".to_string())
        })?;

        // Search for signature magic in this sector with bounds checking
        if sector.len() >= SIGNATURE_MAGIC.len() {
            for pos in 0..=sector.len() - SIGNATURE_MAGIC.len() {
                if sector[pos..pos + SIGNATURE_MAGIC.len()] == SIGNATURE_MAGIC {
                    // Found signature - return position relative to partition start
                    return Ok(Some(sector_start + pos as u32));
                }
            }
        }
    }

    Ok(None)
}
