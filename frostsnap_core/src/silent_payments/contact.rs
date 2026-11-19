//! Trusted silent payment contacts.
//!
//! Adding a contact is where a threshold of devices confirm a silent payment address. Each one
//! answers with its share of `x * B_scan`, for the account key `x`, and a BIP374 proof bound to
//! what the user confirmed. The shares combine into a [`SpContact`], which the wallet signs. A
//! send to the contact is then public maths over the inputs, since
//! `a_sum * B_scan = (sum s_i) * (x * B_scan) + (sum s_i * t_i) * B_scan`, and any device trusts
//! the record by its signature.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use bitcoin::Network;
use schnorr_fun::{
    frost::{ShareIndex, SharedKey},
    fun::{marker::*, Point},
    Schnorr, Signature,
};
use sha2::Digest;

use super::{combine_account_ecdh, tagged, verify_ecdh_share, EcdhShare, SilentPaymentRecipient};
use crate::{
    sign_task::SignItem,
    tweak::{AppTweak, BitcoinAccount, Xpub},
    MasterAppkey,
};

/// The longest contact name, in bytes. Names are printable ASCII and short so a device can show all
/// of a name, exactly, on one line.
pub const MAX_CONTACT_NAME_LEN: usize = 20;

/// What the user confirms on each device when adding a contact.
#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpContactDetails {
    pub name: String,
    pub recipient: SilentPaymentRecipient,
}

impl SpContactDetails {
    pub fn has_valid_name(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= MAX_CONTACT_NAME_LEN
            && self.name.bytes().all(|byte| (0x20..0x7f).contains(&byte))
    }

    /// Names that differ only by case read as the same person.
    pub fn same_name(&self, other: &Self) -> bool {
        self.name.eq_ignore_ascii_case(&other.name)
    }

    /// Commits to the details on `network`. It is the message of each device's DLEQ proof, so a
    /// proof only counts for the record its device confirmed. The scan key and the signer's key
    /// share are already in the proof's challenge: the spend key, name and network are what this
    /// adds.
    pub fn digest(&self, network: Network) -> [u8; 32] {
        tagged("frostsnap/sp-contact-details")
            .chain_update(network.magic().to_bytes())
            .chain_update(self.recipient.scan_key.to_bytes())
            .chain_update(self.recipient.spend_key.to_bytes())
            .chain_update(self.name.as_bytes())
            .finalize()
            .into()
    }
}

/// The record a threshold of devices sign: the details and `x * B_scan` for the account key `x`.
#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpContact {
    pub details: SpContactDetails,
    pub account_ecdh: Point,
}

impl SpContact {
    pub fn sign_item(&self, network: Network) -> SignItem {
        let mut message = self.details.digest(network).to_vec();
        message.extend_from_slice(&self.account_ecdh.to_bytes());
        SignItem {
            message,
            app_tweak: AppTweak::SpContact,
        }
    }
}

/// A contact with the wallet's signature under [`AppTweak::SpContact`].
#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SignedSpContact {
    pub contact: SpContact,
    pub signature: [u8; 64],
}

impl SignedSpContact {
    pub fn verify(&self, master_appkey: MasterAppkey, network: Network) -> bool {
        let Some(signature) = Signature::from_bytes(self.signature) else {
            return false;
        };
        if !self.contact.details.has_valid_name() {
            return false;
        }
        self.contact.sign_item(network).verify_final_signature(
            &Schnorr::<sha2::Sha256, _>::verify_only(),
            master_appkey,
            &signature,
        )
    }
}

/// Checks the ECDH `shares` of `details` against the share images of `app_shared_key`, and returns
/// `x * B_scan` once they reach the threshold. Below it the combination is a valid point but not
/// the wallet's ECDH, so a payment made from it could not be found by the recipient.
pub fn combine_contact_shares(
    app_shared_key: &Xpub<SharedKey>,
    details: &SpContactDetails,
    network: Network,
    shares: &[EcdhShare],
) -> Result<Point, &'static str> {
    verify_contact_shares(app_shared_key, details, network, shares)?;
    let by_index = shares
        .iter()
        .map(|share| (share.share_index, share.ecdh_share))
        .collect::<BTreeMap<ShareIndex, Point>>();
    if by_index.len() < app_shared_key.key().threshold() {
        return Err("fewer contact ECDH shares than the threshold");
    }
    combine_account_ecdh(&by_index.into_iter().collect::<Vec<_>>())
        .ok_or("contact ECDH shares do not combine")
}

/// Checks each share's proof, without requiring a threshold of them.
pub fn verify_contact_shares(
    app_shared_key: &Xpub<SharedKey>,
    details: &SpContactDetails,
    network: Network,
    shares: &[EcdhShare],
) -> Result<(), &'static str> {
    let account = BitcoinAccount::default().derive_account_key(app_shared_key);
    let message = details.digest(network);
    let mut seen = BTreeSet::new();
    for share in shares {
        if share.scan_key != details.recipient.scan_key {
            return Err("contact ECDH share is for another scan key");
        }
        if !seen.insert(share.share_index) {
            return Err("duplicate contact ECDH share");
        }
        let image: Point<Normal, Public, Zero> = account.key().share_image(share.share_index).image;
        if !verify_ecdh_share(image, &message, share) {
            return Err("contact ECDH share has a bad proof");
        }
    }
    Ok(())
}
