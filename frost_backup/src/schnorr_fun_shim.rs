//! `schnorr_fun` types and functions widened to allow index `0`.
//!
//! A `#0` backup is the point `(0, secret)` on the sharing polynomial, but `schnorr_fun` 0.13's
//! [`frost::ShareImage`] and [`frost::SecretShare`] require a non-zero index. Delete this module
//! once `schnorr_fun` accepts index 0.

use alloc::vec::Vec;
use schnorr_fun::{
    frost::{self, SharedKey},
    fun::{poly, prelude::*},
};

/// A share index that may be `0`: the `#0` backup.
pub type ShareIndex = Scalar<Public, Zero>;

/// A [`frost::ShareImage`] whose index may be `0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShareImage {
    pub index: ShareIndex,
    pub image: Point<Normal, Public, Zero>,
}

impl ShareImage {
    /// `None` at index `0`.
    pub fn non_zero(self) -> Option<frost::ShareImage> {
        Some(frost::ShareImage {
            index: self.index.non_zero()?,
            image: self.image,
        })
    }
}

impl From<frost::ShareImage> for ShareImage {
    fn from(image: frost::ShareImage) -> Self {
        ShareImage {
            index: image.index.mark_zero(),
            image: image.image,
        }
    }
}

/// A [`frost::SecretShare`] whose index may be `0`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct SecretShare {
    pub index: ShareIndex,
    pub share: Scalar<Secret, Zero>,
}

impl SecretShare {
    pub fn share_image(&self) -> ShareImage {
        ShareImage {
            index: self.index,
            image: g!(self.share * G).normalize(),
        }
    }

    /// `None` at index `0`.
    pub fn non_zero(self) -> Option<frost::SecretShare> {
        Some(frost::SecretShare {
            index: self.index.non_zero()?,
            share: self.share,
        })
    }

    /// Interpolates the shares' polynomial at `0`.
    pub fn recover_secret(shares: &[SecretShare]) -> Scalar<Secret, Zero> {
        let indices = shares.iter().map(|share| share.index);
        shares.iter().fold(s!(0), |secret, share| {
            let basis = poly::lagrange_basis_poly(share.index, indices.clone());
            s!(secret + basis[0] * share.share)
        })
    }
}

impl From<frost::SecretShare> for SecretShare {
    fn from(share: frost::SecretShare) -> Self {
        SecretShare {
            index: share.index.mark_zero(),
            share: share.share,
        }
    }
}

/// [`SharedKey::from_share_images`] for images whose index may be `0`.
pub fn shared_key_from_share_images(
    images: impl IntoIterator<Item = ShareImage>,
) -> SharedKey<Normal, Zero> {
    let points: Vec<_> = images
        .into_iter()
        .map(|image| (image.index, image.image))
        .collect();
    let poly = poly::point::interpolate(&points);
    SharedKey::from_poly(poly::point::normalize(poly).collect())
}

/// [`SharedKey::share_image`] at an index that may be `0`.
pub fn share_image<Z: ZeroChoice>(
    shared_key: &SharedKey<Normal, Z>,
    index: ShareIndex,
) -> ShareImage {
    ShareImage {
        index,
        image: poly::point::eval(shared_key.point_polynomial(), index).normalize(),
    }
}
