use crate::{
    bitcoin_transaction::{self, PushInput, TransactionTemplate},
    tweak::{AppTweak, AppTweakKind, BitcoinAccountKeychain, BitcoinBip32Path},
    MasterAppkey,
};
use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
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
    master_appkey: MasterAppkey,
    sign_task: SignTask,
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

#[derive(Clone, Debug, bincode::Encode, bincode::Decode)]
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

#[derive(Clone, Debug)]
pub enum PsbtError {
    InputMalformed(usize),
    InputMissingWitness(usize),
    InputAlreadyHasFinalWitness(usize),
    InputMissingTapInternalKey(usize),
    InputMissingTapInternalKeyOrigin(usize),
    InputFingerprintDoesntMatch(usize),
    HardenedDerivation(usize),
    UnusualDerivationPath(usize),
    InputSkpDoesntMatchDerived(usize),
}

impl core::fmt::Display for PsbtError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PsbtError::InputMalformed(input) => write!(f, "PSBT input {input} is malformed"),
            PsbtError::InputMissingWitness(input) => {
                write!(f, "PSBT input {input} missing witness or non-witness utxo")
            }
            PsbtError::InputAlreadyHasFinalWitness(input) => {
                write!(f, "PSBT input {input} already has a final_script_witness")
            }
            PsbtError::InputMissingTapInternalKey(input) => {
                write!(f, "PSBT input {input} doesn't have an tap_internal_key")
            }
            PsbtError::InputMissingTapInternalKeyOrigin(input) => {
                write!(
                    f,
                    "PSBT input {input} doesn't provide a source for the tap_internal_key"
                )
            }
            PsbtError::InputFingerprintDoesntMatch(input) => {
                write!(
                    f,
                    "PSBT input {input} fingerprint doesn't match our master app key"
                )
            }
            PsbtError::HardenedDerivation(input) => {
                write!(f, "PSBT input {input} requires hardended derivation")
            }
            PsbtError::UnusualDerivationPath(input) => {
                write!(f, "PSBT input {input} has an unusual derivation path")
            }
            PsbtError::InputSkpDoesntMatchDerived(input) => {
                write!(f, "PSBT input {input} SPK doesnt match derived SPK")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PsbtError {}

pub fn psbt_to_tx_template(
    psbt: &bitcoin::Psbt,
    master_appkey: MasterAppkey,
    owned_outputs: BTreeMap<usize, (BitcoinAccountKeychain, u32)>,
    network: bitcoin::Network,
) -> Result<TransactionTemplate, PsbtError> {
    use crate::bitcoin_transaction::LocalSpk;

    let bitcoin_app_xpub = master_appkey.derive_appkey(
        &bitcoin::key::Secp256k1::verification_only(),
        AppTweakKind::Bitcoin,
        network.into(),
    );
    let our_fingerprint = bitcoin_app_xpub.fingerprint();
    let mut template = bitcoin_transaction::TransactionTemplate::new();
    let rust_bitcoin_tx = &psbt.unsigned_tx;
    template.set_version(rust_bitcoin_tx.version);
    template.set_lock_time(rust_bitcoin_tx.lock_time);

    for (i, input) in psbt.inputs.iter().enumerate() {
        let txin = rust_bitcoin_tx
            .input
            .get(i)
            .ok_or(PsbtError::InputMalformed(i))?;

        let txout = input
            .witness_utxo
            .as_ref()
            .or_else(|| {
                let tx = input.non_witness_utxo.as_ref()?;
                tx.output.get(txin.previous_output.vout as usize)
            })
            .ok_or(PsbtError::InputMissingWitness(i))?;

        let input_push =
            PushInput::spend_outpoint(txout, txin.previous_output).with_sequence(txin.sequence);

        if input.final_script_witness.is_some() {
            return Err(PsbtError::InputAlreadyHasFinalWitness(i));
        }

        let tap_internal_key = match &input.tap_internal_key {
            Some(tap_internal_key) => tap_internal_key,
            None => return Err(PsbtError::InputMissingTapInternalKey(i)),
        };

        let (fingerprint, derivation_path) = match input.tap_key_origins.get(tap_internal_key) {
            Some(origin) => origin.1.clone(),
            None => return Err(PsbtError::InputMissingTapInternalKeyOrigin(i)),
        };

        if fingerprint != our_fingerprint {
            return Err(PsbtError::InputFingerprintDoesntMatch(i));
        }

        let normal_derivation_path = derivation_path
            .into_iter()
            .map(|child_number| match child_number {
                bitcoin::bip32::ChildNumber::Normal { index } => Ok(*index),
                _ => Err(PsbtError::HardenedDerivation(i)),
            })
            .collect::<Result<Vec<_>, PsbtError>>()?;

        let bip32_path = match BitcoinBip32Path::from_u32_slice(&normal_derivation_path) {
            Some(bip32_path) => bip32_path,
            None => return Err(PsbtError::UnusualDerivationPath(i)),
        };

        template
            .push_owned_input(
                input_push,
                bitcoin_transaction::LocalSpk {
                    master_appkey,
                    bip32_path,
                },
            )
            .map_err(|_| PsbtError::InputSkpDoesntMatchDerived(i))?;
    }

    for (i, _) in psbt.outputs.iter().enumerate() {
        let txout = &rust_bitcoin_tx.output[i];

        match owned_outputs.get(&i) {
            Some(&(account_keychain, index)) => template.push_owned_output(
                txout.value,
                LocalSpk {
                    master_appkey,
                    bip32_path: BitcoinBip32Path {
                        account_keychain,
                        index,
                    },
                },
            ),
            None => {
                template.push_foreign_output(txout.clone());
            }
        }
    }

    Ok(template)
}
