use crate::{bitcoin_transaction, device::KeyPurpose, tweak::AppTweak, MasterAppkey};
use alloc::{boxed::Box, string::String, vec::Vec};
use bitcoin::hashes::Hash;
use schnorr_fun::{Message, Schnorr, Signature};

#[derive(Debug, Clone, bincode::Encode, bincode::Decode, PartialEq, Eq, Hash)]
pub enum WireSignTask {
    Test {
        message: String,
    },
    Nostr {
        #[bincode(with_serde)]
        event: Box<crate::nostr::UnsignedEvent>,
    },
    BitcoinTransaction(bitcoin_transaction::TransactionTemplate),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignTask {
    Test {
        message: String,
    },
    Nostr {
        event: Box<crate::nostr::UnsignedEvent>,
    },
    BitcoinTransaction {
        tx_template: bitcoin_transaction::TransactionTemplate,
        network: bitcoin::Network,
    },
}

#[derive(Debug, Clone, PartialEq)]
/// A sign task bound to a single key. We only support signing tasks with single keys for now.
pub struct CheckedSignTask {
    /// The appkey it the task was checked against. Indicates that for example, the Bitcoin
    /// transaction was signing inputs whose public key was derived from this.
    pub master_appkey: MasterAppkey,
    pub inner: SignTask,
}

impl WireSignTask {
    pub fn check(
        self,
        master_appkey: MasterAppkey,
        purpose: KeyPurpose,
    ) -> Result<CheckedSignTask, SignTaskError> {
        let variant = match self {
            WireSignTask::Test { message } => {
                // We allow any kind of key to sign a test message
                SignTask::Test { message }
            }
            WireSignTask::Nostr { event } => {
                if !matches!(purpose, KeyPurpose::Nostr) {
                    return Err(SignTaskError::WrongPurpose);
                }
                SignTask::Nostr { event }
            }
            WireSignTask::BitcoinTransaction(tx_template) => {
                let network = match purpose {
                    KeyPurpose::Bitcoin(network) => network,
                    _ => return Err(SignTaskError::WrongPurpose),
                };
                let non_matching_key = tx_template.inputs().iter().find_map(|input| {
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

                if tx_template.fee().is_none() {
                    return Err(SignTaskError::InvalidBitcoinTransaction);
                }

                SignTask::BitcoinTransaction {
                    tx_template,
                    network,
                }
            }
        };
        Ok(CheckedSignTask {
            master_appkey,
            inner: variant,
        })
    }
}

impl CheckedSignTask {
    pub fn into_inner(self) -> SignTask {
        self.inner
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
        match &self.inner {
            SignTask::Test { message } => vec![SignItem {
                message: message.as_bytes().to_vec(),
                app_tweak: AppTweak::TestMessage,
            }],
            SignTask::Nostr { event } => vec![SignItem {
                message: event.hash_bytes.clone(),
                app_tweak: AppTweak::Nostr,
            }],
            SignTask::BitcoinTransaction { tx_template, .. } => tx_template
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
        self.schnorr_fun_message();
        schnorr.verify(&derived_key, self.schnorr_fun_message(), signature)
    }

    pub fn schnorr_fun_message(&self) -> schnorr_fun::Message<'_> {
        match self.app_tweak {
            AppTweak::TestMessage => Message::new("frostsnap-test", &self.message[..]),
            AppTweak::Bitcoin(_) => Message::raw(&self.message[..]),
            AppTweak::Nostr => Message::raw(&self.message[..]),
        }
    }
}

#[derive(Clone, Debug)]
pub enum SignTaskError {
    WrongKey {
        got: Box<MasterAppkey>,
        expected: Box<MasterAppkey>,
    },
    WrongPurpose,
    InvalidBitcoinTransaction,
}

impl core::fmt::Display for SignTaskError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignTaskError::WrongKey { got, expected } => write!(
                f,
                "sign task was for key {expected} but got an item for key {got}",
            ),
            SignTaskError::InvalidBitcoinTransaction => {
                write!(f, "Bitcoin transaction input value was less than outputs")
            }
            SignTaskError::WrongPurpose => {
                write!(
                    f,
                    "Coordinator tried to use key for something other than its intended purpose"
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SignTaskError {}
