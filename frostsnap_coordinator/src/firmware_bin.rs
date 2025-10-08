use frostsnap_comms::{Sha256Digest, FIRMWARE_UPGRADE_CHUNK_LEN};

#[derive(Clone, Copy)]
pub struct FirmwareBin {
    bin: &'static [u8],
}

impl frostsnap_comms::firmware_reader::FirmwareReader for FirmwareBin {
    type Error = std::io::Error;

    fn read_sector(
        &self,
        sector: u32,
    ) -> Result<Box<[u8; frostsnap_comms::firmware_reader::SECTOR_SIZE]>, Self::Error> {
        use frostsnap_comms::firmware_reader::SECTOR_SIZE;

        let sector_offset = (sector as usize) * SECTOR_SIZE;
        if sector_offset >= self.bin.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Sector out of bounds",
            ));
        }

        let mut sector_data = Box::new([0u8; SECTOR_SIZE]);
        let end = (sector_offset + SECTOR_SIZE).min(self.bin.len());
        let data_len = end - sector_offset;
        sector_data[..data_len].copy_from_slice(&self.bin[sector_offset..end]);

        Ok(sector_data)
    }

    fn n_sectors(&self) -> u32 {
        use frostsnap_comms::firmware_reader::SECTOR_SIZE;
        self.bin.len().div_ceil(SECTOR_SIZE) as u32
    }
}

impl FirmwareBin {
    pub const fn is_stub(&self) -> bool {
        self.bin.is_empty()
    }

    pub const fn new(bin: &'static [u8]) -> Self {
        Self { bin }
    }

    pub fn num_chunks(&self) -> u32 {
        (self.bin.len() as u32).div_ceil(FIRMWARE_UPGRADE_CHUNK_LEN)
    }

    pub fn size(&self) -> u32 {
        self.bin.len() as u32
    }

    pub fn as_bytes(&self) -> &'static [u8] {
        self.bin
    }

    pub fn validate(self) -> Result<ValidatedFirmwareBin, FirmwareValidationError> {
        ValidatedFirmwareBin::new(self)
    }
}

#[derive(Debug, Clone)]
pub enum FirmwareValidationError {
    InvalidFormat(frostsnap_comms::firmware_reader::FirmwareSizeError),
    SizeMismatch { expected: u32, actual: u32 },
}

impl std::fmt::Display for FirmwareValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FirmwareValidationError::InvalidFormat(err) => {
                write!(f, "Invalid firmware format: {}", err)
            }
            FirmwareValidationError::SizeMismatch { expected, actual } => {
                write!(
                    f,
                    "Firmware size mismatch: expected {} bytes, got {} bytes",
                    expected, actual
                )
            }
        }
    }
}

impl std::error::Error for FirmwareValidationError {}

#[derive(Clone, Copy)]
pub struct ValidatedFirmwareBin {
    firmware: FirmwareBin,
    digest: Sha256Digest,
    firmware_only_digest: Sha256Digest,
    firmware_size: u32,
    total_size: u32,
}

impl ValidatedFirmwareBin {
    pub fn new(firmware: FirmwareBin) -> Result<Self, FirmwareValidationError> {
        use frostsnap_core::sha2::digest::Digest;

        let (firmware_size, total_size) =
            frostsnap_comms::firmware_reader::firmware_size(&firmware)
                .map_err(FirmwareValidationError::InvalidFormat)?;

        if total_size != firmware.size() {
            return Err(FirmwareValidationError::SizeMismatch {
                expected: total_size,
                actual: firmware.size(),
            });
        }

        let mut digest_state = sha2::Sha256::default();
        digest_state.update(firmware.as_bytes());
        let digest = Sha256Digest(digest_state.finalize().into());

        let mut firmware_only_state = sha2::Sha256::default();
        firmware_only_state.update(&firmware.as_bytes()[..firmware_size as usize]);
        let firmware_only_digest = Sha256Digest(firmware_only_state.finalize().into());

        Ok(Self {
            firmware,
            digest,
            firmware_only_digest,
            firmware_size,
            total_size,
        })
    }

    pub fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub fn firmware_only_digest(&self) -> Sha256Digest {
        self.firmware_only_digest
    }

    pub fn size(&self) -> u32 {
        self.total_size
    }

    pub fn firmware_size(&self) -> u32 {
        self.firmware_size
    }

    pub fn total_size(&self) -> u32 {
        self.total_size
    }

    pub fn as_bytes(&self) -> &'static [u8] {
        self.firmware.as_bytes()
    }

    pub fn num_chunks(&self) -> u32 {
        self.firmware.num_chunks()
    }
}
