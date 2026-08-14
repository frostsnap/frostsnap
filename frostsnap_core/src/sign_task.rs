use crate::{
    bitcoin_transaction,
    device::KeyPurpose,
    tweak::{AppTweak, BitcoinAccount, BitcoinAccountKeychain, Keychain},
    MasterAppkey,
};
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
    /// The internal (change) indices of `account` under `master_appkey` that this task's
    /// transaction pays — the indices a wallet must not hand out as fresh change while the task
    /// is in flight. Empty for non-Bitcoin tasks.
    pub fn reserved_change_indices(
        &self,
        master_appkey: MasterAppkey,
        account: BitcoinAccount,
    ) -> impl Iterator<Item = u32> + '_ {
        let internal = BitcoinAccountKeychain {
            account,
            keychain: Keychain::Internal,
        };
        let template = match self {
            WireSignTask::BitcoinTransaction(template) => Some(template),
            _ => None,
        };
        template.into_iter().flat_map(move |template| {
            template
                .iter_locally_owned_outputs()
                .filter_map(move |(_, _, spk)| {
                    (spk.master_appkey == master_appkey
                        && spk.bip32_path.account_keychain == internal)
                        .then_some(spk.bip32_path.index.to_u32())
                })
        })
    }

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
                let input_owners = tx_template
                    .inputs()
                    .iter()
                    .filter_map(|input| Some((input.owner().local_owner()?, "input")));
                let output_owners = tx_template
                    .outputs()
                    .iter()
                    .filter_map(|output| Some((output.owner().local_owner()?, "output")));

                let owners = input_owners.chain(output_owners);

                for (owner, kind) in owners {
                    if owner.master_appkey != master_appkey {
                        return Err(SignTaskError::WrongKey {
                            got: Box::new(owner.master_appkey),
                            expected: Box::new(master_appkey),
                            kind,
                        });
                    }
                }

                if !tx_template.has_any_inputs_to_sign() {
                    return Err(SignTaskError::NothingToSign);
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
        kind: &'static str,
    },
    WrongPurpose,
    InvalidBitcoinTransaction,
    NothingToSign,
}

impl core::fmt::Display for SignTaskError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignTaskError::WrongKey {
                got,
                expected,
                kind,
            } => write!(
                f,
                "sign task was for key {expected} but got an {kind} for key {got}",
            ),
            SignTaskError::InvalidBitcoinTransaction => {
                write!(f, "Bitcoin transaction input value was less than outputs")
            }
            SignTaskError::NothingToSign => {
                write!(f, "Transaction has no inputs that belong to this wallet")
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::bitcoin_transaction::{LocalSpk, TransactionTemplate};
    use crate::tweak::{BitcoinBip32Path, NormalIndex};
    use bitcoin::{Amount, Network, ScriptBuf, TxOut};
    use schnorr_fun::fun::prelude::*;

    // Two distinct appkeys derived from cheap fixed points: the one we sign with
    // and a different one.
    fn signing_key() -> MasterAppkey {
        MasterAppkey::derive_from_rootkey(g!(2 * G).normalize())
    }

    fn other_key() -> MasterAppkey {
        MasterAppkey::derive_from_rootkey(g!(3 * G).normalize())
    }

    /// A locally-owned output under a different key must be rejected.
    #[test]
    fn output_owned_by_wrong_key_is_rejected() {
        let signing = signing_key();
        let other = other_key();

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk {
                master_appkey: signing,
                bip32_path: BitcoinBip32Path::external(NormalIndex::ZERO),
            },
            Amount::from_sat(100_000),
        );
        tx_template.push_owned_output(
            Amount::from_sat(90_000),
            LocalSpk {
                master_appkey: other,
                bip32_path: BitcoinBip32Path::internal(NormalIndex::ZERO),
            },
        );

        let result = WireSignTask::BitcoinTransaction(tx_template)
            .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin));

        match result {
            Err(SignTaskError::WrongKey {
                got,
                expected,
                kind,
            }) => {
                assert_eq!(kind, "output");
                assert_eq!(*got, other);
                assert_eq!(*expected, signing);
            }
            other => panic!("expected WrongKey output error, got {other:?}"),
        }
    }

    /// A locally-owned input under a different key must be rejected.
    #[test]
    fn input_owned_by_wrong_key_is_rejected() {
        let signing = signing_key();
        let other = other_key();

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk {
                master_appkey: other,
                bip32_path: BitcoinBip32Path::external(NormalIndex::ZERO),
            },
            Amount::from_sat(100_000),
        );

        let result = WireSignTask::BitcoinTransaction(tx_template)
            .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin));

        match result {
            Err(SignTaskError::WrongKey {
                got,
                expected,
                kind,
            }) => {
                assert_eq!(kind, "input");
                assert_eq!(*got, other);
                assert_eq!(*expected, signing);
            }
            other => panic!("expected WrongKey input error, got {other:?}"),
        }
    }

    /// A send to an external (non-local) recipient is accepted: the owner check
    /// only applies to locally-owned outputs.
    #[test]
    fn external_recipient_output_is_accepted() {
        let signing = signing_key();

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk {
                master_appkey: signing,
                bip32_path: BitcoinBip32Path::external(NormalIndex::ZERO),
            },
            Amount::from_sat(100_000),
        );
        tx_template.push_foreign_output(TxOut {
            value: Amount::from_sat(90_000),
            script_pubkey: ScriptBuf::new(),
        });

        let checked = WireSignTask::BitcoinTransaction(tx_template)
            .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin))
            .expect("a send to an external recipient must be accepted");
        assert_eq!(checked.master_appkey, signing);
    }
}
