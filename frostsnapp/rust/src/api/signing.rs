use super::super_wallet::SuperWallet;
use super::{
    bitcoin::{BitcoinNetwork, RTransaction, Transaction},
    coordinator::Coordinator,
};
use crate::{frb_generated::StreamSink, sink_wrap::SinkWrap};
use anyhow::{anyhow, Result};
use bitcoin::hex::DisplayHex;
use bitcoin::{Psbt, ScriptBuf};
use flutter_rust_bridge::frb;
pub use frostsnap_coordinator::signing::SigningState;
pub use frostsnap_core::bitcoin_transaction::TransactionTemplate;
pub use frostsnap_core::coordinator::ActiveSignSession;
pub use frostsnap_core::coordinator::{SignSessionProgress, StartSign};
use frostsnap_core::MasterAppkey;
use frostsnap_core::{
    message::EncodedSignature, AccessStructureRef, DeviceId, KeyId, SignSessionId, WireSignTask,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
#[frb(non_opaque)]
pub enum SigningDetails {
    Message {
        message: String,
    },
    Transaction {
        transaction: crate::api::bitcoin::Transaction,
    },
    Nostr {
        id: String,
        content: String,
        hash_bytes: String,
    },
}

#[frb(mirror(SigningState), unignore)]
pub struct _SigningState {
    pub session_id: SignSessionId,
    pub got_shares: Vec<DeviceId>,
    pub needed_from: Vec<DeviceId>,
    // for some reason FRB woudln't allow Option here to empty vec implies not being finished
    pub finished_signatures: Vec<EncodedSignature>,
    pub aborted: Option<String>,
    pub connected_but_need_request: Vec<DeviceId>,
}

#[frb(mirror(ActiveSignSession), unignore)]
pub struct _ActiveSignSession {
    pub progress: Vec<SignSessionProgress>,
    pub init: StartSign,
    pub key_id: KeyId,
    pub sent_req_to_device: HashSet<DeviceId>,
}

pub trait ActiveSignSessionExt {
    #[frb(sync)]
    fn state(&self) -> SigningState;
    #[frb(sync)]
    fn details(&self) -> SigningDetails;
}

impl ActiveSignSessionExt for ActiveSignSession {
    #[frb(sync)]
    fn state(&self) -> SigningState {
        let session_id = self.session_id();
        let session_init = &self.init;
        let got_shares = self.received_from();
        let state = SigningState {
            session_id,
            got_shares: got_shares.into_iter().collect(),
            needed_from: session_init.nonces.keys().copied().collect(),
            finished_signatures: Vec::new(),
            aborted: None,
            connected_but_need_request: Default::default(),
        };

        state
    }

    #[frb(sync)]
    fn details(&self) -> SigningDetails {
        let session_init = &self.init;

        let res = match session_init.group_request.sign_task.clone() {
            WireSignTask::Test { message } => SigningDetails::Message { message },
            WireSignTask::Nostr { event } => SigningDetails::Nostr {
                id: event.id,
                content: event.content,
                hash_bytes: event.hash_bytes.to_lower_hex_string(),
            },
            WireSignTask::BitcoinTransaction(tx_temp) => {
                let raw_tx = tx_temp.to_rust_bitcoin_tx();
                let txid = raw_tx.compute_txid();
                let is_mine = tx_temp
                    .iter_locally_owned_inputs()
                    .map(|(_, _, spk)| spk.spk())
                    .chain(
                        tx_temp
                            .iter_locally_owned_outputs()
                            .map(|(_, _, spk)| spk.spk()),
                    )
                    .collect::<HashSet<_>>();
                let prevouts = tx_temp
                    .inputs()
                    .iter()
                    .map(|input| (input.outpoint(), input.txout()))
                    .collect::<HashMap<bitcoin::OutPoint, bitcoin::TxOut>>();
                SigningDetails::Transaction {
                    transaction: Transaction {
                        inner: raw_tx,
                        txid: txid.to_string(),
                        confirmation_time: None,
                        last_seen: None,
                        prevouts,
                        is_mine,
                    },
                }
            }
        };
        res
    }
}

#[derive(Clone, Debug)]
pub struct UnsignedTx {
    pub template_tx: TransactionTemplate,
}

impl UnsignedTx {
    #[frb(sync)]
    pub fn txid(&self) -> String {
        self.template_tx.txid().to_string()
    }

    #[frb(sync, type_64bit_int)]
    pub fn fee(&self) -> Option<u64> {
        self.template_tx.fee()
    }

    #[frb(sync)]
    pub fn feerate(&self) -> Option<f64> {
        self.template_tx.feerate()
    }

    #[frb(sync)]
    pub fn attach_signatures_to_psbt(
        &self,
        signatures: Vec<EncodedSignature>,
        mut psbt: Psbt,
    ) -> Psbt {
        let mut signatures = signatures.into_iter();
        for (i, _, _) in self.template_tx.iter_locally_owned_inputs() {
            let signature = signatures.next();
            // we are assuming the signatures are correct here.
            let input = &mut psbt.inputs[i];
            let schnorr_sig = bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(
                    &signature.unwrap().0,
                )
                .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            };
            input.tap_key_sig = Some(schnorr_sig);
        }

        psbt
    }

    #[frb(sync)]
    pub fn details(&self, super_wallet: &SuperWallet, master_appkey: MasterAppkey) -> Transaction {
        let super_wallet = super_wallet.inner.lock().unwrap();
        let raw_tx = self.template_tx.to_rust_bitcoin_tx();
        let txid = raw_tx.compute_txid();
        Transaction {
            txid: txid.to_string(),
            confirmation_time: None,
            last_seen: None,
            prevouts: super_wallet
                .get_prevouts(raw_tx.input.iter().map(|txin| txin.previous_output)),
            is_mine: raw_tx
                .output
                .iter()
                .chain(
                    super_wallet
                        .get_prevouts(raw_tx.input.iter().map(|txin| txin.previous_output))
                        .values(),
                )
                .map(|txout| txout.script_pubkey.clone())
                .filter(|spk| super_wallet.is_spk_mine(master_appkey, spk.clone()))
                .collect::<HashSet<ScriptBuf>>(),
            inner: raw_tx,
        }
    }

    #[frb(sync)]
    pub fn complete(&self, signatures: Vec<EncodedSignature>) -> SignedTx {
        let mut tx = self.template_tx.to_rust_bitcoin_tx();
        for (txin, signature) in tx.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                    .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.witness = witness;
        }

        SignedTx {
            signed_tx: tx,
            unsigned_tx: self.clone(),
        }
    }

    #[frb(sync)]
    pub fn effect(
        &self,
        master_appkey: MasterAppkey,
        network: BitcoinNetwork,
    ) -> Result<EffectOfTx> {
        use frostsnap_core::bitcoin_transaction::RootOwner;
        let fee = self
            .template_tx
            .fee()
            .ok_or(anyhow!("invalid transaction"))?;
        let mut net_value = self.template_tx.net_value();
        let value_for_this_key = net_value
            .remove(&RootOwner::Local(master_appkey))
            .ok_or(anyhow!("this transaction has no effect on this key"))?;

        let foreign_receiving_addresses = net_value
            .into_iter()
            .filter_map(|(owner, value)| match owner {
                RootOwner::Local(_) => Some(Err(anyhow!(
                    "we don't support spending from multiple different keys"
                ))),
                RootOwner::Foreign(spk) => {
                    if value > 0 {
                        Some(Ok((
                            bitcoin::Address::from_script(spk.as_script(), network)
                                .expect("will have address form")
                                .to_string(),
                            value as u64,
                        )))
                    } else {
                        None
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(EffectOfTx {
            net_value: value_for_this_key,
            fee,
            feerate: self.template_tx.feerate(),
            foreign_receiving_addresses,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SignedTxDetails {
    pub session_id: SignSessionId,
    pub tx: Transaction,
}

#[derive(Debug, Clone)]
pub struct SignedTx {
    pub signed_tx: RTransaction,
    pub unsigned_tx: UnsignedTx,
}

impl SignedTx {
    #[frb(sync)]
    pub fn txid(&self) -> String {
        self.signed_tx.compute_txid().to_string()
    }

    #[frb(sync)]
    pub fn effect(
        &self,
        master_appkey: MasterAppkey,
        network: BitcoinNetwork,
    ) -> Result<EffectOfTx> {
        self.unsigned_tx.effect(master_appkey, network)
    }
}

impl Coordinator {
    pub fn start_signing(
        &self,
        access_structure_ref: AccessStructureRef,
        devices: Vec<DeviceId>,
        message: String,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::Test { message },
            SinkWrap(sink),
        )?;
        Ok(())
    }

    pub fn start_signing_tx(
        &self,
        access_structure_ref: AccessStructureRef,
        unsigned_tx: UnsignedTx,
        devices: Vec<DeviceId>,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0.start_signing(
            access_structure_ref,
            devices.into_iter().collect(),
            WireSignTask::BitcoinTransaction(unsigned_tx.template_tx.clone()),
            SinkWrap(sink),
        )?;
        Ok(())
    }

    #[frb(sync)]
    pub fn nonces_available(&self, id: DeviceId) -> u32 {
        self.0.nonces_available(id)
    }

    pub fn try_restore_signing_session(
        &self,
        session_id: SignSessionId,
        sink: StreamSink<SigningState>,
    ) -> Result<()> {
        self.0
            .try_restore_signing_session(session_id, SinkWrap(sink))
    }

    #[frb(sync)]
    pub fn active_signing_session(&self, session_id: SignSessionId) -> Option<ActiveSignSession> {
        self.0
            .inner()
            .active_signing_sessions_by_ssid()
            .get(&session_id)
            .cloned()
    }

    #[frb(sync)]
    pub fn active_signing_sessions(&self, key_id: KeyId) -> Vec<ActiveSignSession> {
        self.0
            .inner()
            .active_signing_sessions()
            .filter(|session| session.key_id == key_id)
            .collect()
    }

    #[frb(sync)]
    pub fn unbroadcasted_txs(
        &self,
        super_wallet: &SuperWallet,
        key_id: KeyId,
    ) -> Vec<SignedTxDetails> {
        let coord = self.0.inner();
        let super_wallet = super_wallet.inner.lock().unwrap();
        let txs = coord
            .finished_signing_sessions()
            .iter()
            .filter(|(_, session)| session.key_id == key_id)
            .filter_map(|(_, session)| match &session.init.group_request.sign_task {
                WireSignTask::BitcoinTransaction(tx_temp) => {
                    let mut raw_tx = tx_temp.to_rust_bitcoin_tx();
                    let txid = raw_tx.compute_txid();
                    // Filter out txs that are already broadcasted.
                    if super_wallet.get_tx(txid).is_some() {
                        return None;
                    }
                    for (txin, signature) in raw_tx.input.iter_mut().zip(&session.signatures) {
                        let schnorr_sig = bitcoin::taproot::Signature {
                            signature: bitcoin::secp256k1::schnorr::Signature::from_slice(
                                &signature.0,
                            )
                            .unwrap(),
                            sighash_type: bitcoin::sighash::TapSighashType::Default,
                        };
                        let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
                        txin.witness = witness;
                    }
                    let is_mine = tx_temp
                        .iter_locally_owned_inputs()
                        .map(|(_, _, spk)| spk.spk())
                        .chain(
                            tx_temp
                                .iter_locally_owned_outputs()
                                .map(|(_, _, spk)| spk.spk()),
                        )
                        .collect::<HashSet<_>>();
                    let prevouts = tx_temp
                        .inputs()
                        .iter()
                        .map(|input| (input.outpoint(), input.txout()))
                        .collect::<HashMap<bitcoin::OutPoint, bitcoin::TxOut>>();
                    Some(SignedTxDetails {
                        session_id: session.init.group_request.session_id(),
                        tx: Transaction {
                            inner: raw_tx,
                            txid: txid.to_string(),
                            confirmation_time: None,
                            last_seen: None,
                            prevouts: prevouts,
                            is_mine,
                        },
                    })
                }
                _ => None,
            });
        txs.collect()
    }

    pub fn request_device_sign(
        &self,
        device_id: DeviceId,
        session_id: SignSessionId,
    ) -> Result<()> {
        self.0
            .request_device_sign(device_id, session_id, crate::TEMP_KEY)
    }

    pub fn cancel_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        self.0.cancel_sign_session(ssid)
    }

    pub fn forget_finished_sign_session(&self, ssid: SignSessionId) -> Result<()> {
        self.0.forget_finished_sign_session(ssid)
    }

    pub fn sub_signing_session_signals(&self, key_id: KeyId, sink: StreamSink<()>) {
        self.0.sub_signing_session_signals(key_id, SinkWrap(sink))
    }
}

#[derive(Clone, Debug)]
#[frb(type_64bit_int)]
pub struct EffectOfTx {
    pub net_value: i64,
    pub fee: u64,
    pub feerate: Option<f64>,
    pub foreign_receiving_addresses: Vec<(String, u64)>,
}
