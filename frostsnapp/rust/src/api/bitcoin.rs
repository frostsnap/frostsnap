pub use bitcoin::Transaction as RTransaction;
pub use bitcoin::{
    psbt::Error as PsbtError, Address, Network as BitcoinNetwork, OutPoint, Psbt, ScriptBuf, TxOut,
    Txid,
};
use flutter_rust_bridge::frb; // or, for example, easy_ext's;
use frostsnap_coordinator::bitcoin::chain_sync::{
    default_backup_electrum_server, default_electrum_server, SUPPORTED_NETWORKS,
};
pub use frostsnap_coordinator::bitcoin::wallet::ConfirmationTime;
pub use frostsnap_coordinator::frostsnap_core::{self, MasterAppkey};
use frostsnap_core::bitcoin_transaction::TransactionTemplate;
use frostsnap_core::message::EncodedSignature;
use tracing::{event, Level};

use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Deref;
use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use super::super_wallet::TxState;

// Teach FRB where to get `Network`
#[frb(mirror(BitcoinNetwork))]
enum _BitcoinNetwork {
    /// Mainnet Bitcoin.
    Bitcoin,
    /// Bitcoin's testnet network. (In future versions this will be combined
    /// into a single variant containing the version)
    Testnet,
    /// Bitcoin's testnet4 network. (In future versions this will be combined
    /// into a single variant containing the version)
    Testnet4,
    /// Bitcoin's signet network.
    Signet,
    /// Bitcoin's regtest network.
    Regtest,
}

#[derive(Debug, Clone)]
#[frb(type_64bit_int)]
pub struct SendToRecipient {
    pub address: Address,
    pub amount: Option<u64>,
}

pub trait BitcoinNetworkExt {
    #[frb(sync)]
    fn name(&self) -> String;

    #[frb(sync)]
    fn is_mainnet(&self) -> bool;

    #[frb(sync)]
    fn descriptor_for_key(&self, master_appkey: MasterAppkey) -> String;

    #[frb(sync)]
    fn from_string(string: String) -> Option<BitcoinNetwork>;

    #[frb(sync)]
    fn validate_destination_address(&self, uri: &str) -> Result<SendToRecipient, String>;

    #[frb(sync)]
    fn default_electrum_server(&self) -> String;

    #[frb(sync)]
    fn default_backup_electrum_server(&self) -> String;

    #[frb(ignore)]
    fn bdk_file(&self, app_dir: impl AsRef<Path>) -> PathBuf;

    #[frb(sync)]
    fn validate_amount(&self, address: &str, value: u64) -> Option<String>;

    #[frb(sync)]
    fn supported_networks() -> Vec<BitcoinNetwork>;
}

impl BitcoinNetworkExt for BitcoinNetwork {
    #[frb(sync)]
    fn from_string(string: String) -> Option<BitcoinNetwork> {
        BitcoinNetwork::from_str(&string).ok()
    }

    #[frb(sync)]
    fn name(&self) -> String {
        (*self).to_string()
    }

    #[frb(sync)]
    fn is_mainnet(&self) -> bool {
        bitcoin::NetworkKind::from(*self).is_mainnet()
    }

    #[frb(sync)]
    fn descriptor_for_key(&self, master_appkey: MasterAppkey) -> String {
        let descriptor = frostsnap_coordinator::bitcoin::multi_x_descriptor_for_account(
            master_appkey,
            frostsnap_core::tweak::BitcoinAccount::default(),
            (*self).into(),
        );
        descriptor.to_string()
    }

    #[frb(sync)]
    fn validate_destination_address(&self, uri: &str) -> Result<SendToRecipient, String> {
        let uri = uri.trim();

        // Try parsing as BIP21 URI first
        if let Ok(parsed) = uri.parse::<bip21::Uri<bitcoin::address::NetworkUnchecked>>() {
            let amount = parsed.amount.map(|amt| amt.to_sat());
            let address = parsed
                .address
                .require_network(*self)
                .map_err(|e| format!("Wrong network: {}", e))?;
            Ok(SendToRecipient { address, amount })
        } else {
            // Not a URI -- try as plain address
            let address = bitcoin::Address::from_str(uri)
                // Rust-bitcoin ParseError is generally inappropriate "legacy address base58 string"
                .map_err(|_| "Invalid address".to_string())?
                .require_network(*self)
                .map_err(|e| format!("Wrong network: {}", e))?;
            Ok(SendToRecipient {
                address,
                amount: None,
            })
        }
    }

    #[frb(sync)]
    fn default_electrum_server(&self) -> String {
        default_electrum_server(*self).to_string()
    }

    #[frb(sync)]
    fn default_backup_electrum_server(&self) -> String {
        default_backup_electrum_server(*self).to_string()
    }

    #[frb(ignore)]
    fn bdk_file(&self, app_dir: impl AsRef<Path>) -> PathBuf {
        app_dir.as_ref().join(format!("wallet-{}.sql", self))
    }

    #[frb(sync)]
    fn supported_networks() -> Vec<BitcoinNetwork> {
        SUPPORTED_NETWORKS.into_iter().collect()
    }

    // FIXME: doesn't need to be on the network. Can get the script pubkey without the network.
    #[frb(sync)]
    fn validate_amount(&self, address: &str, value: u64) -> Option<String> {
        match bitcoin::Address::from_str(address) {
            Ok(address) => match address.require_network(*self) {
                Ok(address) => {
                    let dust_value = address.script_pubkey().minimal_non_dust().to_sat();
                    if value < dust_value {
                        event!(
                            Level::DEBUG,
                            value = value,
                            dust_value = dust_value,
                            "address validation rejected"
                        );
                        Some(format!("Too small to send. Must be at least {dust_value}"))
                    } else {
                        None
                    }
                }
                Err(_e) => None,
            },
            Err(_e) => None,
        }
    }
}

#[derive(Debug, Clone)]
#[frb(type_64bit_int)]
pub struct Transaction {
    pub inner: RTransaction,
    pub txid: String,
    pub confirmation_time: Option<ConfirmationTime>,
    pub last_seen: Option<u64>,
    pub prevouts: HashMap<bitcoin::OutPoint, bitcoin::TxOut>,
    pub is_mine: HashSet<bitcoin::ScriptBuf>,
}

impl Transaction {
    pub(crate) fn from_template(tx_temp: &TransactionTemplate) -> Self {
        let raw_tx = tx_temp.to_rust_bitcoin_tx();
        let txid = tx_temp.txid();
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
        Self {
            inner: raw_tx,
            txid: txid.to_string(),
            confirmation_time: None,
            last_seen: None,
            prevouts,
            is_mine,
        }
    }

    pub(crate) fn fill_signatures(&mut self, signatures: &[EncodedSignature]) {
        for (txin, signature) in self.inner.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                    .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.witness = witness;
        }
    }

    #[frb(sync)]
    pub fn raw_txid(&self) -> Txid {
        self.inner.compute_txid()
    }

    fn owned_input_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.inner
            .input
            .iter()
            .enumerate()
            .filter(|(_, txin)| {
                let prev_txout = match self.prevouts.get(&txin.previous_output) {
                    Some(txout) => txout,
                    None => return false,
                };
                self.is_mine.contains(&prev_txout.script_pubkey)
            })
            .map(|(vin, _)| vin)
    }

    #[frb(sync)]
    pub fn attach_signatures_to_psbt(
        &self,
        signatures: Vec<EncodedSignature>,
        psbt: &Psbt,
    ) -> Option<Psbt> {
        let owned_indices = self.owned_input_indices().collect::<Vec<_>>();
        if signatures.len() != owned_indices.len() {
            return None;
        }

        let mut psbt = psbt.clone();
        let mut signatures = signatures.into_iter();

        for i in self.owned_input_indices() {
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

        Some(psbt)
    }

    /// Computes the sum of all inputs, or only those whose previous output script pubkey is in
    /// `filter`, if provided. The result is `None` if any input is missing a previous output.
    fn _sum_inputs(&self, filter: Option<&HashSet<bitcoin::ScriptBuf>>) -> Option<u64> {
        let prevouts = self
            .inner
            .input
            .iter()
            .map(|txin| self.prevouts.get(&txin.previous_output))
            .collect::<Option<Vec<_>>>()?;
        Some(
            prevouts
                .into_iter()
                .filter(|prevout| {
                    match &filter {
                        Some(filter) => filter.contains(prevout.script_pubkey.as_script()),
                        // No filter.
                        None => true,
                    }
                })
                .map(|prevout| prevout.value.to_sat())
                .sum(),
        )
    }

    /// Computes the sum of all outputs, or only those whose script pubkey is in `filter`, if
    /// provided.
    fn _sum_outputs(&self, filter: Option<&HashSet<bitcoin::ScriptBuf>>) -> u64 {
        self.inner
            .output
            .iter()
            .filter(|txout| {
                match &filter {
                    Some(filter) => filter.contains(txout.script_pubkey.as_script()),
                    // No filter.
                    None => true,
                }
            })
            .map(|txout| txout.value.to_sat())
            .sum()
    }

    /// Computes the total value of all inputs. Returns `None` if any input is missing a previous
    /// output.
    #[frb(sync, type_64bit_int)]
    pub fn sum_inputs(&self) -> Option<u64> {
        self._sum_inputs(None)
    }

    /// Computes the sum of all outputs.
    #[frb(sync, type_64bit_int)]
    pub fn sum_outputs(&self) -> u64 {
        self._sum_outputs(None)
    }

    /// Computes the total value of inputs we own. Returns `None` if any owned input is missing a
    /// previous output.
    #[frb(sync, type_64bit_int)]
    pub fn sum_owned_inputs(&self) -> Option<u64> {
        self._sum_inputs(Some(&self.is_mine))
    }

    /// Computes the total value of outputs we own.
    #[frb(sync, type_64bit_int)]
    pub fn sum_owned_outputs(&self) -> u64 {
        self._sum_outputs(Some(&self.is_mine))
    }

    /// Computes the total value of inputs that spend a previous output with the given `spk`.
    ///
    /// Returns `None` if any input is missing a previous output.
    #[frb(sync, type_64bit_int)]
    pub fn sum_inputs_spending_spk(&self, spk: &bitcoin::ScriptBuf) -> Option<u64> {
        self._sum_inputs(Some(&[spk.as_script().to_owned()].into()))
    }

    /// Computes the total value of outputs that send to the given script pubkey.
    #[frb(sync, type_64bit_int)]
    pub fn sum_outputs_to_spk(&self, spk: &bitcoin::ScriptBuf) -> u64 {
        self._sum_outputs(Some(&[spk.as_script().to_owned()].into()))
    }

    /// Computes the net change in our owned balance: owned outputs minus owned inputs.
    ///
    /// Returns `None` if any owned input is missing a previous output.
    #[frb(sync, type_64bit_int)]
    pub fn balance_delta(&self) -> Option<i64> {
        let owned_inputs_sum: i64 = self
            ._sum_inputs(Some(&self.is_mine))?
            .try_into()
            .expect("net spent value must convert to i64");
        let owned_outputs_sum: i64 = self
            ._sum_outputs(Some(&self.is_mine))
            .try_into()
            .expect("net created value must convert to i64");
        Some(owned_outputs_sum.saturating_sub(owned_inputs_sum))
    }

    /// Computes the transaction fee as the difference between total input and output value.
    /// Returns `None` if any input is missing a previous output.
    #[frb(sync, type_64bit_int)]
    pub fn fee(&self) -> Option<u64> {
        let inputs_sum = self._sum_inputs(None)?;
        let outputs_sum = self._sum_outputs(None);
        Some(inputs_sum.saturating_sub(outputs_sum))
    }

    #[frb(sync, type_64bit_int)]
    pub fn timestamp(&self) -> Option<u64> {
        self.confirmation_time
            .as_ref()
            .map(|t| t.time)
            .or(self.last_seen)
    }

    /// Feerate in sats/vbyte.
    #[frb(sync)]
    pub fn feerate(&self) -> Option<f64> {
        Some(((self.fee()?) as f64) / (self.inner.vsize() as f64))
    }

    #[frb(sync)]
    pub fn recipients(&self) -> Vec<TxOutInfo> {
        self.inner
            .output
            .iter()
            .zip(0_u32..)
            .map(|(txout, vout)| TxOutInfo {
                vout,
                amount: txout.value.to_sat(),
                script_pubkey: txout.script_pubkey.clone(),
                is_mine: self.is_mine.contains(&txout.script_pubkey),
            })
            .collect()
    }

    /// Return a transaction with the following signatures added.
    pub fn with_signatures(&self, signatures: Vec<EncodedSignature>) -> RTransaction {
        let mut tx = self.inner.clone();
        for (txin, signature) in tx.input.iter_mut().zip(signatures) {
            let schnorr_sig = bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                    .unwrap(),
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            };
            let witness = bitcoin::Witness::from_slice(&[schnorr_sig.to_vec()]);
            txin.witness = witness;
        }
        tx
    }
}

#[derive(Debug, Clone)]
#[frb(type_64bit_int)]
pub struct TxOutInfo {
    pub vout: u32,
    pub amount: u64,
    pub script_pubkey: bitcoin::ScriptBuf,
    pub is_mine: bool,
}

impl TxOutInfo {
    #[frb(sync)]
    pub fn address(&self, network: BitcoinNetwork) -> Option<bitcoin::Address> {
        bitcoin::Address::from_script(&self.script_pubkey, network).ok()
    }
}

#[frb(mirror(OutPoint), unignore)]
pub struct _OutPoint {
    /// The referenced transaction's txid.
    pub txid: Txid,
    /// The index of the referenced output in its transaction's vout.
    pub vout: u32,
}

#[frb(mirror(ConfirmationTime, unignore))]
pub struct _ConfirmationTime {
    pub height: u32,
    pub time: u64,
}

impl From<Vec<frostsnap_coordinator::bitcoin::wallet::Transaction>> for TxState {
    fn from(txs: Vec<frostsnap_coordinator::bitcoin::wallet::Transaction>) -> Self {
        let txs = txs
            .into_iter()
            .map(From::from)
            .collect::<Vec<Transaction>>();

        let mut balance = 0_i64;
        let mut untrusted_pending_balance = 0_i64;

        for tx in &txs {
            let filter = Some(&tx.is_mine);
            let net_spent: i64 = tx
                ._sum_inputs(filter)
                .unwrap_or(0)
                .try_into()
                .expect("spent value must fit into i64");
            let net_created: i64 = tx
                ._sum_outputs(filter)
                .try_into()
                .expect("created value must fit into i64");
            if net_spent == 0 && tx.confirmation_time.is_none() {
                untrusted_pending_balance += net_created;
            } else {
                balance += net_created;
                balance -= net_spent;
            }
        }

        // Workaround as we are too lazy to exclude spends from unconfirmed as
        // `untrusted_pending_balance`.
        if balance < 0 {
            untrusted_pending_balance += balance;
            balance = 0;
        }

        Self {
            balance,
            untrusted_pending_balance,
            txs,
        }
    }
}

impl From<frostsnap_coordinator::bitcoin::wallet::Transaction> for Transaction {
    fn from(value: frostsnap_coordinator::bitcoin::wallet::Transaction) -> Self {
        Self {
            inner: (value.inner).deref().clone(),
            txid: value.txid.to_string(),
            confirmation_time: value.confirmation_time,
            last_seen: value.last_seen,
            prevouts: value.prevouts,
            is_mine: value.is_mine,
        }
    }
}

#[frb(mirror(Address), opaque)]
pub struct _Address {}

pub trait AddressExt {
    #[frb(sync)]
    fn spk(&self) -> ScriptBuf;

    #[frb(sync, type_64bit_int)]
    fn bip21_uri(&self, amount: Option<u64>, label: Option<String>) -> String;

    #[frb(sync)]
    fn from_string(s: &str, network: &BitcoinNetwork) -> Option<Address>;
}

#[frb(external)]
impl Address {
    #[frb(sync)]
    pub fn to_string(&self) -> String {}
}

impl AddressExt for bitcoin::Address {
    #[frb(sync)]
    fn spk(&self) -> ScriptBuf {
        self.script_pubkey()
    }

    #[frb(sync, type_64bit_int)]
    fn bip21_uri(&self, amount: Option<u64>, label: Option<String>) -> String {
        let mut uri = bip21::Uri::new(self.clone());

        if let Some(sats) = amount {
            uri.amount = Some(bitcoin::Amount::from_sat(sats));
        }

        if let Some(label_str) = label {
            uri.label = Some(label_str.into());
        }

        uri.to_string()
    }

    #[frb(sync)]
    fn from_string(s: &str, network: &BitcoinNetwork) -> Option<Self> {
        Address::from_str(s).ok()?.require_network(*network).ok()
    }
}

#[frb(external)]
impl Psbt {
    #[frb(sync)]
    pub fn serialize(&self) -> Vec<u8> {}

    #[frb(sync)]
    #[allow(unused)]
    pub fn deserialize(bytes: &[u8]) -> Result<Psbt, PsbtError> {}
}

#[frb(sync)]
pub fn compute_txid_of_psbt(psbt: &Psbt) -> Txid {
    psbt.unsigned_tx.compute_txid()
}

#[frb(sync)]
pub fn txid_hex_string(txid: &Txid) -> String {
    txid.to_string()
}
