use anyhow::Result;
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
use frostsnap_core::bitcoin_transaction::{ScopedTo, TransactionTemplate};
use frostsnap_core::message::EncodedSignature;
use tracing::{event, Level};

use std::collections::HashMap;
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
    pub is_mine: HashMap<bitcoin::ScriptBuf, u32>,
    /// Fee over the size this transaction has *signed*, in sats/vB.
    ///
    /// Supplied by whichever constructor knows it rather than derived here, because the two
    /// provenances know it differently and neither can be recovered from these fields alone.
    /// `None` means it could not be determined: an observed transaction may be missing a
    /// prevout, so the fee is unknown; one built from a template may hold an input we cannot
    /// witness, so the signed size is.
    pub feerate: Option<f64>,
}

impl Transaction {
    pub(crate) fn from_template(tx_temp: &TransactionTemplate<ScopedTo>) -> Self {
        let raw_tx = tx_temp.to_rust_bitcoin_tx();
        let txid = tx_temp.txid();
        let is_mine = tx_temp
            .our_spks()
            .into_iter()
            .map(|(spk, path)| (spk, path.index.to_u32()))
            .collect::<HashMap<_, _>>();
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
            // `raw_tx` has no witnesses yet, so its own size is not the size this will be
            // broadcast at. The template knows: every input it can sign is a taproot keyspend
            // witnessed by exactly 64 bytes.
            feerate: tx_temp.feerate(),
        }
    }

    #[frb(sync)]
    pub fn raw_txid(&self) -> Txid {
        self.inner.compute_txid()
    }

    /// The same view, witnessed with the signatures that signed it.
    ///
    /// Witnessing happens here, where the template is, rather than on a built `Transaction`:
    /// `signatures_by_input_index` decides which signature belongs to which input, and a
    /// `Transaction` no longer carries a template that could answer that.
    pub(crate) fn signed_from_template(
        tx_temp: &TransactionTemplate<ScopedTo>,
        signatures: &[EncodedSignature],
    ) -> Result<Self> {
        let inner = tx_temp.to_signed_rust_bitcoin_tx(signatures)?;
        Ok(Self {
            inner,
            ..Self::from_template(tx_temp)
        })
    }

    /// The fee, from prevouts and a transaction that need not be built yet.
    ///
    /// Free-standing so a constructor can compute the feerate before there is a `Self` to ask.
    /// `None` when any input's prevout is absent, which is what makes the fee unknowable.
    fn fee_from(
        prevouts: &HashMap<bitcoin::OutPoint, bitcoin::TxOut>,
        tx: &RTransaction,
    ) -> Option<u64> {
        let inputs: u64 = tx
            .input
            .iter()
            .map(|txin| {
                prevouts
                    .get(&txin.previous_output)
                    .map(|o| o.value.to_sat())
            })
            .sum::<Option<u64>>()?;
        Some(inputs.saturating_sub(tx.output.iter().map(|o| o.value.to_sat()).sum()))
    }

    /// Computes the sum of all inputs, or only those whose previous output script pubkey is in
    /// `filter`, if provided. The result is `None` if any input is missing a previous output.
    fn _sum_inputs(&self, filter: Option<&HashMap<bitcoin::ScriptBuf, u32>>) -> Option<u64> {
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
                        Some(filter) => filter.contains_key(prevout.script_pubkey.as_script()),
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
    fn _sum_outputs(&self, filter: Option<&HashMap<bitcoin::ScriptBuf, u32>>) -> u64 {
        self.inner
            .output
            .iter()
            .filter(|txout| {
                match &filter {
                    Some(filter) => filter.contains_key(txout.script_pubkey.as_script()),
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
        let filter = HashMap::from([(spk.as_script().to_owned(), 0)]);
        self._sum_inputs(Some(&filter))
    }

    /// Computes the total value of outputs that send to the given script pubkey.
    #[frb(sync, type_64bit_int)]
    pub fn sum_outputs_to_spk(&self, spk: &bitcoin::ScriptBuf) -> u64 {
        let filter = HashMap::from([(spk.as_script().to_owned(), 0)]);
        self._sum_outputs(Some(&filter))
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
    #[frb(sync)]
    pub fn recipients(&self) -> Vec<TxOutInfo> {
        self.inner
            .output
            .iter()
            .zip(0_u32..)
            .map(|(txout, vout)| {
                let derivation_index = self.is_mine.get(&txout.script_pubkey).copied();
                TxOutInfo {
                    vout,
                    amount: txout.value.to_sat(),
                    script_pubkey: txout.script_pubkey.clone(),
                    is_mine: derivation_index.is_some(),
                    derivation_index,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
#[frb(type_64bit_int)]
pub struct TxOutInfo {
    pub vout: u32,
    pub amount: u64,
    pub script_pubkey: bitcoin::ScriptBuf,
    pub is_mine: bool,
    pub derivation_index: Option<u32>,
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
        let inner: RTransaction = (value.inner).deref().clone();
        // In the canonical graph means broadcast or seen on chain, so `inner` carries real
        // witnesses and its own size is the signed size.
        let feerate =
            Self::fee_from(&value.prevouts, &inner).map(|fee| fee as f64 / inner.vsize() as f64);
        Self {
            inner,
            txid: value.txid.to_string(),
            confirmation_time: value.confirmation_time,
            last_seen: value.last_seen,
            prevouts: value.prevouts,
            is_mine: value.is_mine,
            feerate,
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

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::{hashes::Hash, Amount, OutPoint, ScriptBuf, TxOut, Txid};
    use frostsnap_core::{
        bitcoin_transaction::{LocalSpk, PushInput},
        schnorr_fun::fun::{g, G},
        tweak::{BitcoinBip32Path, NormalIndex},
    };

    fn idx(n: u32) -> NormalIndex {
        NormalIndex::new(n).expect("test index is in range")
    }

    fn key() -> MasterAppkey {
        MasterAppkey::derive_from_rootkey(g!(2 * G).normalize())
    }

    fn signature(tag: u8) -> EncodedSignature {
        let mut bytes = [1u8; 64];
        bytes[0] = tag;
        EncodedSignature(bytes)
    }

    fn outpoint(vout: u32) -> OutPoint {
        OutPoint {
            txid: Txid::from_byte_array([9u8; 32]),
            vout,
        }
    }

    /// Input 0 is foreign, input 1 is ours.
    /// One input, ours — so the signed size is fully knowable and the rate is `Some`.
    fn single_owned_input_template() -> TransactionTemplate {
        let key = key();
        let path = BitcoinBip32Path::external(idx(0));
        let owner = LocalSpk {
            master_appkey: key,
            bip32_path: path,
        };
        let mut template = TransactionTemplate::new();
        let ours = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: owner.spk(),
        };
        template
            .push_owned_input(PushInput::spend_outpoint(&ours, outpoint(0)), owner)
            .unwrap();
        template.push_foreign_output(TxOut {
            value: Amount::from_sat(90_000),
            script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xcc; 32][..]].concat()),
        });
        template
    }

    /// Input 0 is foreign, input 1 is ours — so we cannot weigh a witness for every input and
    /// the rate is `None`.
    fn template_with_a_foreign_input_first() -> TransactionTemplate {
        let key = key();
        let path = BitcoinBip32Path::external(idx(0));
        let mut template = TransactionTemplate::new();

        let foreign = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xaa; 32][..]].concat()),
        };
        template.push_foreign_input(PushInput::spend_outpoint(&foreign, outpoint(0)));

        let ours = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: LocalSpk {
                master_appkey: key,
                bip32_path: path,
            }
            .spk(),
        };
        template
            .push_owned_input(
                PushInput::spend_outpoint(&ours, outpoint(1)),
                LocalSpk {
                    master_appkey: key,
                    bip32_path: path,
                },
            )
            .unwrap();
        template.push_foreign_output(TxOut {
            value: Amount::from_sat(150_000),
            script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xbb; 32][..]].concat()),
        });
        template
    }

    /// Witnessing at construction must place signatures the same way the template does —
    /// placing them here instead is how four copies of that loop drifted apart.
    ///
    /// There is no longer a test that a wallet-read transaction refuses to be signed: it has
    /// no `with_signatures` to call, so the compiler states it and a runtime assertion would
    /// only be able to restate it more weakly.
    #[test]
    fn signing_at_construction_agrees_with_the_template_it_delegates_to() {
        let template = template_with_a_foreign_input_first();
        let scoped = template.as_seen_by(key());

        for signatures in [vec![], vec![signature(7)], vec![signature(1), signature(2)]] {
            assert_eq!(
                Transaction::signed_from_template(&scoped, &signatures)
                    .ok()
                    .map(|tx| tx.inner),
                scoped.to_signed_rust_bitcoin_tx(&signatures).ok(),
                "construction must not have its own idea of where a signature goes"
            );
        }
    }

    /// A PSBT can leave us inputs we do not own. Signing ours still leaves those witnessless,
    /// because the template has nowhere to put anyone else's signature, so the result is a
    /// transaction the network rejects. Offering to broadcast it is the failure this guards.
    #[test]
    fn a_transaction_with_an_input_that_is_not_ours_cannot_be_broadcast_by_us() {
        let ours_only = single_owned_input_template().as_seen_by(key());
        let partly_theirs = template_with_a_foreign_input_first().as_seen_by(key());

        assert!(ours_only.owns_every_input());
        assert!(!partly_theirs.owns_every_input());

        assert!(
            partly_theirs.has_any_inputs_to_sign(),
            "we can still contribute — which is why `has_any_inputs_to_sign` cannot be the \
             guard: it answers a different question"
        );

        let signed = Transaction::signed_from_template(&partly_theirs, &[signature(7)]).unwrap();
        assert!(
            signed.inner.input[0].witness.is_empty(),
            "and this is what would be broadcast: an input nobody has signed"
        );
    }

    /// The bug this whole line of work started from: `with_signatures` used to `zip` the
    /// signature list against every input, so a foreign input at position 0 took the signature
    /// produced for our input at position 1 — and the result was broadcastable-looking and
    /// invalid. Signatures come back per *owned* input, in owned order, so only the template
    /// can say where each belongs.
    #[test]
    fn a_signature_lands_on_our_input_and_not_the_foreign_one() {
        let scoped = template_with_a_foreign_input_first().as_seen_by(key());
        assert!(
            scoped.inputs()[0].owner().local_owner().is_none(),
            "input 0 is the foreign one: the ordering that made zip wrong"
        );

        let tx = Transaction::signed_from_template(&scoped, &[signature(7)]).unwrap();

        assert!(
            tx.inner.input[0].witness.is_empty(),
            "the foreign input keeps no witness of ours"
        );
        assert!(
            !tx.inner.input[1].witness.is_empty(),
            "our input is the one that gets signed"
        );
    }

    /// A template-built transaction has no witnesses yet, so its own size is not the size it
    /// will be broadcast at. Deriving the rate from it inflated the number on precisely the
    /// screen a user reads while deciding whether to sign.
    #[test]
    fn feerate_is_over_the_signed_size_not_the_unsigned_one() {
        let scoped = single_owned_input_template().as_seen_by(key());
        let tx = Transaction::from_template(&scoped);

        let unsigned_rate = tx.fee().unwrap() as f64 / tx.inner.vsize() as f64;

        assert_eq!(tx.feerate, scoped.feerate());
        assert!(
            tx.feerate.unwrap() < unsigned_rate,
            "witnessing can only make it bigger, so the honest rate is below the empty-witness \
             one ({:?} vs {unsigned_rate})",
            tx.feerate
        );
    }

    /// The two constructors are one rule, not two answers: a Schnorr signature is exactly the
    /// 64 bytes the template weighs with, so witnessing cannot move the rate.
    #[test]
    fn witnessing_does_not_change_the_feerate() {
        let scoped = single_owned_input_template().as_seen_by(key());
        let unsigned = Transaction::from_template(&scoped);
        let signed = Transaction::signed_from_template(&scoped, &[signature(1)]).unwrap();

        assert_eq!(signed.feerate, unsigned.feerate);
        assert_eq!(
            signed.feerate.unwrap(),
            signed.fee().unwrap() as f64 / signed.inner.vsize() as f64,
            "now that it is witnessed, its own size agrees with the template's"
        );
    }

    /// The old `details()` built this from the wallet index, which only knows scripts it has
    /// already derived — so an owned output past the lookahead was shown as a stranger's.
    #[test]
    fn recipients_marks_an_owned_output_past_any_lookahead_as_ours() {
        let key = key();
        let deep = BitcoinBip32Path::internal(idx(400_000));
        let mut template = template_with_a_foreign_input_first();
        template
            .push_owned_output_checked(
                &TxOut {
                    value: Amount::from_sat(40_000),
                    script_pubkey: LocalSpk {
                        master_appkey: key,
                        bip32_path: deep,
                    }
                    .spk(),
                },
                LocalSpk {
                    master_appkey: key,
                    bip32_path: deep,
                },
            )
            .unwrap();

        let tx = Transaction::from_template(&template.as_seen_by(key));
        let recipients = tx.recipients();

        let ours = recipients.last().unwrap();
        assert!(ours.is_mine, "an owned output is ours at any depth");
        assert_eq!(ours.derivation_index, Some(400_000));
        assert!(!recipients[0].is_mine, "the foreign output is not ours");
    }

    /// The old `details()` looked prevouts up in the wallet's graph, which does not contain a
    /// PSBT's foreign inputs. `_sum_inputs` returns `None` if any is missing, so fee and
    /// feerate disappeared from the review screen for exactly those transactions.
    #[test]
    fn fee_survives_an_input_the_wallet_has_never_seen() {
        let tx =
            Transaction::from_template(&template_with_a_foreign_input_first().as_seen_by(key()));

        assert_eq!(tx.prevouts.len(), 2, "both inputs, foreign one included");
        assert_eq!(tx.fee(), Some(50_000));
        assert_eq!(
            tx.feerate, None,
            "the fee is knowable but the signed size is not: we cannot weigh a witness for an \
             input that is not ours. `is_some()` here is what let an inflated rate stand."
        );
    }
}
