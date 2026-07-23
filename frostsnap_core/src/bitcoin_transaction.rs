use alloc::vec::Vec;
use alloc::{boxed::Box, collections::BTreeMap};
use bitcoin::{
    consensus::Encodable,
    hashes::{sha256d, Hash},
    key::TweakedPublicKey,
    sighash::SighashCache,
    taproot::{LeafVersion, TapLeafHash, TapNodeHash},
    OutPoint, Script, ScriptBuf, TapSighash, TxOut, Txid, XOnlyPublicKey,
};

use crate::{
    tweak::{AppTweak, BitcoinBip32Path, BitcoinTweak, BitcoinTweakKind},
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

    /// The sighash for each input. Owned inputs use the sighash for how we spend them (key-path
    /// or script-path); foreign inputs default to the key-path sighash (never used — we don't
    /// sign them).
    pub fn iter_sighash(&self) -> Vec<TapSighash> {
        let tx = self.to_rust_bitcoin_tx();
        let mut sighash_cache = SighashCache::new(&tx);
        let ty = bitcoin::sighash::TapSighashType::Default;
        let prevouts = self.inputs.iter().map(Input::txout).collect::<Vec<_>>();
        let all = bitcoin::sighash::Prevouts::All(&prevouts);
        self.inputs
            .iter()
            .enumerate()
            .map(|(i, input)| {
                match input.owner.local_owner().and_then(LocalSpk::leaf_hash) {
                    // Script-path: sign the leaf sighash.
                    Some(leaf_hash) => sighash_cache
                        .taproot_script_spend_signature_hash(i, &all, leaf_hash, ty)
                        .expect("inputs are right length"),
                    // Key-path (or foreign): sign the key-spend sighash.
                    None => sighash_cache
                        .taproot_key_spend_signature_hash(i, &all, ty)
                        .expect("inputs are right length"),
                }
            })
            .collect()
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

        // True if any input we're signing is a key-spend of an output that also commits to a
        // taproot script tree — worth flagging to the user as an advanced/experimental spend.
        let spends_script_path = self
            .iter_locally_owned_inputs()
            .any(|(_, _, owner)| owner.spends_extra_conditions());

        PromptSignBitcoinTx {
            foreign_recipients,
            fee,
            fee_rate_sats_per_vbyte,
            spends_script_path,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptSignBitcoinTx {
    pub foreign_recipients: Vec<(bitcoin::Address, bitcoin::Amount)>,
    pub fee: bitcoin::Amount,
    /// Fee rate in sats/vB
    pub fee_rate_sats_per_vbyte: Option<f64>,
    /// At least one signed input spends a taproot output that commits to a script tree.
    pub spends_script_path: bool,
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
    pub spend: LocalSpend,
}

/// How this wallet owns/spends a taproot output.
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
        leaf_version: u8,
        control_block: Vec<u8>,
    },
}

impl LocalSpk {
    /// A plain BIP86 key-spend-only owned output (no script tree).
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
        leaf_version: u8,
        control_block: Vec<u8>,
    ) -> Self {
        Self {
            master_appkey,
            bip32_path,
            spend: LocalSpend::ScriptSpend {
                script_pubkey,
                leaf_script,
                leaf_version,
                control_block,
            },
        }
    }

    /// The witness pieces for a script-path spend: `(leaf_script, control_block)`.
    pub fn script_path_witness(&self) -> Option<(&Script, &[u8])> {
        match &self.spend {
            LocalSpend::KeySpend { .. } => None,
            LocalSpend::ScriptSpend {
                leaf_script,
                control_block,
                ..
            } => Some((leaf_script.as_script(), control_block)),
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
                let version = LeafVersion::from_consensus(*leaf_version).ok()?;
                Some(TapLeafHash::from_script(leaf_script, version))
            }
        }
    }

    /// The x-only key we actually sign with (tweaked output key for key-spend, or the raw leaf
    /// key for script-path).
    pub fn signing_xonly(&self) -> XOnlyPublicKey {
        self.app_tweak()
            .derive_xonly_key(&self.master_appkey.to_xpub())
            .into()
    }

    /// True if this is anything other than a plain BIP86 key-spend — worth flagging to the user.
    pub fn spends_extra_conditions(&self) -> bool {
        match &self.spend {
            LocalSpend::KeySpend { merkle_root } => merkle_root.is_some(),
            LocalSpend::ScriptSpend { .. } => true,
        }
    }

    pub fn spk(&self) -> ScriptBuf {
        match &self.spend {
            LocalSpend::KeySpend { .. } => ScriptBuf::new_p2tr_tweaked(
                TweakedPublicKey::dangerous_assume_tweaked(self.signing_xonly()),
            ),
            LocalSpend::ScriptSpend { script_pubkey, .. } => script_pubkey.clone(),
        }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::tweak::{AppTweakKind, TweakableKey};
    use bitcoin::{hashes::Hash, secp256k1::Secp256k1, taproot::TapNodeHash};
    use schnorr_fun::fun::Point;

    /// The output key we derive for a key-spend of a taproot output committing to a script tree
    /// must match what rust-bitcoin builds from `internal_key + merkle_root` — i.e. it applies
    /// the taproot tweak the same way Bitcoin Core does. This is what lets a Frostsnap key be the
    /// internal key of e.g. a Liana-style vault while we only ever sign the key path.
    #[test]
    fn scripted_keyspend_spk_matches_rust_bitcoin() {
        let master_appkey =
            MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
        let bip32_path = BitcoinBip32Path::external(7);

        // The untweaked taproot internal key we derive for this path.
        let internal_xonly = master_appkey
            .derive_appkey(AppTweakKind::Bitcoin)
            .derive_bip32(bip32_path.path_segments_from_bitcoin_appkey())
            .into_key()
            .to_libsecp_xonly();

        let secp = Secp256k1::verification_only();
        // A dummy script-tree merkle root (recovery leaves held by other keys).
        let merkle_root = TapNodeHash::from_byte_array([7u8; 32]);

        let scripted =
            LocalSpk::key_spend_with_merkle_root(master_appkey, bip32_path, Some(merkle_root));
        assert_eq!(
            scripted.spk(),
            ScriptBuf::new_p2tr(&secp, internal_xonly, Some(merkle_root)),
            "scripted key-spend spk must match rust-bitcoin new_p2tr(internal, Some(root))"
        );

        // The plain BIP86 key-spend path (no scripts) still matches new_p2tr(.., None).
        let keyspend = LocalSpk::key_spend(master_appkey, bip32_path);
        assert_eq!(
            keyspend.spk(),
            ScriptBuf::new_p2tr(&secp, internal_xonly, None),
        );
        assert_ne!(scripted.spk(), keyspend.spk());
    }

    /// For a script-path spend, the leaf sighash we compute must equal rust-bitcoin's, and the
    /// key we sign with must be the raw (untweaked) key that actually appears in the leaf. This is
    /// what lets a Frostsnap key sit inside an arbitrary tapscript leaf and spend it.
    #[test]
    fn script_path_spend_matches_rust_bitcoin() {
        use bitcoin::{
            key::UntweakedPublicKey,
            opcodes::all::OP_CHECKSIG,
            script::Builder,
            secp256k1::{Keypair, SecretKey},
            sighash::{Prevouts, SighashCache, TapSighashType},
            taproot::{LeafVersion, TaprootBuilder},
            Amount, OutPoint, Txid,
        };

        let secp = Secp256k1::new();
        let master_appkey =
            MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
        let bip32_path = BitcoinBip32Path::external(1);

        // The (untweaked) leaf key we'll sign with, and a leaf script `<key> OP_CHECKSIG`.
        let leaf_xonly = LocalSpk::script_spend(
            master_appkey,
            bip32_path,
            ScriptBuf::new(),
            ScriptBuf::new(),
            LeafVersion::TapScript.to_consensus(),
            vec![],
        )
        .signing_xonly();
        let leaf_script = Builder::new()
            .push_slice(leaf_xonly.serialize())
            .push_opcode(OP_CHECKSIG)
            .into_script();

        // A taproot output committing to that leaf under some unrelated internal key.
        let internal_key: UntweakedPublicKey =
            Keypair::from_secret_key(&secp, &SecretKey::from_slice(&[9u8; 32]).unwrap())
                .x_only_public_key()
                .0;
        let spend_info = TaprootBuilder::new()
            .add_leaf(0, leaf_script.clone())
            .unwrap()
            .finalize(&secp, internal_key)
            .unwrap();
        let prevout = TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: ScriptBuf::new_p2tr(&secp, internal_key, spend_info.merkle_root()),
        };
        let control_block = spend_info
            .control_block(&(leaf_script.clone(), LeafVersion::TapScript))
            .unwrap()
            .serialize();

        // Build a template that script-path spends the output.
        let owner = LocalSpk::script_spend(
            master_appkey,
            bip32_path,
            prevout.script_pubkey.clone(),
            leaf_script.clone(),
            LeafVersion::TapScript.to_consensus(),
            control_block,
        );
        // The key we sign with really appears in the leaf.
        assert!(leaf_script
            .as_bytes()
            .windows(32)
            .any(|w| w == owner.signing_xonly().serialize().as_slice()));

        let outpoint = OutPoint {
            txid: Txid::all_zeros(),
            vout: 0,
        };
        let mut template = TransactionTemplate::new();
        template
            .push_owned_input(PushInput::spend_outpoint(&prevout, outpoint), owner)
            .unwrap();
        template.push_foreign_output(TxOut {
            value: Amount::from_sat(90_000),
            script_pubkey: ScriptBuf::new_op_return([]),
        });

        // Our leaf sighash must equal rust-bitcoin's for the same tx + leaf.
        let ours = template.iter_sighash()[0];
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
    }
}
