//! Firmware versions as embedded in the signed image's app descriptor, and the
//! downgrade floor devices enforce against them.

use crate::Sha256Digest;

#[derive(
    bincode::Encode, bincode::Decode, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct VersionNumber {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl core::fmt::Display for VersionNumber {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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

    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let mut next = || -> Option<u8> {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let version = Self::new(next()?, next()?, next()?);
        match parts.next() {
            Some(_) => None,
            None => Some(version),
        }
    }

    pub fn features(&self) -> FirmwareFeatures {
        const V0_0_1: VersionNumber = VersionNumber::new(0, 0, 1);
        const V0_3_0: VersionNumber = VersionNumber::new(0, 3, 0);
        const V0_4_0: VersionNumber = VersionNumber::new(0, 4, 0);

        FirmwareFeatures {
            upgrade_digest_no_sig: *self > V0_0_1,
            check_backup: *self >= V0_3_0,
            genuine_check: *self > V0_4_0,
            upgrade_names_version: *self > V0_4_0,
        }
    }
}

/// Whether something belongs to production or to development: a device by the factory that
/// certified it, a firmware image by the key that signed it. Every build of a release has the
/// same version, so only on a production device does a version name exactly one image.
#[derive(bincode::Encode, bincode::Decode, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Environment {
    Production,
    Dev,
}

/// How a build is named to a person: in development its version is shared by many builds, so
/// the name also carries the start of its firmware digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareLabel {
    pub version: VersionNumber,
    pub digest: Sha256Digest,
    pub environment: Environment,
}

impl core::fmt::Display for FirmwareLabel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.version)?;
        if self.environment == Environment::Dev {
            write!(f, " dev ")?;
            for byte in &self.digest.0[..4] {
                write!(f, "{byte:02x}")?;
            }
        }
        Ok(())
    }
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FirmwareFeatures {
    /// Device supports firmware digest verification without signature block
    pub upgrade_digest_no_sig: bool,
    /// Device supports the check backup quiz workflow
    pub check_backup: bool,
    /// Device answers `CoordinatorSendBody::GenuineCheck`
    pub genuine_check: bool,
    /// Device understands `CoordinatorUpgradeMessage::PrepareUpgrade3`
    pub upgrade_names_version: bool,
}

/// Devices built from this tree refuse to install any image built as an
/// earlier release number than this, and any image that doesn't embed a
/// version at all (every release before 0.5.0). Bumped when a release is cut.
pub const EARLIEST_ACCEPTABLE: VersionNumber = VersionNumber::new(0, 4, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionRefusal {
    /// The image embeds no version, or one below [`EARLIEST_ACCEPTABLE`].
    BelowFloor,
    /// The image embeds a version other than the one the coordinator announced.
    NotAnnounced,
}

/// Whether a downloaded image may be installed, judged by the version it embeds.
/// `announced` is `None` when the coordinator didn't name one.
pub fn upgrade_version_check(
    embedded: Option<VersionNumber>,
    announced: Option<VersionNumber>,
) -> Result<(), VersionRefusal> {
    match embedded {
        Some(version) if version >= EARLIEST_ACCEPTABLE => {}
        _ => return Err(VersionRefusal::BelowFloor),
    }
    match announced {
        Some(announced) if embedded != Some(announced) => Err(VersionRefusal::NotAnnounced),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn the_device_crate_version_parses() {
        let version = include_str!("../../device/Cargo.toml")
            .lines()
            .find_map(|line| line.strip_prefix("version = "))
            .expect("device/Cargo.toml has a version")
            .trim_matches('"');
        assert!(VersionNumber::parse(version).is_some(), "{version}");
    }

    #[test]
    fn upgrade_version_check_requires_the_floor_and_the_announced_version() {
        let release = VersionNumber::new(0, 5, 0);
        let other = VersionNumber::new(0, 6, 0);
        let below_floor = VersionNumber::new(0, 3, 0);

        assert_eq!(upgrade_version_check(Some(release), Some(release)), Ok(()));
        assert_eq!(
            upgrade_version_check(Some(release), Some(other)),
            Err(VersionRefusal::NotAnnounced)
        );
        for announced in [None, Some(release)] {
            assert_eq!(
                upgrade_version_check(None, announced),
                Err(VersionRefusal::BelowFloor)
            );
            assert_eq!(
                upgrade_version_check(Some(below_floor), announced),
                Err(VersionRefusal::BelowFloor)
            );
        }
    }
}
