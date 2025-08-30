use alloc::vec::Vec;
use alloc::{boxed::Box, collections::BTreeMap};
use bitcoin::{
    consensus::Encodable,
    hashes::{sha256d, Hash},
    key::TweakedPublicKey,
    sighash::SighashCache,
    OutPoint, Script, ScriptBuf, TapSighash, TxOut, Txid,
};

use crate::{
    tweak::{AppTweak, BitcoinBip32Path},
    MasterAppkey,
};

/// Invalid state free representation of a transaction
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Eq, PartialEq, Hash)]
pub struct TransactionTemplate {
    #[bincode(with_serde)]
    version: bitcoin::blockdata::transaction::Version,
    #[bincode(with_serde)]
    lock_time: bitcoin::absolute::LockTime,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
}

pub const TR_KEYSPEND_SATISFACTION_WEIGHT: u32 = 1 /*witness_len*/ + 1 /*item len*/ +  64 /*signature*/;

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

    pub fn fee(&self) -> Option<u64> {
        self.inputs
            .iter()
            .map(|input| input.value)
            .sum::<u64>()
            .checked_sub(self.outputs.iter().map(|output| output.value).sum())
    }

    pub fn feerate(&self) -> Option<f64> {
        let tx = self.to_rust_bitcoin_tx();
        let mut weight = tx.weight().to_wu() as u32;

        for input in &self.inputs {
            if input.owner().local_owner().is_some() {
                weight += TR_KEYSPEND_SATISFACTION_WEIGHT;
            } else {
                return None;
            }
        }

        let vbytes = weight as f64 / 4.0;
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

        PromptSignBitcoinTx {
            foreign_recipients,
            fee,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptSignBitcoinTx {
    pub foreign_recipients: Vec<(bitcoin::Address, bitcoin::Amount)>,
    pub fee: bitcoin::Amount,
}

impl PromptSignBitcoinTx {
    /// Calculate the total amount being sent to foreign recipients
    pub fn total_sent(&self) -> bitcoin::Amount {
        self.foreign_recipients
            .iter()
            .map(|(_, amount)| *amount)
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
}

impl SpkOwner {
    pub fn root_owner(&self) -> RootOwner {
        match self {
            SpkOwner::Foreign(spk) => RootOwner::Foreign(spk.clone()),
            SpkOwner::Local(local) => RootOwner::Local(local.master_appkey),
        }
    }
    pub fn spk(&self) -> ScriptBuf {
        match self {
            SpkOwner::Foreign(spk) => spk.clone(),
            SpkOwner::Local(owner) => owner.spk(),
        }
    }

    pub fn local_owner_key(&self) -> Option<MasterAppkey> {
        match self {
            SpkOwner::Foreign(_) => None,
            SpkOwner::Local(owner) => Some(owner.master_appkey),
        }
    }

    pub fn local_owner(&self) -> Option<&LocalSpk> {
        match self {
            SpkOwner::Foreign(_) => None,
            SpkOwner::Local(owner) => Some(owner),
        }
    }
}
