use alloc::vec::Vec;
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
};
use bitcoin::{
    consensus::Encodable,
    hashes::{sha256d, Hash},
    key::TweakedPublicKey,
    sighash::SighashCache,
    OutPoint, Script, ScriptBuf, TapSighash, TxOut, Txid,
};
use schnorr_fun::fun::hash::Tag;
use sha2::{Digest, Sha256};

use crate::{
    silent_payments::SilentPaymentRecipient,
    tweak::{AppTweak, BitcoinBip32Path},
    MasterAppkey,
};

const OUTPUT_IS_ADDRESS: u8 = 0x00;
const OUTPUT_IS_SCRIPT_HEX: u8 = 0x01;

/// The first 16 bytes of an outputs fingerprint as eight groups of hex, e.g.
/// `ab12 cd34 ef56 7890 ...`, for the user to compare between the app and the device.
///
/// This is a convenience, not a security guarantee. An app that builds both the outputs it
/// shows and the outputs it signs controls free details (change index, change amount, order),
/// so with a truncated code it only needs a collision. The device's own review of the outputs
/// is the security boundary.
pub fn fingerprint_code(fp: &[u8; 32]) -> String {
    fp[..16]
        .chunks(2)
        .map(|c| alloc::format!("{:02x}{:02x}", c[0], c[1]))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Invalid state free representation of a transaction
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Eq, PartialEq, Hash)]
pub struct TransactionTemplate {
    #[bincode(with_serde)]
    version: bitcoin::blockdata::transaction::Version,
    #[bincode(with_serde)]
    lock_time: bitcoin::absolute::LockTime,
    inputs: Vec<Input>,
    pub(crate) outputs: Vec<Output>,
}

pub struct PushInput<'a> {
    pub prev_txout: PrevTxOut<'a>,
    pub sequence: bitcoin::Sequence,
}

impl<'a> PushInput<'a> {
    pub fn spend_tx_output(transaction: &'a bitcoin::Transaction, vout: u32) -> Self {
        Self {
            prev_txout: PrevTxOut::Full { transaction, vout },
            sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
        }
    }

    pub fn spend_outpoint(txout: &'a TxOut, outpoint: OutPoint) -> Self {
        Self {
            prev_txout: PrevTxOut::Partial { txout, outpoint },
            sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
        }
    }

    pub fn with_sequence(mut self, sequence: bitcoin::Sequence) -> Self {
        self.sequence = sequence;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrevTxOut<'a> {
    Full {
        transaction: &'a bitcoin::Transaction,
        vout: u32,
    },
    Partial {
        txout: &'a TxOut,
        outpoint: OutPoint,
    },
}

impl<'a> PrevTxOut<'a> {
    pub fn txout(&self) -> &'a TxOut {
        match self {
            PrevTxOut::Full { transaction, vout } => &transaction.output[*vout as usize],
            PrevTxOut::Partial { txout, .. } => txout,
        }
    }

    pub fn outpoint(&self) -> OutPoint {
        match self {
            PrevTxOut::Full { transaction, vout } => OutPoint {
                txid: transaction.compute_txid(),
                vout: *vout,
            },
            PrevTxOut::Partial { outpoint, .. } => *outpoint,
        }
    }
}

impl Default for TransactionTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionTemplate {
    pub fn new() -> Self {
        Self {
            version: bitcoin::blockdata::transaction::Version::TWO,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            inputs: Default::default(),
            outputs: Default::default(),
        }
    }

    pub fn set_version(&mut self, version: bitcoin::blockdata::transaction::Version) {
        self.version = version;
    }

    pub fn set_lock_time(&mut self, lock_time: bitcoin::absolute::LockTime) {
        self.lock_time = lock_time;
    }

    pub fn txid(&self) -> Txid {
        self.to_rust_bitcoin_tx().compute_txid()
    }

    pub fn push_foreign_input(&mut self, input: PushInput) {
        let txout = input.prev_txout.txout();

        self.inputs.push(Input {
            outpoint: input.prev_txout.outpoint(),
            owner: SpkOwner::Foreign(txout.script_pubkey.clone()),
            value: txout.value.to_sat(),
            sequence: input.sequence,
        })
    }
    pub fn push_owned_input(
        &mut self,
        input: PushInput<'_>,
        owner: LocalSpk,
    ) -> Result<(), Box<SpkDoesntMatchPathError>> {
        let txout = input.prev_txout.txout();
        let expected_spk = owner.spk();

        if txout.script_pubkey != expected_spk {
            return Err(Box::new(SpkDoesntMatchPathError {
                got: txout.script_pubkey.clone(),
                expected: expected_spk,
                path: owner
                    .bip32_path
                    .path_segments_from_bitcoin_appkey()
                    .collect(),
                master_appkey: owner.master_appkey,
            }));
        }

        self.inputs.push(Input {
            outpoint: input.prev_txout.outpoint(),
            owner: SpkOwner::Local(owner),
            value: txout.value.to_sat(),
            sequence: input.sequence,
        });
        Ok(())
    }

    pub fn push_imaginary_owned_input(&mut self, owner: LocalSpk, value: bitcoin::Amount) {
        let txout = TxOut {
            value,
            script_pubkey: owner.spk(),
        };
        let mut engine = sha256d::Hash::engine();
        txout.consensus_encode(&mut engine).unwrap();
        let txid = Txid::from_engine(engine);
        let outpoint = OutPoint { txid, vout: 0 };
        self.push_owned_input(PushInput::spend_outpoint(&txout, outpoint), owner)
            .expect("unreachable");
    }

    pub fn push_foreign_output(&mut self, txout: TxOut) {
        self.outputs.push(Output {
            owner: SpkOwner::Foreign(txout.script_pubkey),
            value: txout.value.to_sat(),
        });
    }

    pub fn push_owned_output(&mut self, value: bitcoin::Amount, owner: LocalSpk) {
        self.outputs.push(Output {
            owner: SpkOwner::Local(owner),
            value: value.to_sat(),
        });
    }

    pub fn to_rust_bitcoin_tx(&self) -> bitcoin::Transaction {
        bitcoin::Transaction {
            version: self.version,
            lock_time: self.lock_time,
            input: self
                .inputs
                .iter()
                .map(|input| bitcoin::TxIn {
                    previous_output: input.outpoint,
                    sequence: input.sequence,
                    ..Default::default()
                })
                .collect(),
            output: self.outputs.iter().map(|output| output.txout()).collect(),
        }
    }

    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    pub fn iter_sighash(&self) -> impl Iterator<Item = TapSighash> {
        let tx = self.to_rust_bitcoin_tx();
        let mut sighash_cache = SighashCache::new(tx);
        let schnorr_sighashty = bitcoin::sighash::TapSighashType::Default;
        let prevouts = self.inputs.iter().map(Input::txout).collect::<Vec<_>>();
        (0..self.inputs.len()).map(move |i| {
            sighash_cache
                .taproot_key_spend_signature_hash(
                    i,
                    &bitcoin::sighash::Prevouts::All(&prevouts),
                    schnorr_sighashty,
                )
                .expect("inputs are right length")
        })
    }

    pub fn iter_sighashes_of_locally_owned_inputs(
        &self,
    ) -> impl Iterator<Item = (LocalSpk, TapSighash)> + '_ {
        self.inputs
            .iter()
            .zip(self.iter_sighash())
            .filter_map(|(input, sighash)| {
                let owner = input.owner.local_owner()?.clone();
                Some((owner, sighash))
            })
    }

    pub fn iter_locally_owned_inputs(&self) -> impl Iterator<Item = (usize, &Input, &LocalSpk)> {
        self.inputs
            .iter()
            .enumerate()
            .filter_map(|(i, input)| Some((i, input, input.owner.local_owner()?)))
    }

    pub fn iter_locally_owned_outputs(&self) -> impl Iterator<Item = (usize, &Output, &LocalSpk)> {
        self.outputs
            .iter()
            .enumerate()
            .filter_map(|(i, output)| Some((i, output, output.owner.local_owner()?)))
    }

    /// Returns true if this transaction has any inputs that need signing by this wallet.
    pub fn has_any_inputs_to_sign(&self) -> bool {
        self.inputs
            .iter()
            .any(|input| input.owner.local_owner().is_some())
    }

    pub fn fee(&self) -> Option<u64> {
        self.inputs
            .iter()
            .map(|input| input.value)
            .sum::<u64>()
            .checked_sub(self.outputs.iter().map(|output| output.value).sum())
    }

    pub fn feerate(&self) -> Option<f64> {
        let mut tx = self.to_rust_bitcoin_tx();

        for (i, input) in self.inputs.iter().enumerate() {
            if input.owner().local_owner().is_some() {
                tx.input[i].witness.push([0u8; 64]);
            } else {
                return None;
            }
        }

        let vbytes = tx.weight().to_vbytes_ceil() as f64;
        Some(self.fee()? as f64 / vbytes)
    }

    pub fn net_value(&self) -> BTreeMap<RootOwner, i64> {
        let mut spk_to_value: BTreeMap<RootOwner, i64> = Default::default();

        for input in &self.inputs {
            let value = spk_to_value.entry(input.owner.root_owner()).or_default();
            *value -= i64::try_from(input.value).expect("input ridiciously large");
        }

        for output in &self.outputs {
            let value = spk_to_value.entry(output.owner.root_owner()).or_default();
            *value += i64::try_from(output.value).expect("input ridiciously large");
        }

        spk_to_value
    }

    pub fn foreign_recipients(&self) -> impl Iterator<Item = (&Script, u64)> {
        self.outputs
            .iter()
            .filter_map(|output| match &output.owner {
                SpkOwner::Foreign(spk) => Some((spk.as_script(), output.value)),
                _ => None,
            })
    }

    /// A hash of every output's index, value and address on `network`, in order. A silent
    /// payment output contributes the address it was given, so the fingerprint stays the same
    /// once its script is filled in.
    pub fn outputs_fingerprint(&self, network: bitcoin::Network) -> [u8; 32] {
        let mut hash = Sha256::default().tag(b"frostsnap/outputs/v2");
        for (i, output) in self.outputs.iter().enumerate() {
            let (form, text) = match &output.owner {
                SpkOwner::SilentPayment(sp) => (OUTPUT_IS_ADDRESS, sp.address.clone()),
                SpkOwner::Foreign(spk) => address_or_script_hex(spk, network),
                SpkOwner::Local(local) => address_or_script_hex(&local.spk(), network),
            };
            hash.update((i as u32).to_le_bytes());
            hash.update(output.value.to_le_bytes());
            hash.update([form]);
            hash.update((text.len() as u64).to_le_bytes());
            hash.update(text.as_bytes());
        }
        hash.finalize().into()
    }

    pub fn user_prompt(&self, network: bitcoin::Network) -> PromptSignBitcoinTx {
        let fee = bitcoin::Amount::from_sat(
            self.fee()
                .expect("transaction validity should have already been checked"),
        );
        let foreign_recipients = self
            .foreign_recipients()
            .map(|(spk, value)| {
                (
                    bitcoin::Address::from_script(spk, network)
                        .expect("has address representation"),
                    bitcoin::Amount::from_sat(value),
                )
            })
            .collect::<Vec<_>>();

        // Calculate fee rate in sats/vB
        let fee_rate_sats_per_vbyte = self.feerate();

        let silent_payment_recipients = self
            .outputs
            .iter()
            .filter_map(|output| match &output.owner {
                SpkOwner::SilentPayment(sp) => {
                    Some((sp.address.clone(), bitcoin::Amount::from_sat(output.value)))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        PromptSignBitcoinTx {
            foreign_recipients,
            silent_payment_recipients,
            fee,
            fee_rate_sats_per_vbyte,
            outputs_fingerprint: self.outputs_fingerprint(network),
        }
    }
}

/// An address for `spk` on `network`, or the hex of the script when it has no address form.
fn address_or_script_hex(spk: &Script, network: bitcoin::Network) -> (u8, String) {
    match bitcoin::Address::from_script(spk, network) {
        Ok(address) => (OUTPUT_IS_ADDRESS, address.to_string()),
        Err(_) => (OUTPUT_IS_SCRIPT_HEX, alloc::format!("{spk:x}")),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptSignBitcoinTx {
    pub foreign_recipients: Vec<(bitcoin::Address, bitcoin::Amount)>,
    /// Silent payment recipients, as the address the recipient gave. The device has checked it
    /// against the keys the output is paid to.
    pub silent_payment_recipients: Vec<(String, bitcoin::Amount)>,
    pub fee: bitcoin::Amount,
    /// Fee rate in sats/vB
    pub fee_rate_sats_per_vbyte: Option<f64>,
    pub outputs_fingerprint: [u8; 32],
}

impl PromptSignBitcoinTx {
    pub fn fingerprint_code(&self) -> String {
        fingerprint_code(&self.outputs_fingerprint)
    }

    /// Calculate the total amount being sent to foreign and silent payment recipients
    pub fn total_sent(&self) -> bitcoin::Amount {
        self.foreign_recipients
            .iter()
            .map(|(_, amount)| *amount)
            .chain(
                self.silent_payment_recipients
                    .iter()
                    .map(|(_, amount)| *amount),
            )
            .sum()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum RootOwner {
    Local(MasterAppkey),
    Foreign(ScriptBuf),
}

/// The provided spk doesn't match what was derived from the derivation path
#[derive(Debug, Clone)]
pub struct SpkDoesntMatchPathError {
    pub got: ScriptBuf,
    pub expected: ScriptBuf,
    pub path: Vec<u32>,
    pub master_appkey: MasterAppkey,
}

impl core::fmt::Display for SpkDoesntMatchPathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "the script pubkey {:?} didn't match what we expected {:?} at derivation path {:?} from {}", self.got, self.expected, self.path, self.master_appkey)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SpkDoesntMatchPathError {}

#[derive(bincode::Decode, bincode::Encode, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Input {
    #[bincode(with_serde)]
    outpoint: OutPoint,
    value: u64,
    owner: SpkOwner,
    #[bincode(with_serde)]
    sequence: bitcoin::Sequence,
}

impl Input {
    pub fn outpoint(&self) -> OutPoint {
        self.outpoint
    }
    pub fn txout(&self) -> TxOut {
        TxOut {
            value: bitcoin::Amount::from_sat(self.value),
            script_pubkey: self.owner.spk(),
        }
    }

    pub fn raw_spk(&self) -> ScriptBuf {
        self.owner.spk()
    }

    pub fn owner(&self) -> &SpkOwner {
        &self.owner
    }
}

#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocalSpk {
    pub master_appkey: MasterAppkey,
    pub bip32_path: BitcoinBip32Path,
}

impl LocalSpk {
    pub fn spk(&self) -> ScriptBuf {
        let expected_external_xonly =
            AppTweak::Bitcoin(self.bip32_path).derive_xonly_key(&self.master_appkey.to_xpub());
        ScriptBuf::new_p2tr_tweaked(TweakedPublicKey::dangerous_assume_tweaked(
            expected_external_xonly.into(),
        ))
    }
}

#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Output {
    pub value: u64,
    pub owner: SpkOwner,
}

impl Output {
    pub fn txout(&self) -> TxOut {
        TxOut {
            value: bitcoin::Amount::from_sat(self.value),
            script_pubkey: self.owner.spk(),
        }
    }

    pub fn local_owner(&self) -> Option<&LocalSpk> {
        self.owner.local_owner()
    }

    pub fn owner(&self) -> &SpkOwner {
        &self.owner
    }
}

#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SpkOwner {
    Foreign(#[bincode(with_serde)] ScriptBuf),
    Local(LocalSpk),
    SilentPayment(Box<SilentPaymentOutput>),
}

/// A silent payment output. Its script depends on the ECDH shares of the signing devices, so it
/// is `None` until they are combined. A send with any output still missing its script cannot be
/// signed.
#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SilentPaymentOutput {
    pub recipient: SilentPaymentRecipient,
    /// The address the recipient gave, for display. Must encode `recipient` on the signing network.
    pub address: String,
    #[bincode(with_serde)]
    pub script: Option<ScriptBuf>,
}

impl SpkOwner {
    pub fn root_owner(&self) -> RootOwner {
        match self {
            SpkOwner::Foreign(spk) => RootOwner::Foreign(spk.clone()),
            SpkOwner::Local(local) => RootOwner::Local(local.master_appkey),
            SpkOwner::SilentPayment(_) => RootOwner::Foreign(self.spk()),
        }
    }
    pub fn spk(&self) -> ScriptBuf {
        match self {
            SpkOwner::Foreign(spk) => spk.clone(),
            SpkOwner::Local(owner) => owner.spk(),
            // SignTask::check refuses a send whose silent payment scripts are missing.
            SpkOwner::SilentPayment(sp) => sp.script.clone().unwrap_or_default(),
        }
    }

    pub fn local_owner_key(&self) -> Option<MasterAppkey> {
        match self {
            SpkOwner::Foreign(_) | SpkOwner::SilentPayment(_) => None,
            SpkOwner::Local(owner) => Some(owner.master_appkey),
        }
    }

    pub fn local_owner(&self) -> Option<&LocalSpk> {
        match self {
            SpkOwner::Foreign(_) | SpkOwner::SilentPayment(_) => None,
            SpkOwner::Local(owner) => Some(owner),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bitcoin::{Address, Amount, Network};
    use core::str::FromStr;
    use schnorr_fun::fun::{marker::*, Scalar, G};

    fn dummy_sp_recipient(scan: u8, spend: u8) -> (SilentPaymentRecipient, String) {
        let scan_s = Scalar::<Secret, Zero>::from_bytes_mod_order([scan; 32])
            .non_zero()
            .unwrap();
        let spend_s = Scalar::<Secret, Zero>::from_bytes_mod_order([spend; 32])
            .non_zero()
            .unwrap();
        let recipient = SilentPaymentRecipient {
            scan_key: schnorr_fun::fun::g!(scan_s * G)
                .normalize()
                .non_zero()
                .unwrap(),
            spend_key: schnorr_fun::fun::g!(spend_s * G)
                .normalize()
                .non_zero()
                .unwrap(),
        };
        let addr = recipient.address(Network::Bitcoin).unwrap();
        (recipient, addr)
    }

    fn mainnet_address(s: &str) -> Address {
        Address::from_str(s)
            .unwrap()
            .require_network(Network::Bitcoin)
            .unwrap()
    }

    #[test]
    fn outputs_fingerprint_fixed_vector() {
        let mut template = TransactionTemplate::new();
        template.push_foreign_output(TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: mainnet_address("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                .script_pubkey(),
        });
        template.push_foreign_output(TxOut {
            value: Amount::from_sat(250_000),
            script_pubkey: mainnet_address(
                "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3",
            )
            .script_pubkey(),
        });

        let fp = template.outputs_fingerprint(Network::Bitcoin);
        let code = fingerprint_code(&fp);
        // Eight groups of four hex characters, separated by spaces.
        assert_eq!(code.len(), 39);
        assert_eq!(code.split(' ').count(), 8);
        assert!(code.split(' ').all(|g| g.len() == 4));

        assert_eq!(code, "1a50 e39a 473d 614f ffd1 dade fd42 85bb");
    }

    #[test]
    fn outputs_fingerprint_stable_across_with_sp_scripts() {
        let mut template = TransactionTemplate::new();
        template.push_foreign_output(TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: mainnet_address("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")
                .script_pubkey(),
        });
        let (recipient, sp_addr) = dummy_sp_recipient(1, 2);
        template.push_silent_payment_output(Amount::from_sat(50_000), recipient, sp_addr);

        let fp_before = template.outputs_fingerprint(Network::Bitcoin);

        let mut shared_secrets = BTreeMap::new();
        shared_secrets.insert(recipient.scan_key, G.normalize());
        let template_with_sp = template.with_sp_scripts(&shared_secrets).unwrap();

        assert_eq!(
            fp_before,
            template_with_sp.outputs_fingerprint(Network::Bitcoin)
        );
    }

    #[test]
    fn outputs_fingerprint_changes_on_amount_or_address_or_order() {
        let addr1 = mainnet_address("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4");
        let addr2 =
            mainnet_address("bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3");

        let make_template = |val1: u64, addr1: &Address, val2: u64, addr2: &Address| {
            let mut template = TransactionTemplate::new();
            template.push_foreign_output(TxOut {
                value: Amount::from_sat(val1),
                script_pubkey: addr1.script_pubkey(),
            });
            template.push_foreign_output(TxOut {
                value: Amount::from_sat(val2),
                script_pubkey: addr2.script_pubkey(),
            });
            template
        };

        let base_fp =
            make_template(100_000, &addr1, 250_000, &addr2).outputs_fingerprint(Network::Bitcoin);

        let changed = [
            make_template(100_001, &addr1, 250_000, &addr2),
            make_template(100_000, &addr1, 250_001, &addr2),
            make_template(100_000, &addr2, 250_000, &addr2),
            make_template(250_000, &addr2, 100_000, &addr1),
        ];
        for template in changed {
            assert_ne!(base_fp, template.outputs_fingerprint(Network::Bitcoin));
        }

        let (rec, sp_addr) = dummy_sp_recipient(1, 2);
        let mut sp_base = TransactionTemplate::new();
        sp_base.push_silent_payment_output(Amount::from_sat(100_000), rec, sp_addr.clone());
        let sp_base_fp = sp_base.outputs_fingerprint(Network::Bitcoin);

        let mut sp_changed_amt = TransactionTemplate::new();
        sp_changed_amt.push_silent_payment_output(Amount::from_sat(100_001), rec, sp_addr);
        assert_ne!(
            sp_base_fp,
            sp_changed_amt.outputs_fingerprint(Network::Bitcoin)
        );

        let (rec2, sp_addr2) = dummy_sp_recipient(3, 4);
        let mut sp_changed_addr = TransactionTemplate::new();
        sp_changed_addr.push_silent_payment_output(Amount::from_sat(100_000), rec2, sp_addr2);
        assert_ne!(
            sp_base_fp,
            sp_changed_addr.outputs_fingerprint(Network::Bitcoin)
        );
    }
}
