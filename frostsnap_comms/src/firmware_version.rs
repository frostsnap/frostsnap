//! The shared record of released firmware versions. Data only — what the
//! device and coordinator do with it lives in their own crates — and the
//! digest tables stay private because the two digest conventions are easy to
//! confuse: go through the accessors, which say which one they mean.
//!
//! A release's digest is appended to the list when it should become active
//! (after its artifact is built and signed — an image can never contain its
//! own digest, and it doesn't need to).

use crate::Sha256Digest;
use frostsnap_macros::hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    /// The release `digest` identifies, under either digest convention a
    /// device may announce (body digest, or v0.0.1's full-image digest).
    pub fn from_digest(digest: &Sha256Digest) -> Option<Self> {
        if *digest == V0_0_1_FULL_IMAGE_DIGEST {
            return Some(VersionNumber::new(0, 0, 1));
        }
        KNOWN_FIRMWARE_VERSIONS
            .iter()
            .find(|(d, _)| d == digest)
            .map(|(_, v)| *v)
    }

    /// Releases strictly older than `self`, as (body digest, version) — the
    /// superseded set when `self` is [`EARLIEST_ACCEPTABLE`].
    pub fn versions_before(self) -> impl Iterator<Item = &'static (Sha256Digest, VersionNumber)> {
        KNOWN_FIRMWARE_VERSIONS
            .iter()
            .filter(move |(_, v)| *v < self)
    }

    pub fn features(&self) -> FirmwareFeatures {
        const V0_0_1: VersionNumber = VersionNumber::new(0, 0, 1);
        const V0_3_0: VersionNumber = VersionNumber::new(0, 3, 0);

        FirmwareFeatures {
            upgrade_digest_no_sig: *self > V0_0_1,
            check_backup: *self >= V0_3_0,
        }
    }
}

#[derive(bincode::Encode, bincode::Decode, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FirmwareFeatures {
    /// Device supports firmware digest verification without signature block
    pub upgrade_digest_no_sig: bool,
    /// Device supports the check backup quiz workflow
    pub check_backup: bool,
}

impl FirmwareFeatures {
    pub const fn all() -> Self {
        Self {
            upgrade_digest_no_sig: true,
            check_backup: true,
        }
    }
}

/// Devices built from this tree refuse to install any release older than
/// this — currently the upcoming release, i.e. every previous firmware is
/// rejected. Bumped when a release is cut, together with appending the
/// previous release to the list below.
pub const EARLIEST_ACCEPTABLE: VersionNumber = VersionNumber::new(0, 4, 0);

/// Body (firmware-only, sans signature block) digest of each shipped release —
/// the digest a device computes over a written image, and the
/// deterministic-build digest.
const KNOWN_FIRMWARE_VERSIONS: &[(Sha256Digest, VersionNumber)] = &[
    // Coordinator-only so registering a digest can't change the image it names.
    // Next release: delete this attribute and bump EARLIEST_ACCEPTABLE in the
    // same commit. The device can only block releases it links.
    #[cfg(feature = "coordinator")]
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

/// v0.0.1-era devices announced the digest over the FULL image (body plus
/// signature block), so that one release is also identified by this digest.
const V0_0_1_FULL_IMAGE_DIGEST: Sha256Digest = Sha256Digest(hex!(
    "57161f80b41413b1053e272f9c3da8d16ecfce44793345be69f7fe03d93f4eb0"
));

#[cfg(test)]
mod test {
    use super::*;

    /// Only the release currently shipping may sit at or above the bound.
    #[test]
    fn at_most_one_release_is_registered_but_not_superseded() {
        let registered = KNOWN_FIRMWARE_VERSIONS.len();
        let superseded = EARLIEST_ACCEPTABLE.versions_before().count();
        assert!(registered <= superseded + 1);
    }

    #[test]
    fn versions_at_or_after_the_bound_are_excluded() {
        assert_eq!(VersionNumber::new(0, 0, 1).versions_before().count(), 0);
        assert!(VersionNumber::new(0, 2, 0)
            .versions_before()
            .all(|(_, v)| *v < VersionNumber::new(0, 2, 0)));
    }

    #[test]
    fn v0_0_1_resolves_under_both_digest_conventions() {
        let v0_0_1 = VersionNumber::new(0, 0, 1);
        assert_eq!(
            VersionNumber::from_digest(&V0_0_1_FULL_IMAGE_DIGEST),
            Some(v0_0_1)
        );
        let (body, _) = KNOWN_FIRMWARE_VERSIONS
            .iter()
            .find(|(_, v)| *v == v0_0_1)
            .unwrap();
        assert_eq!(VersionNumber::from_digest(body), Some(v0_0_1));
        // The full-image digest names the release but is not a body digest, so
        // it must never appear in the superseded (blockable) set.
        assert!(EARLIEST_ACCEPTABLE
            .versions_before()
            .all(|(d, _)| *d != V0_0_1_FULL_IMAGE_DIGEST));
    }
}
