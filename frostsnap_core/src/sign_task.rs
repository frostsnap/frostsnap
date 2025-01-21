use crate::{bitcoin_transaction, tweak::AppTweak, MasterAppkey};
use alloc::{boxed::Box, string::String, vec::Vec};
use bitcoin::hashes::Hash;
use schnorr_fun::{fun::marker::*, Message, Schnorr, Signature};

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq, Eq, Hash)]
pub enum SignTask {
    Plain {
        message: String,
    },
    Nostr {
        #[bincode(with_serde)]
        event: Box<crate::nostr::UnsignedEvent>,
    },
    BitcoinTransaction(bitcoin_transaction::TransactionTemplate),
}

#[derive(Debug, Clone, PartialEq)]
/// A sign task bound to a single key. We only support signing tasks with single keys for now.
pub struct CheckedSignTask {
    pub master_appkey: MasterAppkey,
    pub sign_task: SignTask,
}

impl SignTask {
    pub fn check(self, master_appkey: MasterAppkey) -> Result<CheckedSignTask, SignTaskError> {
        match &self {
            SignTask::Plain { .. } | SignTask::Nostr { .. } => {}
            SignTask::BitcoinTransaction(transaction) => {
                let non_matching_key = transaction.inputs().iter().find_map(|input| {
                    let owner = input.owner().local_owner()?;
                    if owner.master_appkey != master_appkey {
                        Some(owner.master_appkey)
                    } else {
                        None
                    }
                });

                if let Some(non_matching_key) = non_matching_key {
                    return Err(SignTaskError::WrongKey {
                        got: Box::new(non_matching_key),
                        expected: Box::new(master_appkey),
                    });
                }

                if transaction.fee().is_none() {
                    return Err(SignTaskError::InvalidBitcoinTransaction);
                }
            }
        }
        Ok(CheckedSignTask {
            master_appkey,
            sign_task: self,
        })
    }
}

impl CheckedSignTask {
    pub fn into_inner(self) -> SignTask {
        self.sign_task
    }

    pub fn verify_final_signatures<NG>(
        &self,
        schnorr: &Schnorr<sha2::Sha256, NG>,
        signatures: &[Signature],
    ) -> bool {
        self.sign_items().iter().enumerate().all(|(i, item)| {
            item.verify_final_signature(schnorr, self.master_appkey, &signatures[i])
        })
    }

    pub fn sign_items(&self) -> Vec<SignItem> {
        match &self.sign_task {
            SignTask::Plain { message } => vec![SignItem {
                message: message.as_bytes().to_vec(),
                app_tweak: AppTweak::TestMessage,
            }],
            SignTask::Nostr { event } => vec![SignItem {
                message: event.hash_bytes.clone(),
                app_tweak: AppTweak::Nostr,
            }],
            SignTask::BitcoinTransaction(transaction) => transaction
                .iter_sighashes_of_locally_owned_inputs()
                .map(|(owner, sighash)| {
                    assert_eq!(
                        owner.master_appkey, self.master_appkey,
                        "we should have checked this"
                    );
                    SignItem {
                        message: sighash.as_raw_hash().to_byte_array().to_vec(),
                        app_tweak: AppTweak::Bitcoin(owner.bip32_path),
                    }
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, bincode::Encode, bincode::Decode, PartialEq)]
pub struct SignItem {
    pub message: Vec<u8>,
    pub app_tweak: AppTweak,
}

impl SignItem {
    pub fn verify_final_signature<NG>(
        &self,
        schnorr: &Schnorr<sha2::Sha256, NG>,
        master_appkey: MasterAppkey,
        signature: &Signature,
    ) -> bool {
        let derived_key = self.app_tweak.derive_xonly_key(&master_appkey.to_xpub());
        schnorr.verify(&derived_key, self.schnorr_fun_message(), signature)
    }

    pub fn schnorr_fun_message(&self) -> schnorr_fun::Message<Public> {
        // FIXME: This shouldn't be raw -- plain messages should do domain separation
        Message::raw(&self.message[..])
    }
}

#[derive(Clone, Debug)]
pub enum SignTaskError {
    WrongKey {
        got: Box<MasterAppkey>,
        expected: Box<MasterAppkey>,
    },
    InvalidBitcoinTransaction,
}

impl core::fmt::Display for SignTaskError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignTaskError::WrongKey { got, expected } => write!(
                f,
                "sign task was for key {} but got an item for key {}",
                expected, got
            ),
            SignTaskError::InvalidBitcoinTransaction => {
                write!(f, "Bitcoin transaction input value was less than outputs")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SignTaskError {}
