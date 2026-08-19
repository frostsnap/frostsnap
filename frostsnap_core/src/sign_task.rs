use crate::{
    bitcoin_transaction,
    device::KeyPurpose,
    tweak::{AppTweak, BitcoinAccount, BitcoinAccountKeychain, Keychain, NormalIndex},
    MasterAppkey,
};
use alloc::{boxed::Box, string::String, vec::Vec};
use bitcoin::hashes::Hash;
use schnorr_fun::{Message, Schnorr, Signature};

/// TEMPORARY ceiling on the index of an output a sign task may claim as ours, bounding the space
/// a coordinator can hide one in. A chosen number, not a property of the wallet: chain activity
/// moves the revealed range too, so "past what ordinary use reaches" is not something this side
/// can assert.
const OUTPUT_INDEX_LIMIT: u32 = 200_000;

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
        tx_template: bitcoin_transaction::TransactionTemplate<bitcoin_transaction::ScopedTo>,
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
                .as_seen_by(master_appkey)
                .iter_locally_owned_outputs()
                .filter_map(move |(_, _, spk)| {
                    (spk.bip32_path.account_keychain == internal)
                        .then_some(spk.bip32_path.index.to_u32())
                })
                .collect::<Vec<_>>()
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
                // A coordinator may put another key's scripts in the transaction; that is its
                // business. What it may not do is have them reach us as ours, so they are
                // demoted to foreign here and every reader below is right by construction.
                let tx_template = tx_template.as_seen_by(master_appkey);

                // TEMPORARY. A blunt narrowing of the paths a coordinator may present as ours,
                // while the wallet's account and address-issuance model is still implicit. Not a
                // considered final model.
                //
                // Outputs only. An input path is self-certifying: `Input::txout` builds the
                // prevout from the claimed path's own spk, so the sighash commits to it and a
                // forged path signs against a prevout that is not on chain. Policing inputs would
                // buy nothing and would strand coins a PSBT legitimately imports.
                //
                // The known cost is that only this side is bounded: an honest coordinator that
                // allocated change or revealed an address past the ceiling would have its own task
                // refused here. Reaching that is implausible today, and closing it properly means
                // one shared issuance boundary in the wallet, which is the cleanup.
                for (_, _, owner) in tx_template.iter_locally_owned_outputs() {
                    let path = owner.bip32_path;

                    if path.account_keychain.account != BitcoinAccount::default() {
                        return Err(SignTaskError::UnwatchedAccount {
                            account: path.account_keychain.account,
                        });
                    }

                    if path.index.to_u32() >= OUTPUT_INDEX_LIMIT {
                        return Err(SignTaskError::OutputIndexOutOfRange { index: path.index });
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
                .map(|(owner, sighash)| SignItem {
                    message: sighash.as_raw_hash().to_byte_array().to_vec(),
                    app_tweak: AppTweak::Bitcoin(owner.bip32_path),
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
    UnwatchedAccount { account: BitcoinAccount },
    OutputIndexOutOfRange { index: NormalIndex },
    WrongPurpose,
    InvalidBitcoinTransaction,
    NothingToSign,
}

impl core::fmt::Display for SignTaskError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignTaskError::UnwatchedAccount { account } => write!(
                f,
                "sign task has an output in an account this wallet doesn't use: {account:?}",
            ),
            SignTaskError::OutputIndexOutOfRange { index } => write!(
                f,
                "sign task has an output at index {index}, past the limit of {OUTPUT_INDEX_LIMIT}",
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
    use crate::bitcoin_transaction::PromptRecipient;
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

    /// Another key's output is money leaving this one, so the signer must be shown it as a
    /// recipient rather than have it silently counted as change coming back.
    #[test]
    fn an_output_under_another_key_is_presented_as_leaving() {
        let signing = signing_key();
        let other = other_key();
        let their_spk = LocalSpk {
            master_appkey: other,
            bip32_path: BitcoinBip32Path::internal(NormalIndex::ZERO),
        };

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk {
                master_appkey: signing,
                bip32_path: BitcoinBip32Path::external(NormalIndex::ZERO),
            },
            Amount::from_sat(100_000),
        );
        tx_template.push_owned_output(Amount::from_sat(90_000), their_spk.clone());

        let checked = WireSignTask::BitcoinTransaction(tx_template)
            .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin))
            .expect("another key's output is tolerated, not rejected");

        let SignTask::BitcoinTransaction { tx_template, .. } = &checked.inner else {
            panic!("expected a bitcoin task")
        };

        assert_eq!(
            tx_template.foreign_recipients().collect::<Vec<_>>(),
            vec![(their_spk.spk().as_script(), 90_000)],
            "their output is a recipient, at their address"
        );
        assert_eq!(
            tx_template.iter_locally_owned_outputs().count(),
            0,
            "and it is not ours"
        );

        // What the signer is actually shown. `owned` is the load-bearing part: without
        // normalising, the prompt would derive it from `SpkOwner::Local` and tell the user
        // another key's output comes back to them.
        let prompt = tx_template.user_prompt(Network::Bitcoin);
        let their_address = bitcoin::Address::from_script(
            their_spk.spk().as_script(),
            bitcoin::params::Params::MAINNET,
        )
        .expect("p2tr has an address");

        assert_eq!(
            prompt.recipients,
            vec![PromptRecipient {
                address: their_address,
                amount: Amount::from_sat(90_000),
                owned: None,
            }],
            "their output is disclosed at their address, and not as ours"
        );
        assert_eq!(prompt.fee, Amount::from_sat(10_000));
    }

    /// A locally-owned input under a different key must be rejected.
    /// An input under another key is one we cannot sign, so it must not be counted as ours —
    /// leaving nothing here to sign at all.
    #[test]
    fn an_input_under_another_key_is_not_ours_to_sign() {
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
        tx_template.push_foreign_output(bitcoin::TxOut {
            value: Amount::from_sat(90_000),
            script_pubkey: bitcoin::ScriptBuf::from_bytes(
                [&[0x51u8, 0x20][..], &[0xcd; 32][..]].concat(),
            ),
        });

        let result = WireSignTask::BitcoinTransaction(tx_template)
            .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin));

        assert!(
            matches!(result, Err(SignTaskError::NothingToSign)),
            "their input is not ours to sign, so there is nothing here for us; got {result:?}"
        );
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

    /// One test for the whole temporary policy: it is three lines of check and will be replaced,
    /// so it gets its boundaries pinned once rather than a case each.
    #[test]
    fn temporary_path_policy() {
        let signing = signing_key();
        let other_account = crate::tweak::BitcoinAccountKeychain {
            account: BitcoinAccount {
                index: NormalIndex::new(1).unwrap(),
                ..Default::default()
            },
            keychain: crate::tweak::Keychain::External,
        };
        let limit = NormalIndex::new(OUTPUT_INDEX_LIMIT).unwrap();
        let below_limit = NormalIndex::new(OUTPUT_INDEX_LIMIT - 1).unwrap();

        let spend_to_self = |input: BitcoinBip32Path, output: BitcoinBip32Path| {
            let mut tx = TransactionTemplate::new();
            tx.push_imaginary_owned_input(
                LocalSpk {
                    master_appkey: signing,
                    bip32_path: input,
                },
                Amount::from_sat(100_000),
            );
            tx.push_owned_output(
                Amount::from_sat(90_000),
                LocalSpk {
                    master_appkey: signing,
                    bip32_path: output,
                },
            );
            WireSignTask::BitcoinTransaction(tx)
                .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin))
        };
        let external = BitcoinBip32Path::external;

        assert!(
            spend_to_self(external(NormalIndex::ZERO), external(below_limit)).is_ok(),
            "the highest in-range output index must still be accepted"
        );

        assert!(matches!(
            spend_to_self(external(NormalIndex::ZERO), external(limit)),
            Err(SignTaskError::OutputIndexOutOfRange { index }) if index == limit
        ));

        let in_other_account = BitcoinBip32Path {
            account_keychain: other_account,
            index: NormalIndex::ZERO,
        };

        assert!(matches!(
            spend_to_self(external(NormalIndex::ZERO), in_other_account),
            Err(SignTaskError::UnwatchedAccount { .. })
        ));

        assert!(
            spend_to_self(in_other_account, external(NormalIndex::ZERO)).is_ok(),
            "an input is a coin we control whatever its path, so it must stay spendable"
        );
    }
}
