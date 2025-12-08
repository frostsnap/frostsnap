use bincode::{Decode, Encode};
use frostsnap_comms::{Sha256Digest, FIRMWARE_UPGRADE_CHUNK_LEN};

#[derive(Encode, Decode, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FirmwareFeatures {
    /// Device supports firmware digest verification without signature block
    pub upgrade_digest_no_sig: bool,
}

impl FirmwareFeatures {
    pub const fn all() -> Self {
        Self {
            upgrade_digest_no_sig: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareVersion {
    pub digest: Sha256Digest,
    pub version: Option<VersionNumber>,
}

impl FirmwareVersion {
    pub fn new(digest: Sha256Digest) -> Self {
        Self {
            digest,
            version: VersionNumber::from_digest(&digest),
        }
    }

    pub fn features(&self) -> FirmwareFeatures {
        match self.version {
            Some(version) => version.features(),
            None => FirmwareFeatures::all(),
        }
    }

    pub fn version_name(&self) -> String {
        match self.version {
            Some(version) => format!("v{}", version),
            None => {
                let short_hash = frostsnap_core::hex::encode(&self.digest.0[..3]);
                format!("dev-{}", short_hash)
            }
        }
    }

    pub fn check_upgrade_eligibility(
        &self,
        device_digest: &Sha256Digest,
    ) -> FirmwareUpgradeEligibility {
        if *device_digest == self.digest {
            return FirmwareUpgradeEligibility::UpToDate;
        }

        let device_version = VersionNumber::from_digest(device_digest);
        let app_version = self.version;

        match (device_version, app_version) {
            (None, None) => FirmwareUpgradeEligibility::CanUpgrade,
            (None, Some(_)) => FirmwareUpgradeEligibility::CannotUpgrade {
                reason:
                    "Device firmware version newer app. Cannot downgrade firmware. Upgrade the app."
                        .to_string(),
            },
            (Some(_), None) => FirmwareUpgradeEligibility::CannotUpgrade {
                reason: "This is a development app. Cannot upgrade proper device.".to_string(),
            },
            (Some(device_ver), Some(app_ver)) => {
                if app_ver > device_ver {
                    FirmwareUpgradeEligibility::CanUpgrade
                } else if app_ver == device_ver {
                    FirmwareUpgradeEligibility::UpToDate
                } else {
                    FirmwareUpgradeEligibility::CannotUpgrade {
                        reason: "Device firmware version newer app. Cannot downgrade firmware. Upgrade the app.".to_string(),
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareUpgradeEligibility {
    UpToDate,
    CanUpgrade,
    CannotUpgrade { reason: String },
}

#[derive(Clone, Copy)]
pub struct FirmwareBin {
    bin: &'static [u8],
}

impl std::fmt::Debug for FirmwareBin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirmwareBin")
            .field("size", &self.bin.len())
            .finish()
    }
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

#[derive(Clone, Copy, Debug)]
pub struct ValidatedFirmwareBin {
    firmware: FirmwareBin,
    digest_with_signature: Sha256Digest,
    firmware_version: FirmwareVersion,
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
        let digest_with_signature = Sha256Digest(digest_state.finalize().into());

        let mut firmware_only_state = sha2::Sha256::default();
        firmware_only_state.update(&firmware.as_bytes()[..firmware_size as usize]);
        let firmware_only_digest = Sha256Digest(firmware_only_state.finalize().into());

        let is_signed = firmware_size < total_size;

        if is_signed {
            // Signed firmware MUST be in KNOWN_FIRMWARE_VERSIONS
            VersionNumber::from_digest(&firmware_only_digest).ok_or(
                FirmwareValidationError::UnknownSignedFirmware {
                    digest: digest_with_signature,
                },
            )?;
        }

        let firmware_version = FirmwareVersion::new(firmware_only_digest);

        Ok(Self {
            firmware,
            digest_with_signature,
            firmware_version,
            firmware_size,
            total_size,
        })
    }

    pub fn firmware_version(&self) -> FirmwareVersion {
        self.firmware_version
    }

    pub fn digest(&self) -> Sha256Digest {
        self.firmware_version.digest
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
        self.firmware_version.version
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

    pub fn features(&self) -> FirmwareFeatures {
        const V0_0_1: VersionNumber = VersionNumber::new(0, 0, 1);

        FirmwareFeatures {
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
use frostsnap_macros::hex;
pub const KNOWN_FIRMWARE_VERSIONS: &[(Sha256Digest, VersionNumber)] = &[
    (
        Sha256Digest(hex!(
            "e432d313c7698b1e8843b10ba95efa8c28e66a5723b966c56c156687d09d16e0"
        )),
        VersionNumber::new(0, 2, 0),
    ),
    (
        Sha256Digest(hex!(
            "5ff7bd280b96d645b2c739a6b91bfc6c27197e213302770f1a7180678ca4f720"
        )),
        VersionNumber::new(0, 1, 0),
    ),
    /*signed*/
    (
        Sha256Digest(hex!(
            "57161f80b41413b1053e272f9c3da8d16ecfce44793345be69f7fe03d93f4eb0"
        )),
        VersionNumber::new(0, 0, 1),
    ),
    /*unsigned*/
    (
        Sha256Digest(hex!(
            "8f45ae6b72c241a20798acbd3c6d3e54071cae73e335df1785f2d485a915da4c"
        )),
        VersionNumber::new(0, 0, 1),
    ),
];
