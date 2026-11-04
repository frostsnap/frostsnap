use alloc::vec::Vec;
use alloc::{boxed::Box, collections::BTreeMap};
use bitcoin::{
    consensus::Encodable,
    hashes::{sha256d, Hash},
    key::TweakedPublicKey,
    sighash::SighashCache,
    taproot::{ControlBlock, LeafVersion, TapLeafHash, TapNodeHash},
    OutPoint, Script, ScriptBuf, TapSighash, TxOut, Txid, XOnlyPublicKey,
};

use crate::{
    message::EncodedSignature,
    tweak::{AppTweak, BitcoinBip32Path, BitcoinTweak, BitcoinTweakKind, Keychain},
    MasterAppkey,
};

/// Marks a template that has not been narrowed to any one key: the form that travels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash, bincode::Encode, bincode::Decode)]
pub struct Unscoped;

/// Marks a template narrowed to one key by [`TransactionTemplate::as_seen_by`], where
/// `Local` can only mean that key.
///
/// Deliberately not `bincode::Encode`/`Decode`: a scoped template is a local conclusion, and
/// making it unserializable means the form that goes over the wire is the general one by
/// construction rather than by discipline.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ScopedTo(pub MasterAppkey);

/// Invalid state free representation of a transaction.
///
/// The type parameter is the scope: which key's transaction this is.
///
/// An ownership question — which inputs are ours, which recipients are foreign — has no
/// answer until the template names a key, so those readers exist only on
/// `TransactionTemplate<ScopedTo>`. [`TransactionTemplate::as_seen_by`] is the only way
/// across, and it is what firmware's `WireSignTask::check` calls.
///
/// ```compile_fail,E0599
/// # use frostsnap_core::bitcoin_transaction::TransactionTemplate;
/// let template = TransactionTemplate::new();
/// // Whose inputs? The unscoped template cannot say, so this does not compile.
/// template.iter_our_inputs();
/// ```
///
/// ```compile_fail,E0277
/// # use frostsnap_core::{bitcoin_transaction::TransactionTemplate, MasterAppkey};
/// # fn key() -> MasterAppkey { unimplemented!() }
/// let scoped = TransactionTemplate::new().as_seen_by(key());
/// // A scoped template is a local conclusion and must not go over the wire.
/// bincode::encode_to_vec(scoped, bincode::config::standard()).unwrap();
/// ```
///
/// There is deliberately no way back either: scoping erases another key's ownership, so a
/// widened template would carry less than the one it came from. Anything that must send
/// holds the unscoped form and narrows to read.
#[derive(Clone, Debug, bincode::Encode, bincode::Decode, Eq, PartialEq, Hash)]
pub struct TransactionTemplate<S = Unscoped> {
    scope: S,
    #[bincode(with_serde)]
    version: bitcoin::blockdata::transaction::Version,
    #[bincode(with_serde)]
    lock_time: bitcoin::absolute::LockTime,
    inputs: Vec<Input>,
    outputs: Vec<Output>,
}

#[derive(Clone, Copy)]
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

/// Facts that do not depend on whose transaction it is.
impl<S> TransactionTemplate<S> {
    pub fn txid(&self) -> Txid {
        self.to_rust_bitcoin_tx().compute_txid()
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

    /// The sighash for each input. Owned inputs get the sighash for how we spend them (the
    /// leaf sighash on the script path); foreign inputs get the key-path sighash but are never
    /// signed.
    pub fn iter_sighash(&self) -> impl Iterator<Item = TapSighash> + '_ {
        let tx = self.to_rust_bitcoin_tx();
        let mut sighash_cache = SighashCache::new(tx);
        let schnorr_sighashty = bitcoin::sighash::TapSighashType::Default;
        let prevouts = self.inputs.iter().map(Input::txout).collect::<Vec<_>>();
        self.inputs.iter().enumerate().map(move |(i, input)| {
            let prevouts = bitcoin::sighash::Prevouts::All(&prevouts);
            match input.owner.local_owner().and_then(LocalSpk::leaf_hash) {
                Some(leaf_hash) => sighash_cache.taproot_script_spend_signature_hash(
                    i,
                    &prevouts,
                    leaf_hash,
                    schnorr_sighashty,
                ),
                None => {
                    sighash_cache.taproot_key_spend_signature_hash(i, &prevouts, schnorr_sighashty)
                }
            }
            .expect("inputs are right length")
        })
    }

    pub fn fee(&self) -> Option<u64> {
        self.inputs
            .iter()
            .map(|input| input.value)
            .sum::<u64>()
            .checked_sub(self.outputs.iter().map(|output| output.value).sum())
    }
}

impl TransactionTemplate<Unscoped> {
    pub fn new() -> Self {
        Self {
            scope: Unscoped,
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
        owner.check_spk(&txout.script_pubkey)?;

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

    /// Claim an existing output as `owner`'s, proving the claim against its spk.
    ///
    /// For outputs whose spk came from somewhere other than `owner` — a PSBT, an index
    /// lookup — where a `LocalSpk` assembled from mismatched parts would otherwise stand.
    pub fn push_owned_output_checked(
        &mut self,
        txout: &TxOut,
        owner: LocalSpk,
    ) -> Result<(), Box<SpkDoesntMatchPathError>> {
        owner.check_spk(&txout.script_pubkey)?;

        self.push_owned_output(txout.value, owner);
        Ok(())
    }

    /// This transaction as `master_appkey` sees it: another key's scripts become foreign,
    /// because they are not ours to sign and its outputs are money leaving.
    pub fn as_seen_by(&self, master_appkey: MasterAppkey) -> TransactionTemplate<ScopedTo> {
        let foreign_to_us = |owner: &SpkOwner| match owner {
            SpkOwner::Local(local) if local.master_appkey != master_appkey => {
                SpkOwner::Foreign(local.spk())
            }
            owner => owner.clone(),
        };

        TransactionTemplate {
            scope: ScopedTo(master_appkey),
            version: self.version,
            lock_time: self.lock_time,
            inputs: self
                .inputs
                .iter()
                .map(|input| Input {
                    owner: foreign_to_us(&input.owner),
                    ..input.clone()
                })
                .collect(),
            outputs: self
                .outputs
                .iter()
                .map(|output| Output {
                    owner: foreign_to_us(&output.owner),
                    value: output.value,
                })
                .collect(),
        }
    }
}

/// Answers that only make sense once the template has been narrowed to one key.
impl TransactionTemplate<ScopedTo> {
    pub fn master_appkey(&self) -> MasterAppkey {
        self.scope.0
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

    pub fn iter_our_input_sighashes(&self) -> impl Iterator<Item = (LocalSpk, TapSighash)> + '_ {
        self.inputs
            .iter()
            .zip(self.iter_sighash())
            .filter_map(|(input, sighash)| {
                let owner = input.owner.local_owner()?.clone();
                Some((owner, sighash))
            })
    }

    pub fn iter_our_inputs(&self) -> impl Iterator<Item = (usize, &Input, &LocalSpk)> {
        self.inputs
            .iter()
            .enumerate()
            .filter_map(|(i, input)| Some((i, input, input.owner.local_owner()?)))
    }

    /// Every script this key owns in this transaction, on either side, with the path that
    /// derives it.
    ///
    /// The template is the authority on what is ours; asking a wallet index instead answers
    /// a different question and is bounded by whatever it has derived so far.
    pub fn our_spks(&self) -> BTreeMap<ScriptBuf, BitcoinBip32Path> {
        self.iter_our_inputs()
            .map(|(_, _, owner)| (owner.spk(), owner.bip32_path))
            .chain(
                self.iter_our_outputs()
                    .map(|(_, _, owner)| (owner.spk(), owner.bip32_path)),
            )
            .collect()
    }

    /// Pairs each signature with the index of the input it was produced for.
    ///
    /// Signatures arrive in the order [`Self::iter_our_input_sighashes`] produced their
    /// sighashes, which skips foreign inputs. Their positions are therefore *not* input
    /// indices, and pairing them positionally against every input silently signs the wrong one.
    pub fn signatures_by_input_index<'a>(
        &self,
        signatures: &'a [EncodedSignature],
    ) -> Result<Vec<(usize, &'a EncodedSignature)>, SignatureCountMismatch> {
        let expected = self.iter_our_inputs().count();
        if signatures.len() != expected {
            return Err(SignatureCountMismatch {
                expected,
                got: signatures.len(),
            });
        }

        Ok(self
            .iter_our_inputs()
            .map(|(i, _, _)| i)
            .zip(signatures)
            .collect())
    }

    /// The transaction with each signature witnessed onto the input it signs.
    pub fn to_signed_rust_bitcoin_tx(
        &self,
        signatures: &[EncodedSignature],
    ) -> Result<bitcoin::Transaction, SignatureCountMismatch> {
        let mut tx = self.to_rust_bitcoin_tx();
        for (i, signature) in self.signatures_by_input_index(signatures)? {
            let owner = self.inputs[i]
                .owner
                .local_owner()
                .expect("signatures are only paired with our inputs");
            tx.input[i].witness = owner.witness(signature);
        }
        Ok(tx)
    }

    pub fn iter_our_outputs(&self) -> impl Iterator<Item = (usize, &Output, &LocalSpk)> {
        self.outputs
            .iter()
            .enumerate()
            .filter_map(|(i, output)| Some((i, output, output.owner.local_owner()?)))
    }

    /// Whether signing this ourselves produces a transaction anyone can broadcast.
    ///
    /// Distinct from [`Self::has_any_inputs_to_sign`], which asks whether we can *contribute*.
    /// A PSBT can leave us inputs we do not own, and this template has nowhere to put the
    /// signatures for those — `to_rust_bitcoin_tx` builds every input witnessless — so
    /// witnessing ours still leaves a transaction a node will reject. Whoever else must sign
    /// finishes it, from the PSBT. The same holds for a script-path input whose leaf needs
    /// more than our signature.
    pub fn owns_every_input(&self) -> bool {
        self.inputs.iter().all(|input| {
            input
                .owner()
                .local_owner()
                .is_some_and(LocalSpk::is_satisfied_by_our_signature_alone)
        })
    }

    /// Returns true if this transaction has any inputs that need signing by this wallet.
    pub fn has_any_inputs_to_sign(&self) -> bool {
        self.inputs
            .iter()
            .any(|input| input.owner.local_owner().is_some())
    }

    /// What this transaction does to the balance of the key it is scoped to.
    pub fn our_net_value(&self) -> i64 {
        let ours = |owner: &SpkOwner, value: u64| match owner {
            SpkOwner::Local(_) => i64::try_from(value).expect("value ridiculously large"),
            SpkOwner::Foreign(_) => 0,
        };

        self.outputs
            .iter()
            .map(|output| ours(&output.owner, output.value))
            .sum::<i64>()
            - self
                .inputs
                .iter()
                .map(|input| ours(&input.owner, input.value))
                .sum::<i64>()
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
        // Calculate fee rate in sats/vB
        let fee_rate_sats_per_vbyte = self.feerate();

        let any_foreign = self
            .outputs
            .iter()
            .any(|output| matches!(output.owner, SpkOwner::Foreign(_)));
        let internal_count = self
            .iter_our_outputs()
            .filter(|(_, _, local)| {
                local.bip32_path.account_keychain.keychain == Keychain::Internal
            })
            .count();
        // A single change output alongside a foreign recipient is the shape of an
        // ordinary send; disclosing it would train users to skim past their own
        // outputs. Any other local output — or more than one change output — is
        // value returning to us that the signer must be shown.
        let hide_single_change = any_foreign && internal_count == 1;

        let recipients = self
            .outputs
            .iter()
            .filter_map(|output| {
                let owned = match &output.owner {
                    SpkOwner::Foreign(_) => None,
                    SpkOwner::Local(local) => {
                        if hide_single_change
                            && local.bip32_path.account_keychain.keychain == Keychain::Internal
                        {
                            return None;
                        }
                        Some(local.bip32_path)
                    }
                };
                Some(PromptRecipient {
                    destination: PromptDestination::of(&output.owner.spk(), network),
                    amount: bitcoin::Amount::from_sat(output.value),
                    owned,
                })
            })
            .collect();

        let spends_scripted_output = self
            .iter_our_inputs()
            .any(|(_, _, owner)| owner.spends_extra_conditions());

        PromptSignBitcoinTx {
            recipients,
            fee,
            fee_rate_sats_per_vbyte,
            spends_scripted_output,
        }
    }
}

/// Where an output pays, as far as the signer can be shown it.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptDestination {
    Address(bitcoin::Address),
    /// A script with no address rendering: an OP_RETURN data carrier, a bare multisig, a
    /// future segwit version. Says nothing about whether it can be spent — only that we
    /// cannot show it as an address, and a signing screen must not claim more than it knows.
    UnrecognizedScript(ScriptBuf),
}

impl PromptDestination {
    fn of(spk: &Script, network: bitcoin::Network) -> Self {
        match bitcoin::Address::from_script(spk, network) {
            Ok(address) => PromptDestination::Address(address),
            Err(_) => PromptDestination::UnrecognizedScript(spk.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptRecipient {
    pub destination: PromptDestination,
    pub amount: bitcoin::Amount,
    /// `Some` iff the signing wallet itself derives this output's script at that path — the
    /// prompt renders such a recipient as our own.
    pub owned: Option<BitcoinBip32Path>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptSignBitcoinTx {
    /// Disclosed outputs in transaction order. The single change output of an ordinary
    /// send is not disclosed.
    pub recipients: Vec<PromptRecipient>,
    pub fee: bitcoin::Amount,
    /// Fee rate in sats/vB
    pub fee_rate_sats_per_vbyte: Option<f64>,
    /// At least one signed input spends a taproot output that commits to a script tree.
    pub spends_scripted_output: bool,
}

impl PromptSignBitcoinTx {
    /// Total value the prompt itemises as moving.
    pub fn value_moved(&self) -> bitcoin::Amount {
        self.recipients.iter().map(|r| r.amount).sum()
    }

    /// What the proportional high-fee warning is measured against: the value at risk of
    /// leaving the wallet.
    ///
    /// When anything goes to a foreign recipient that is the foreign total alone — outputs
    /// returning to us are not at risk, and counting them would let a self-payment dilute the
    /// ratio until a disproportionate fee stopped warning. When nothing leaves, the fee is the
    /// only value the transaction actually consumes, so the self-spend total is the proxy that
    /// keeps the warning armed.
    pub fn value_at_risk(&self) -> bitcoin::Amount {
        match self.foreign_value() {
            Some(foreign) => foreign,
            None => self.value_moved(),
        }
    }

    /// The value leaving the wallet, or `None` when nothing does. Whichever arm this takes decides
    /// both the denominator above and how the warning describes itself, so the two cannot drift.
    pub fn foreign_value(&self) -> Option<bitcoin::Amount> {
        let foreign: bitcoin::Amount = self
            .recipients
            .iter()
            .filter(|r| r.owned.is_none())
            .map(|r| r.amount)
            .sum();
        (foreign > bitcoin::Amount::ZERO).then_some(foreign)
    }
}

/// A signature was produced for every input of ours; a different count means the list did
/// not come from this template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureCountMismatch {
    pub expected: usize,
    pub got: usize,
}

impl core::fmt::Display for SignatureCountMismatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "expected {} signatures for this transaction's own inputs but got {}",
            self.expected, self.got
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SignatureCountMismatch {}

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

/// Encoded as part of [`SpkOwner`], which has a hand-written bincode encoding to stay
/// compatible with old databases and firmware.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocalSpk {
    pub master_appkey: MasterAppkey,
    pub bip32_path: BitcoinBip32Path,
    pub spend: LocalSpend,
}

/// How this wallet spends a taproot output it has a key in.
#[derive(bincode::Encode, bincode::Decode, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocalSpend {
    /// We are the taproot internal key and spend the key path. `merkle_root` is `None` for a
    /// plain BIP86 output, or `Some` when the output also commits to a script tree.
    KeySpend {
        #[bincode(with_serde)]
        merkle_root: Option<TapNodeHash>,
    },
    /// We are a key *inside* a tapscript leaf and spend that leaf (script path). The output
    /// key can't be re-derived from our leaf key alone, so we carry the prevout `script_pubkey`,
    /// the leaf we're satisfying, and the `control_block` (merkle proof) needed to build the
    /// witness `[signature, leaf_script, control_block]`.
    ScriptSpend {
        #[bincode(with_serde)]
        script_pubkey: ScriptBuf,
        #[bincode(with_serde)]
        leaf_script: ScriptBuf,
        /// Always a valid [`LeafVersion`]: checked at construction and decode.
        leaf_version: u8,
        control_block: Vec<u8>,
    },
}

impl LocalSpk {
    /// A plain BIP86 key-spend-only output (no script tree).
    pub fn key_spend(master_appkey: MasterAppkey, bip32_path: BitcoinBip32Path) -> Self {
        Self::key_spend_with_merkle_root(master_appkey, bip32_path, None)
    }

    /// A key-spend of a taproot output that may also commit to a script tree.
    pub fn key_spend_with_merkle_root(
        master_appkey: MasterAppkey,
        bip32_path: BitcoinBip32Path,
        merkle_root: Option<TapNodeHash>,
    ) -> Self {
        Self {
            master_appkey,
            bip32_path,
            spend: LocalSpend::KeySpend { merkle_root },
        }
    }

    /// A script-path spend of `leaf_script` (whose prevout is `script_pubkey`) where our key at
    /// `bip32_path` appears in the leaf. `control_block` is the leaf's merkle proof.
    pub fn script_spend(
        master_appkey: MasterAppkey,
        bip32_path: BitcoinBip32Path,
        script_pubkey: ScriptBuf,
        leaf_script: ScriptBuf,
        leaf_version: LeafVersion,
        control_block: Vec<u8>,
    ) -> Self {
        Self {
            master_appkey,
            bip32_path,
            spend: LocalSpend::ScriptSpend {
                script_pubkey,
                leaf_script,
                leaf_version: leaf_version.to_consensus(),
                control_block,
            },
        }
    }

    pub fn app_tweak(&self) -> AppTweak {
        let kind = match &self.spend {
            LocalSpend::KeySpend { merkle_root } => BitcoinTweakKind::KeySpend {
                merkle_root: *merkle_root,
            },
            LocalSpend::ScriptSpend { .. } => BitcoinTweakKind::ScriptSpend,
        };
        AppTweak::Bitcoin(BitcoinTweak {
            bip32_path: self.bip32_path,
            kind,
        })
    }

    /// The tapscript leaf hash to sign against, for a script-path spend.
    pub fn leaf_hash(&self) -> Option<TapLeafHash> {
        match &self.spend {
            LocalSpend::KeySpend { .. } => None,
            LocalSpend::ScriptSpend {
                leaf_script,
                leaf_version,
                ..
            } => {
                let version = LeafVersion::from_consensus(*leaf_version)
                    .expect("checked at construction and decode");
                Some(TapLeafHash::from_script(leaf_script, version))
            }
        }
    }

    /// The x-only key we actually sign with: the tweaked output key for a key-path spend, or
    /// the raw leaf key for a script-path spend.
    pub fn signing_xonly(&self) -> XOnlyPublicKey {
        self.app_tweak()
            .derive_xonly_key(&self.master_appkey.to_xpub())
            .into()
    }

    /// True if this is anything other than a plain BIP86 key-spend.
    pub fn spends_extra_conditions(&self) -> bool {
        match &self.spend {
            LocalSpend::KeySpend { merkle_root } => merkle_root.is_some(),
            LocalSpend::ScriptSpend { .. } => true,
        }
    }

    /// Proves this owner derives `spk`, which is what separates a claim that is ours from one
    /// that is merely made. A PSBT names a key and a path; only the script decides.
    ///
    /// A script-path owner carries its `spk` rather than deriving it, so for one the proof is
    /// that the leaf holds the key we sign with and the control block commits the leaf to
    /// `spk`'s output key.
    fn check_spk(&self, spk: &ScriptBuf) -> Result<(), Box<SpkDoesntMatchPathError>> {
        let expected = self.spk();
        let leaf_proven = match &self.spend {
            LocalSpend::KeySpend { .. } => true,
            LocalSpend::ScriptSpend {
                leaf_script,
                control_block,
                ..
            } => {
                let key = self.signing_xonly().serialize();
                let key_in_leaf = leaf_script.instructions().any(|instruction| {
                    matches!(
                        instruction,
                        Ok(bitcoin::script::Instruction::PushBytes(bytes)) if bytes.as_bytes() == key
                    )
                });
                let committed = match (ControlBlock::decode(control_block), tweaked_output_key(spk))
                {
                    (Ok(control_block), Some(output_key)) => control_block
                        .verify_taproot_commitment(
                            &bitcoin::secp256k1::Secp256k1::verification_only(),
                            output_key,
                            leaf_script,
                        ),
                    _ => false,
                };
                key_in_leaf && committed
            }
        };
        if *spk != expected || !leaf_proven {
            return Err(Box::new(SpkDoesntMatchPathError {
                got: spk.clone(),
                expected,
                path: self
                    .bip32_path
                    .path_segments_from_bitcoin_appkey()
                    .collect(),
                master_appkey: self.master_appkey,
            }));
        }
        Ok(())
    }

    pub fn spk(&self) -> ScriptBuf {
        match &self.spend {
            LocalSpend::KeySpend { .. } => ScriptBuf::new_p2tr_tweaked(
                TweakedPublicKey::dangerous_assume_tweaked(self.signing_xonly()),
            ),
            LocalSpend::ScriptSpend { script_pubkey, .. } => script_pubkey.clone(),
        }
    }

    /// The witness that spends this with `signature`: `[signature]` on the key path, or
    /// `[signature, leaf_script, control_block]` on the script path.
    ///
    /// Only complete when our signature alone satisfies the leaf; see
    /// [`Self::is_satisfied_by_our_signature_alone`].
    pub fn witness(&self, signature: &EncodedSignature) -> bitcoin::Witness {
        let signature = bitcoin::taproot::Signature {
            signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                .expect("a schnorr signature is 64 bytes"),
            sighash_type: bitcoin::sighash::TapSighashType::Default,
        }
        .to_vec();
        match &self.spend {
            LocalSpend::KeySpend { .. } => bitcoin::Witness::from_slice(&[signature]),
            LocalSpend::ScriptSpend {
                leaf_script,
                control_block,
                ..
            } => bitcoin::Witness::from_slice(&[
                signature,
                leaf_script.to_bytes(),
                control_block.clone(),
            ]),
        }
    }

    /// Whether [`Self::witness`] is a complete witness: always on the key path, and on the
    /// script path only for the single-key leaf `<our key> OP_CHECKSIG`. Any other leaf may need
    /// other keys' signatures or more stack items, which only whoever finalises the PSBT knows.
    pub fn is_satisfied_by_our_signature_alone(&self) -> bool {
        match &self.spend {
            LocalSpend::KeySpend { .. } => true,
            LocalSpend::ScriptSpend {
                leaf_script,
                leaf_version,
                ..
            } => {
                *leaf_version == LeafVersion::TapScript.to_consensus()
                    && *leaf_script
                        == bitcoin::script::Builder::new()
                            .push_slice(self.signing_xonly().serialize())
                            .push_opcode(bitcoin::opcodes::all::OP_CHECKSIG)
                            .into_script()
            }
        }
    }
}

/// The output key of a P2TR `spk`, or `None` if it isn't one.
fn tweaked_output_key(spk: &Script) -> Option<XOnlyPublicKey> {
    if !spk.is_p2tr() {
        return None;
    }
    XOnlyPublicKey::from_slice(&spk.as_bytes()[2..34]).ok()
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SpkOwner {
    Foreign(ScriptBuf),
    Local(LocalSpk),
}

/// `SpkOwner` crosses the device wire (inside [`TransactionTemplate`]) and is persisted in
/// signing session mutations, so its encoding must stay stable. Plain BIP86 key-spends keep
/// the pre-scripted-taproot layout (`Local { master_appkey, bip32_path }` at variant 1) so old
/// databases still decode and old firmware still understands plain spends; scripted spends use
/// an appended variant 2 that also carries the [`LocalSpend`]. Each value has exactly one
/// encoding: a plain key-spend is never written as variant 2.
impl bincode::Encode for SpkOwner {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        match self {
            SpkOwner::Foreign(spk) => {
                0u32.encode(encoder)?;
                bincode::serde::Compat(spk).encode(encoder)
            }
            SpkOwner::Local(local) => {
                let plain = local.spend == (LocalSpend::KeySpend { merkle_root: None });
                if plain { 1u32 } else { 2u32 }.encode(encoder)?;
                local.master_appkey.encode(encoder)?;
                local.bip32_path.encode(encoder)?;
                if !plain {
                    local.spend.encode(encoder)?;
                }
                Ok(())
            }
        }
    }
}

impl<Context> bincode::Decode<Context> for SpkOwner {
    fn decode<D: bincode::de::Decoder>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Ok(match u32::decode(decoder)? {
            0 => SpkOwner::Foreign(bincode::serde::Compat::<ScriptBuf>::decode(decoder)?.0),
            variant @ (1 | 2) => {
                let master_appkey = bincode::Decode::decode(decoder)?;
                let bip32_path = bincode::Decode::decode(decoder)?;
                let spend = if variant == 1 {
                    LocalSpend::KeySpend { merkle_root: None }
                } else {
                    match bincode::Decode::decode(decoder)? {
                        LocalSpend::KeySpend { merkle_root: None } => {
                            return Err(bincode::error::DecodeError::Other(
                                "a plain key-spend owner must be encoded as variant 1",
                            ))
                        }
                        LocalSpend::ScriptSpend { leaf_version, .. }
                            if LeafVersion::from_consensus(leaf_version).is_err() =>
                        {
                            return Err(bincode::error::DecodeError::OtherString(alloc::format!(
                                "invalid taproot leaf version {leaf_version}"
                            )))
                        }
                        spend => spend,
                    }
                };
                SpkOwner::Local(LocalSpk {
                    master_appkey,
                    bip32_path,
                    spend,
                })
            }
            variant => {
                return Err(bincode::error::DecodeError::OtherString(alloc::format!(
                    "unknown SpkOwner variant {variant}"
                )))
            }
        })
    }
}

impl<'de, Context> bincode::BorrowDecode<'de, Context> for SpkOwner {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        bincode::Decode::decode(decoder)
    }
}

impl SpkOwner {
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::tweak::{AppTweakKind, NormalIndex, TweakableKey};
    use bitcoin::{
        opcodes::all::{OP_CHECKSIG, OP_CHECKSIGVERIFY},
        script::Builder,
        secp256k1::{Keypair, Secp256k1, SecretKey},
        sighash::{Prevouts, TapSighashType},
        taproot::TaprootBuilder,
        Amount,
    };
    use schnorr_fun::fun::{g, G};

    fn key() -> MasterAppkey {
        MasterAppkey::derive_from_rootkey(g!(2 * G).normalize())
    }

    fn path(index: u32) -> BitcoinBip32Path {
        BitcoinBip32Path::external(NormalIndex::new(index).unwrap())
    }

    /// The untweaked key at `bip32_path`: the taproot internal key of a BIP86 output, and the
    /// key that goes in a leaf.
    fn raw_key(master_appkey: MasterAppkey, bip32_path: BitcoinBip32Path) -> XOnlyPublicKey {
        master_appkey
            .derive_appkey(AppTweakKind::Bitcoin)
            .derive_bip32(bip32_path.path_segments_from_bitcoin_appkey())
            .into_key()
            .to_libsecp_xonly()
    }

    /// A key that is nobody's in these tests: the internal key of a script-only output.
    fn unrelated_internal_key() -> XOnlyPublicKey {
        Keypair::from_secret_key(
            &Secp256k1::new(),
            &SecretKey::from_slice(&[9u8; 32]).unwrap(),
        )
        .x_only_public_key()
        .0
    }

    /// Script-path spends `leaf_script`, the only leaf of an output under an unrelated key.
    fn leaf_spend(bip32_path: BitcoinBip32Path, leaf_script: ScriptBuf) -> LocalSpk {
        let secp = Secp256k1::verification_only();
        let spend_info = TaprootBuilder::new()
            .add_leaf(0, leaf_script.clone())
            .unwrap()
            .finalize(&secp, unrelated_internal_key())
            .unwrap();
        let control_block = spend_info
            .control_block(&(leaf_script.clone(), LeafVersion::TapScript))
            .unwrap()
            .serialize();
        LocalSpk::script_spend(
            key(),
            bip32_path,
            ScriptBuf::new_p2tr(&secp, unrelated_internal_key(), spend_info.merkle_root()),
            leaf_script,
            LeafVersion::TapScript,
            control_block,
        )
    }

    fn single_key_leaf(bip32_path: BitcoinBip32Path) -> ScriptBuf {
        Builder::new()
            .push_x_only_key(&raw_key(key(), bip32_path))
            .push_opcode(OP_CHECKSIG)
            .into_script()
    }

    /// The layout `SpkOwner` had before scripted taproot spends. Encodings of this must decode
    /// as `SpkOwner`, and plain key-spend owners must encode identically, so that old persisted
    /// signing sessions and old firmware stay compatible.
    #[derive(bincode::Encode, bincode::Decode)]
    enum LegacySpkOwner {
        Foreign(#[bincode(with_serde)] ScriptBuf),
        Local(LegacyLocalSpk),
    }

    #[derive(bincode::Encode, bincode::Decode)]
    struct LegacyLocalSpk {
        master_appkey: MasterAppkey,
        bip32_path: BitcoinBip32Path,
    }

    #[test]
    fn spk_owner_encoding_is_backwards_compatible() {
        let config = bincode::config::standard();
        let master_appkey = key();
        let bip32_path = path(7);

        let legacy_bytes = bincode::encode_to_vec(
            LegacySpkOwner::Local(LegacyLocalSpk {
                master_appkey,
                bip32_path,
            }),
            config,
        )
        .unwrap();
        let key_spend = SpkOwner::Local(LocalSpk::key_spend(master_appkey, bip32_path));
        assert_eq!(
            bincode::encode_to_vec(&key_spend, config).unwrap(),
            legacy_bytes
        );
        let (decoded, _) =
            bincode::decode_from_slice::<SpkOwner, _>(&legacy_bytes, config).unwrap();
        assert_eq!(decoded, key_spend);

        let foreign_spk = ScriptBuf::new_op_return([1u8; 4]);
        let legacy_foreign =
            bincode::encode_to_vec(LegacySpkOwner::Foreign(foreign_spk.clone()), config).unwrap();
        let foreign = SpkOwner::Foreign(foreign_spk);
        assert_eq!(
            bincode::encode_to_vec(&foreign, config).unwrap(),
            legacy_foreign
        );
        let (decoded, _) =
            bincode::decode_from_slice::<SpkOwner, _>(&legacy_foreign, config).unwrap();
        assert_eq!(decoded, foreign);

        for owner in [
            SpkOwner::Local(LocalSpk::key_spend_with_merkle_root(
                master_appkey,
                bip32_path,
                Some(TapNodeHash::from_byte_array([7u8; 32])),
            )),
            SpkOwner::Local(leaf_spend(bip32_path, single_key_leaf(bip32_path))),
        ] {
            let bytes = bincode::encode_to_vec(&owner, config).unwrap();
            let (decoded, _) = bincode::decode_from_slice::<SpkOwner, _>(&bytes, config).unwrap();
            assert_eq!(decoded, owner);
        }

        // A plain key-spend smuggled in as the scripted variant would be a second encoding of
        // the same owner.
        let mut second_form = legacy_bytes.clone();
        second_form[0] = 2;
        second_form.extend(
            bincode::encode_to_vec(LocalSpend::KeySpend { merkle_root: None }, config).unwrap(),
        );
        assert!(bincode::decode_from_slice::<SpkOwner, _>(&second_form, config).is_err());
    }

    /// The output key we derive for a key-spend of a taproot output committing to a script tree
    /// must match what rust-bitcoin builds from `internal_key + merkle_root`.
    #[test]
    fn scripted_keyspend_spk_matches_rust_bitcoin() {
        let secp = Secp256k1::verification_only();
        let internal_key = raw_key(key(), path(7));
        let merkle_root = TapNodeHash::from_byte_array([7u8; 32]);

        let scripted = LocalSpk::key_spend_with_merkle_root(key(), path(7), Some(merkle_root));
        assert_eq!(
            scripted.spk(),
            ScriptBuf::new_p2tr(&secp, internal_key, Some(merkle_root)),
        );

        let plain = LocalSpk::key_spend(key(), path(7));
        assert_eq!(plain.spk(), ScriptBuf::new_p2tr(&secp, internal_key, None));
        assert_ne!(scripted.spk(), plain.spk());
    }

    /// For a script-path spend, the leaf sighash we compute must equal rust-bitcoin's, the key
    /// we sign with must be the raw (untweaked) key in the leaf, and the witness must be
    /// `[signature, leaf_script, control_block]`.
    #[test]
    fn script_path_spend_matches_rust_bitcoin() {
        let leaf_script = single_key_leaf(path(1));
        let owner = leaf_spend(path(1), leaf_script.clone());
        assert_eq!(owner.signing_xonly(), raw_key(key(), path(1)));

        let prevout = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: owner.spk(),
        };
        let mut template = TransactionTemplate::new();
        template
            .push_owned_input(
                PushInput::spend_outpoint(&prevout, OutPoint::null()),
                owner.clone(),
            )
            .unwrap();
        template.push_foreign_output(TxOut {
            value: Amount::from_sat(90_000),
            script_pubkey: ScriptBuf::new_op_return([]),
        });

        let ours = template.iter_sighash().next().unwrap();
        let tx = template.to_rust_bitcoin_tx();
        let leaf_hash = TapLeafHash::from_script(&leaf_script, LeafVersion::TapScript);
        let expected = SighashCache::new(&tx)
            .taproot_script_spend_signature_hash(
                0,
                &Prevouts::All(&[prevout]),
                leaf_hash,
                TapSighashType::Default,
            )
            .unwrap();
        assert_eq!(ours, expected);

        let signature = EncodedSignature([5u8; 64]);
        let signed = template
            .as_seen_by(key())
            .to_signed_rust_bitcoin_tx(&[signature])
            .unwrap();
        let LocalSpend::ScriptSpend { control_block, .. } = &owner.spend else {
            unreachable!()
        };
        assert_eq!(
            signed.input[0].witness.to_vec(),
            vec![
                signature.0.to_vec(),
                leaf_script.to_bytes(),
                control_block.clone()
            ]
        );
    }

    /// Pushing is where ownership is proven, so a script-path owner must be refused when its
    /// leaf lacks our key or its control block does not commit the leaf to the prevout.
    #[test]
    fn a_leaf_we_cannot_prove_is_refused() {
        let ours = leaf_spend(path(1), single_key_leaf(path(1)));

        let someone_elses_leaf = leaf_spend(path(1), single_key_leaf(path(2)));

        // Our leaf's proof in a different tree, so it commits to a different output key.
        let mut wrong_control_block = ours.clone();
        if let LocalSpend::ScriptSpend { control_block, .. } = &mut wrong_control_block.spend {
            let other_tree = TaprootBuilder::new()
                .add_leaf(1, single_key_leaf(path(1)))
                .unwrap()
                .add_leaf(1, single_key_leaf(path(3)))
                .unwrap()
                .finalize(&Secp256k1::verification_only(), unrelated_internal_key())
                .unwrap();
            *control_block = other_tree
                .control_block(&(single_key_leaf(path(1)), LeafVersion::TapScript))
                .unwrap()
                .serialize();
        }

        for (owner, expected) in [
            (ours, true),
            (someone_elses_leaf, false),
            (wrong_control_block, false),
        ] {
            let prevout = TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: owner.spk(),
            };
            let mut template = TransactionTemplate::new();
            let pushed = template
                .push_owned_input(PushInput::spend_outpoint(&prevout, OutPoint::null()), owner);
            assert_eq!(pushed.is_ok(), expected);
        }
    }

    /// Only `<our key> OP_CHECKSIG` is satisfied by our signature alone; any other leaf leaves
    /// the transaction for whoever finalises the PSBT.
    #[test]
    fn only_a_single_key_leaf_lets_us_finalise() {
        let other_key = raw_key(
            MasterAppkey::derive_from_rootkey(g!(3 * G).normalize()),
            path(0),
        );
        let two_key_leaf = Builder::new()
            .push_x_only_key(&raw_key(key(), path(1)))
            .push_opcode(OP_CHECKSIGVERIFY)
            .push_x_only_key(&other_key)
            .push_opcode(OP_CHECKSIG)
            .into_script();

        for (leaf_script, finalisable) in [(single_key_leaf(path(1)), true), (two_key_leaf, false)]
        {
            let mut template = TransactionTemplate::new();
            template.push_imaginary_owned_input(
                leaf_spend(path(1), leaf_script),
                Amount::from_sat(100_000),
            );
            assert_eq!(template.as_seen_by(key()).owns_every_input(), finalisable);
        }
    }
}
