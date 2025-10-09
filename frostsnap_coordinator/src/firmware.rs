use frostsnap_comms::{FirmwareCapabilities, Sha256Digest, FIRMWARE_UPGRADE_CHUNK_LEN};

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
    UnknownSignedFirmware { digest: Sha256Digest },
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
            FirmwareValidationError::UnknownSignedFirmware { digest } => {
                write!(
                    f,
                    "Unknown signed firmware with digest: {}. Signed firmware must be in KNOWN_FIRMWARE_VERSIONS",
                    digest
                )
            }
        }
    }
}

impl std::error::Error for FirmwareValidationError {}

#[derive(Clone, Copy)]
pub struct ValidatedFirmwareBin {
    firmware: FirmwareBin,
    digest_with_signature: Sha256Digest,
    firmware_only_digest: Sha256Digest,
    firmware_size: u32,
    total_size: u32,
    version: Option<VersionNumber>,
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
        let digest_with_signature = Sha256Digest(digest_state.finalize().into());

        let mut firmware_only_state = sha2::Sha256::default();
        firmware_only_state.update(&firmware.as_bytes()[..firmware_size as usize]);
        let firmware_only_digest = Sha256Digest(firmware_only_state.finalize().into());

        let is_signed = firmware_size < total_size;

        let version = if is_signed {
            // Signed firmware MUST be in KNOWN_FIRMWARE_VERSIONS
            VersionNumber::from_digest(&digest_with_signature).ok_or(
                FirmwareValidationError::UnknownSignedFirmware {
                    digest: digest_with_signature,
                },
            )?;
            VersionNumber::from_digest(&digest_with_signature)
        } else {
            // Unsigned firmware might be a dev build, so it's optional
            VersionNumber::from_digest(&firmware_only_digest)
        };

        Ok(Self {
            firmware,
            digest_with_signature,
            firmware_only_digest,
            firmware_size,
            total_size,
            version,
        })
    }

    pub fn digest(&self) -> Sha256Digest {
        self.firmware_only_digest
    }

    pub fn digest_with_signature(&self) -> Sha256Digest {
        self.digest_with_signature
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

    pub fn is_signed(&self) -> bool {
        self.firmware_size < self.total_size
    }

    pub fn version(&self) -> Option<VersionNumber> {
        self.version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionNumber {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl std::fmt::Display for VersionNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl VersionNumber {
    pub const fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn from_digest(digest: &Sha256Digest) -> Option<Self> {
        KNOWN_FIRMWARE_VERSIONS
            .iter()
            .find(|(d, _)| d == digest)
            .map(|(_, v)| *v)
    }

    pub fn capabilities(&self) -> FirmwareCapabilities {
        const V0_0_1: VersionNumber = VersionNumber::new(0, 0, 1);

        FirmwareCapabilities {
            upgrade_digest_no_sig: *self > V0_0_1,
        }
    }
}

// Known firmware versions indexed by their digest
//
// NOTE: v0.0.1 has both signed and unsigned entries because we didn't have a proper
// digest-to-version system yet. Devices announced their full digest (including signature),
// so signed and unsigned builds had different digests. Starting with future versions, we
// should only track the firmware-only (deterministic) digest.
pub const KNOWN_FIRMWARE_VERSIONS: &[(Sha256Digest, VersionNumber)] = &[
    /*signed*/
    (
        Sha256Digest([
            0x57, 0x16, 0x1f, 0x80, 0xb4, 0x14, 0x13, 0xb1, 0x05, 0x3e, 0x27, 0x2f, 0x9c, 0x3d,
            0xa8, 0xd1, 0x6e, 0xcf, 0xce, 0x44, 0x79, 0x33, 0x45, 0xbe, 0x69, 0xf7, 0xfe, 0x03,
            0xd9, 0x3f, 0x4e, 0xb0,
        ]),
        VersionNumber::new(0, 0, 1),
    ),
    /*unsigned*/
    (
        Sha256Digest([
            0x8f, 0x45, 0xae, 0x6b, 0x72, 0xc2, 0x41, 0xa2, 0x07, 0x98, 0xac, 0xbd, 0x3c, 0x6d,
            0x3e, 0x54, 0x07, 0x1c, 0xae, 0x73, 0xe3, 0x35, 0xdf, 0x17, 0x85, 0xf2, 0xd4, 0x85,
            0xa9, 0x15, 0xda, 0x4c,
        ]),
        VersionNumber::new(0, 0, 1),
    ),
];
