use frostsnap_comms::{DeviceSendBody, Sha256Digest, FIRMWARE_UPGRADE_CHUNK_LEN};
use frostsnap_macros::hex;

pub use frostsnap_comms::firmware_version::{
    Environment, FirmwareFeatures, FirmwareLabel, VersionNumber, EARLIEST_ACCEPTABLE,
};

/// A firmware image the coordinator holds, identified by the version embedded in it and, through
/// its environment, by the key that signed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareVersion {
    pub digest: Sha256Digest,
    pub version: VersionNumber,
    pub environment: Environment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceVersion {
    /// Announced with `Announce2`.
    Reported {
        version: VersionNumber,
        environment: Environment,
    },
    /// Announced with `Announce`, as every firmware before 0.5.0 does. Says
    /// nothing authenticated about age.
    Unreported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFirmware {
    pub digest: Sha256Digest,
    pub version: DeviceVersion,
}

impl FirmwareVersion {
    pub fn version_name(&self) -> String {
        let label = FirmwareLabel {
            version: self.version,
            digest: self.digest,
            environment: self.environment,
        };
        format!("v{label}")
    }

    /// A device boots only images signed for its own environment, so the app doesn't offer it any
    /// other. Dev builds share a version, so only production ones are ordered by it.
    pub fn check_upgrade_eligibility(&self, device: &DeviceFirmware) -> FirmwareUpgradeEligibility {
        use Environment::*;
        use FirmwareUpgradeEligibility::*;

        if device.digest == self.digest {
            return UpToDate;
        }

        let DeviceVersion::Reported {
            version: device_version,
            environment: device_environment,
        } = device.version
        else {
            return CanUpgrade;
        };

        match (self.environment, device_environment) {
            (Production, Production) if self.version >= device_version => CanUpgrade,
            (Production, Production) => CannotUpgrade {
                reason: "Device firmware is newer than this app's. Upgrade the app.".to_string(),
            },
            (Dev, Dev) => CanUpgrade,
            (Dev, Production) => CannotUpgrade {
                reason: "This is a development app. It can't upgrade a production device."
                    .to_string(),
            },
            (Production, Dev) => CannotUpgrade {
                reason: "This is a development device. A release app can't upgrade it.".to_string(),
            },
        }
    }
}

impl DeviceFirmware {
    /// The firmware a device announces, or `None` if `body` isn't an announcement.
    pub fn announced(body: &DeviceSendBody) -> Option<Self> {
        match *body {
            DeviceSendBody::Announce { firmware_digest } => Some(Self {
                digest: firmware_digest,
                version: DeviceVersion::Unreported,
            }),
            DeviceSendBody::Announce2 {
                firmware_digest,
                version,
                environment,
            } => Some(Self {
                digest: firmware_digest,
                version: DeviceVersion::Reported {
                    version,
                    environment,
                },
            }),
            _ => None,
        }
    }

    pub fn features(&self) -> FirmwareFeatures {
        match self.version {
            DeviceVersion::Reported { version, .. } => version.features(),
            DeviceVersion::Unreported => shipped_release(&self.digest)
                .unwrap_or(LATEST_SHIPPED_RELEASE)
                .features(),
        }
    }

    pub fn version_name(&self) -> String {
        match (self.version, shipped_release(&self.digest)) {
            (
                DeviceVersion::Reported {
                    version,
                    environment,
                },
                _,
            ) => {
                let label = FirmwareLabel {
                    version,
                    digest: self.digest,
                    environment,
                };
                format!("v{label}")
            }
            (_, Some(release)) => format!("v{release}"),
            (DeviceVersion::Unreported, None) => "older than v0.5.0".to_string(),
        }
    }
}

/// Body digest of every release shipped before firmware reported its version,
/// newest first. Frozen: later firmware reports its version, so nothing is ever
/// added.
const SHIPPED_RELEASES: &[(Sha256Digest, VersionNumber)] = &[
    (
        Sha256Digest(hex!(
            "037c345c9db9866d30e219ae56de5efe3905a4050a82df362590d5017e709df0"
        )),
        VersionNumber::new(0, 4, 0),
    ),
    (
        Sha256Digest(hex!(
            "6273dc08ca7c805fa70ac8feeb98c62f7b6fcb337fb1cd8412f24f0d6dda51f7"
        )),
        VersionNumber::new(0, 3, 0),
    ),
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
    (
        Sha256Digest(hex!(
            "8f45ae6b72c241a20798acbd3c6d3e54071cae73e335df1785f2d485a915da4c"
        )),
        VersionNumber::new(0, 0, 1),
    ),
];

/// v0.0.1 devices announce the digest over the full image (body plus signature
/// block) rather than the body.
const V0_0_1_FULL_IMAGE_DIGEST: Sha256Digest = Sha256Digest(hex!(
    "57161f80b41413b1053e272f9c3da8d16ecfce44793345be69f7fe03d93f4eb0"
));

/// What an unreported device whose digest isn't a shipped release (old dev
/// firmware) is taken to support: no feature newer than the last release that
/// couldn't report.
const LATEST_SHIPPED_RELEASE: VersionNumber = SHIPPED_RELEASES[0].1;

/// The shipped release a device announcing `announced_digest` runs.
pub fn shipped_release(announced_digest: &Sha256Digest) -> Option<VersionNumber> {
    if *announced_digest == V0_0_1_FULL_IMAGE_DIGEST {
        return Some(VersionNumber::new(0, 0, 1));
    }
    SHIPPED_RELEASES
        .iter()
        .find(|(digest, _)| digest == announced_digest)
        .map(|(_, version)| *version)
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
    NoEmbeddedVersion,
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
            FirmwareValidationError::NoEmbeddedVersion => {
                write!(f, "Firmware doesn't embed its version")
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

        let version = frostsnap_comms::firmware_reader::embedded_version(&firmware)
            .map_err(FirmwareValidationError::InvalidFormat)?
            .ok_or(FirmwareValidationError::NoEmbeddedVersion)?;

        let environment = match frostsnap_secure_boot::verify_firmware(firmware.as_bytes()) {
            Ok(verified)
                if frostsnap_secure_boot::classify_pinned(&verified.key_digest)
                    == frostsnap_secure_boot::Signer::Prod =>
            {
                Environment::Production
            }
            _ => Environment::Dev,
        };

        let firmware_version = FirmwareVersion {
            digest: firmware_only_digest,
            version,
            environment,
        };

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

    pub fn version(&self) -> VersionNumber {
        self.firmware_version.version
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const DEVICE_DIGEST: Sha256Digest = Sha256Digest([2; 32]);

    fn unreported(digest: Sha256Digest) -> DeviceFirmware {
        DeviceFirmware {
            digest,
            version: DeviceVersion::Unreported,
        }
    }

    #[test]
    fn a_production_device_newer_than_the_app_is_not_downgraded() {
        let app = FirmwareVersion {
            digest: Sha256Digest([1; 32]),
            version: VersionNumber::new(0, 5, 0),
            environment: Environment::Production,
        };
        let device = DeviceFirmware {
            digest: DEVICE_DIGEST,
            version: DeviceVersion::Reported {
                version: VersionNumber::new(0, 6, 0),
                environment: Environment::Production,
            },
        };
        assert!(matches!(
            app.check_upgrade_eligibility(&device),
            FirmwareUpgradeEligibility::CannotUpgrade { .. }
        ));
    }

    #[test]
    fn unknown_unreported_firmware_gets_the_latest_shipped_features() {
        let firmware = unreported(DEVICE_DIGEST);
        assert_eq!(firmware.version_name(), "older than v0.5.0");
        assert_eq!(firmware.features(), VersionNumber::new(0, 4, 0).features());
        assert!(
            !firmware.features().genuine_check && !firmware.features().upgrade_names_version,
            "a feature added after v0.4.0 stays off for a device that doesn't report"
        );
    }

    #[test]
    fn shipped_releases_are_newest_first_and_predate_reporting() {
        assert!(SHIPPED_RELEASES.windows(2).all(|w| w[0].1 > w[1].1));
        assert!(SHIPPED_RELEASES
            .iter()
            .all(|(_, version)| *version < VersionNumber::new(0, 5, 0)));
        assert_eq!(
            shipped_release(&SHIPPED_RELEASES.last().unwrap().0),
            Some(VersionNumber::new(0, 0, 1)),
            "v0.0.1's body digest names it too"
        );
    }
}
