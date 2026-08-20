use crate::{
    bitcoin_transaction::{
        LocalSpk, PushInput, ScopedTo, SignatureCountMismatch, SpkDoesntMatchPathError,
        TransactionTemplate,
    },
    message::EncodedSignature,
    tweak::{AppTweakKind, BitcoinBip32Path},
    MasterAppkey,
};
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use bitcoin::{
    bip32::{self, Fingerprint},
    taproot, Psbt, XOnlyPublicKey,
};
use tracing::{event, Level};

impl TransactionTemplate {
    /// Reads a PSBT as a transaction `master_appkeys` are being asked to sign.
    ///
    /// Ownership is taken from the PSBT's own key origins on both sides, and every claimed
    /// path is derived from the key it names and matched against the actual script before it
    /// counts, so a PSBT cannot make us attribute a script to a key that does not produce it.
    ///
    /// The result is unscoped and stays that way honestly: with several keys given, `Local`
    /// means one of them and records which, and [`TransactionTemplate::as_seen_by`] narrows it
    /// to whichever is signing. Anything none of the keys owns is recorded as foreign rather
    /// than rejected — a PSBT is only refused outright when it is malformed, or when nothing
    /// in it is ours to sign.
    pub fn from_psbt(
        psbt: &Psbt,
        master_appkeys: &[MasterAppkey],
    ) -> Result<TransactionTemplate, PsbtValidationError> {
        // A fingerprint is four bytes, so two of our own keys can share one. Holding every
        // candidate and letting the script decide means a collision cannot hide the real owner.
        let mut ours_by_fingerprint: BTreeMap<Fingerprint, Vec<MasterAppkey>> = BTreeMap::new();
        for &master_appkey in master_appkeys {
            let fingerprint = master_appkey
                .derive_appkey(AppTweakKind::Bitcoin)
                .fingerprint();
            ours_by_fingerprint
                .entry(fingerprint)
                .or_default()
                .push(master_appkey);
        }

        let mut template = TransactionTemplate::new();
        let unsigned_tx = &psbt.unsigned_tx;
        template.set_version(unsigned_tx.version);
        template.set_lock_time(unsigned_tx.lock_time);

        let mut already_signed_count = 0;
        let mut foreign_count = 0;
        let mut owned_count = 0;

        for (i, input) in psbt.inputs.iter().enumerate() {
            let txin = unsigned_tx.input.get(i).ok_or_else(|| {
                PsbtValidationError::Other(format!("PSBT input {i} is malformed"))
            })?;

            let txout = input
                .witness_utxo
                .as_ref()
                .or_else(|| {
                    let tx = input.non_witness_utxo.as_ref()?;
                    tx.output.get(txin.previous_output.vout as usize)
                })
                .ok_or_else(|| {
                    PsbtValidationError::Other(format!(
                        "PSBT input {i} missing witness and non-witness utxo"
                    ))
                })?;

            let input_push =
                PushInput::spend_outpoint(txout, txin.previous_output).with_sequence(txin.sequence);

            macro_rules! bail {
            ($category:ident, $($reason:tt)*) => {{
                event!(
                    Level::INFO,
                    "Skipping signing PSBT input {i} because {}", $($reason)*
                );
                $category += 1;
                template.push_foreign_input(input_push);
                continue;

            }};
        }

            if input.final_script_witness.is_some() {
                bail!(
                    already_signed_count,
                    "it already has a final_script_witness"
                );
            }

            let (bip32_path, candidates) = match claimed_path(
                input.tap_internal_key.as_ref(),
                &input.tap_key_origins,
                &ours_by_fingerprint,
            ) {
                Ok(claim) => claim,
                // Refusing outright rather than skipping: an input claiming our fingerprint at a
                // path we cannot derive is a PSBT we do not understand, not someone else's coin.
                Err(NotOurs::Hardened) => {
                    return Err(PsbtValidationError::Other(
                        "can't sign with hardened derivation".to_string(),
                    ))
                }
                Err(reason) => bail!(foreign_count, reason.to_string()),
            };

            push_owned(candidates, bip32_path, |owner| {
                template.push_owned_input(input_push, owner)
            })?;
            owned_count += 1;
        }

        for (i, txout) in unsigned_tx.output.iter().enumerate() {
            let claim = match psbt.outputs.get(i) {
                Some(output) => claimed_path(
                    output.tap_internal_key.as_ref(),
                    &output.tap_key_origins,
                    &ours_by_fingerprint,
                ),
                None => Err(NotOurs::NoOrigin),
            };

            match claim {
                Ok((bip32_path, candidates)) => push_owned(candidates, bip32_path, |owner| {
                    template.push_owned_output_checked(txout, owner)
                })?,
                Err(reason) => {
                    event!(Level::INFO, "PSBT output {i} isn't ours because {reason}");
                    template.push_foreign_output(txout.clone());
                }
            }
        }

        if owned_count == 0 {
            return Err(PsbtValidationError::NothingToSign {
                total_inputs: psbt.inputs.len(),
                foreign_count,
                already_signed_count,
            });
        }

        Ok(template)
    }
}

/// Only a template narrowed to one key can say which input a signature belongs to.
impl TransactionTemplate<ScopedTo> {
    /// Writes each signature onto the PSBT input it was produced for.
    ///
    /// The counterpart to [`Self::from_psbt`]: the template decided which inputs were ours,
    /// so it is the only thing that can say which input each signature belongs to.
    pub fn attach_signatures_to_psbt(
        &self,
        signatures: &[EncodedSignature],
        psbt: &Psbt,
    ) -> Result<Psbt, AttachSignaturesError> {
        let pairs = self.signatures_by_input_index(signatures)?;

        let mut psbt = psbt.clone();
        for (i, signature) in pairs {
            let input = psbt
                .inputs
                .get_mut(i)
                .ok_or(AttachSignaturesError::NoSuchInput(i))?;
            input.tap_key_sig = Some(bitcoin::taproot::Signature {
                signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&signature.0)
                    .map_err(|_| AttachSignaturesError::MalformedSignature(i))?,
                sighash_type: bitcoin::sighash::TapSighashType::Default,
            });
        }

        Ok(psbt)
    }
}

#[derive(Debug, Clone)]
pub enum AttachSignaturesError {
    CountMismatch(SignatureCountMismatch),
    /// The PSBT has fewer inputs than the template it was signed against.
    NoSuchInput(usize),
    MalformedSignature(usize),
}

impl From<SignatureCountMismatch> for AttachSignaturesError {
    fn from(e: SignatureCountMismatch) -> Self {
        AttachSignaturesError::CountMismatch(e)
    }
}

impl core::fmt::Display for AttachSignaturesError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AttachSignaturesError::CountMismatch(e) => write!(f, "{e}"),
            AttachSignaturesError::NoSuchInput(i) => {
                write!(f, "this PSBT has no input {i} to sign")
            }
            AttachSignaturesError::MalformedSignature(i) => {
                write!(f, "the signature for input {i} is not a schnorr signature")
            }
        }
    }
}

impl std::error::Error for AttachSignaturesError {}

/// Why a PSBT key origin doesn't name a script this key derives.
enum NotOurs {
    NoInternalKey,
    NoOrigin,
    ForeignFingerprint,
    Hardened,
    UnusualPath(String),
}

impl core::fmt::Display for NotOurs {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NotOurs::NoInternalKey => write!(f, "it doesn't have a tap_internal_key"),
            NotOurs::NoOrigin => write!(f, "it doesn't provide a source for the tap_internal_key"),
            NotOurs::ForeignFingerprint => {
                write!(f, "its key fingerprint doesn't match our root key")
            }
            NotOurs::Hardened => write!(f, "its derivation path is hardened"),
            NotOurs::UnusualPath(path) => write!(f, "it has an unusual derivation path {path:?}"),
        }
    }
}

/// The path a PSBT claims for `tap_internal_key`, and every key of ours whose fingerprint it
/// claimed.
///
/// Shared by both loops so inputs and outputs cannot drift apart on what counts as ours.
///
/// Which candidate it really is cannot be decided here: only deriving the path and comparing
/// the result against the actual script settles it, and that is [`push_owned`]'s job.
fn claimed_path<'a>(
    tap_internal_key: Option<&XOnlyPublicKey>,
    tap_key_origins: &BTreeMap<XOnlyPublicKey, (Vec<taproot::TapLeafHash>, bip32::KeySource)>,
    ours_by_fingerprint: &'a BTreeMap<Fingerprint, Vec<MasterAppkey>>,
) -> Result<(BitcoinBip32Path, &'a [MasterAppkey]), NotOurs> {
    let tap_internal_key = tap_internal_key.ok_or(NotOurs::NoInternalKey)?;
    let (fingerprint, derivation_path) = tap_key_origins
        .get(tap_internal_key)
        .map(|(_, key_source)| key_source)
        .ok_or(NotOurs::NoOrigin)?;

    let candidates = ours_by_fingerprint
        .get(fingerprint)
        .ok_or(NotOurs::ForeignFingerprint)?;

    let path = derivation_path
        .into_iter()
        .map(|child_number| match child_number {
            bip32::ChildNumber::Normal { index } => Ok(*index),
            _ => Err(NotOurs::Hardened),
        })
        .collect::<Result<Vec<_>, _>>()?;

    let bip32_path = BitcoinBip32Path::from_u32_slice(&path).ok_or_else(|| {
        NotOurs::UnusualPath(
            path.iter()
                .map(|n| n.to_string())
                .collect::<Vec<String>>()
                .join("/"),
        )
    })?;

    Ok((bip32_path, candidates))
}

/// Pushes the script under whichever candidate key actually derives it.
///
/// A fingerprint match with no script match is a PSBT claiming one of our keys over a script
/// that key does not produce, so the last mismatch is returned rather than the script being
/// recorded as foreign — silently demoting it would let a PSBT lie about us without saying so.
fn push_owned(
    candidates: &[MasterAppkey],
    bip32_path: BitcoinBip32Path,
    mut push: impl FnMut(LocalSpk) -> Result<(), Box<SpkDoesntMatchPathError>>,
) -> Result<(), Box<SpkDoesntMatchPathError>> {
    let mut last_mismatch = None;
    for &master_appkey in candidates {
        match push(LocalSpk {
            master_appkey,
            bip32_path,
        }) {
            Ok(()) => return Ok(()),
            Err(mismatch) => last_mismatch = Some(mismatch),
        }
    }

    Err(last_mismatch.expect("a fingerprint is only in the map because a key produced it"))
}

#[derive(Debug, Clone)]
pub enum PsbtValidationError {
    NothingToSign {
        total_inputs: usize,
        foreign_count: usize,
        already_signed_count: usize,
    },
    Other(String),
}

impl From<Box<SpkDoesntMatchPathError>> for PsbtValidationError {
    fn from(e: Box<SpkDoesntMatchPathError>) -> Self {
        PsbtValidationError::Other(e.to_string())
    }
}

impl core::fmt::Display for PsbtValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PsbtValidationError::NothingToSign {
                total_inputs,
                foreign_count,
                already_signed_count,
            } => {
                let (input_word, pronoun) = if *total_inputs == 1 {
                    ("input", "it")
                } else {
                    ("inputs", "any of them")
                };

                write!(
                    f,
                    "This PSBT has {total_inputs} {input_word} but this wallet can not sign {pronoun}"
                )?;

                let mut reasons = Vec::new();
                if *foreign_count > 0 {
                    reasons.push(format!("{foreign_count} not owned by this wallet"));
                }
                if *already_signed_count > 0 {
                    reasons.push(format!("{already_signed_count} already signed"));
                }

                if !reasons.is_empty() {
                    write!(f, " ({})", reasons.join(", "))?;
                }

                write!(f, ".")
            }
            PsbtValidationError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for PsbtValidationError {}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        bitcoin_transaction::SpkOwner,
        schnorr_fun::fun::{g, G},
        tweak::{AppTweak, NormalIndex},
    };
    use bitcoin::{
        absolute::LockTime,
        bip32::{ChildNumber, DerivationPath, Fingerprint},
        hashes::Hash,
        psbt,
        transaction::Version,
        Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness,
        XOnlyPublicKey,
    };

    fn idx(n: u32) -> NormalIndex {
        NormalIndex::new(n).expect("test index is in range")
    }

    fn our_key() -> MasterAppkey {
        MasterAppkey::derive_from_rootkey(g!(2 * G).normalize())
    }

    fn sibling_key() -> MasterAppkey {
        MasterAppkey::derive_from_rootkey(g!(3 * G).normalize())
    }

    fn spk_of(key: MasterAppkey, path: BitcoinBip32Path) -> ScriptBuf {
        LocalSpk {
            master_appkey: key,
            bip32_path: path,
        }
        .spk()
    }

    fn internal_key_of(key: MasterAppkey, path: BitcoinBip32Path) -> XOnlyPublicKey {
        AppTweak::Bitcoin(path)
            .derive_xonly_key(&key.to_xpub())
            .into()
    }

    fn fingerprint_of(key: MasterAppkey) -> Fingerprint {
        key.derive_appkey(AppTweakKind::Bitcoin).fingerprint()
    }

    fn derivation_of(path: BitcoinBip32Path) -> DerivationPath {
        path.path_segments_from_bitcoin_appkey()
            .map(|i| ChildNumber::from_normal_idx(i).expect("test path is unhardened"))
            .collect()
    }

    fn txout_of(key: MasterAppkey, path: BitcoinBip32Path, value: u64) -> TxOut {
        TxOut {
            value: Amount::from_sat(value),
            script_pubkey: spk_of(key, path),
        }
    }

    fn foreign_txout(value: u64) -> TxOut {
        TxOut {
            value: Amount::from_sat(value),
            script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xab; 32][..]].concat()),
        }
    }

    /// A PSBT input annotated the way a producer that recognises the utxo would.
    fn owned_input(key: MasterAppkey, path: BitcoinBip32Path, value: u64) -> psbt::Input {
        let internal_key = internal_key_of(key, path);
        psbt::Input {
            witness_utxo: Some(txout_of(key, path, value)),
            tap_internal_key: Some(internal_key),
            tap_key_origins: [(
                internal_key,
                (vec![], (fingerprint_of(key), derivation_of(path))),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        }
    }

    fn psbt_of(inputs: Vec<psbt::Input>, outputs: Vec<TxOut>) -> Psbt {
        let annotations = vec![psbt::Output::default(); outputs.len()];
        annotated_psbt_of(inputs, outputs, annotations)
    }

    fn annotated_psbt_of(
        inputs: Vec<psbt::Input>,
        outputs: Vec<TxOut>,
        annotations: Vec<psbt::Output>,
    ) -> Psbt {
        let unsigned_tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: (0..inputs.len())
                .map(|i| TxIn {
                    previous_output: OutPoint {
                        txid: Txid::from_byte_array([7u8; 32]),
                        vout: i as u32,
                    },
                    sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                    script_sig: ScriptBuf::new(),
                    witness: Witness::new(),
                })
                .collect(),
            output: outputs,
        };
        Psbt {
            outputs: annotations,
            unsigned_tx,
            version: 0,
            xpub: Default::default(),
            proprietary: Default::default(),
            unknown: Default::default(),
            inputs,
        }
    }

    /// The `PSBT_OUT_TAP_BIP32_DERIVATION` a producer writes for an output it recognises.
    fn owned_output(key: MasterAppkey, path: BitcoinBip32Path) -> psbt::Output {
        let internal_key = internal_key_of(key, path);
        psbt::Output {
            tap_internal_key: Some(internal_key),
            tap_key_origins: [(
                internal_key,
                (vec![], (fingerprint_of(key), derivation_of(path))),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        }
    }

    fn err_string(e: PsbtValidationError) -> String {
        e.to_string()
    }

    #[test]
    fn owned_input_is_recorded_with_its_path() {
        let key = our_key();
        let path = BitcoinBip32Path::external(idx(4));
        let psbt = psbt_of(
            vec![owned_input(key, path, 100_000)],
            vec![foreign_txout(90_000)],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert_eq!(
            template.inputs()[0].owner().local_owner(),
            Some(&LocalSpk {
                master_appkey: key,
                bip32_path: path
            })
        );
    }

    #[test]
    fn already_signed_input_is_foreign_and_tallied() {
        let key = our_key();
        let path = BitcoinBip32Path::external(idx(4));
        let mut signed = owned_input(key, path, 100_000);
        signed.final_script_witness = Some(Witness::from_slice(&[[3u8; 64].as_slice()]));

        let psbt = psbt_of(vec![signed], vec![foreign_txout(90_000)]);
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        match err {
            PsbtValidationError::NothingToSign {
                total_inputs,
                foreign_count,
                already_signed_count,
            } => {
                assert_eq!(
                    (total_inputs, foreign_count, already_signed_count),
                    (1, 0, 1)
                );
            }
            other => panic!("expected NothingToSign, got {other:?}"),
        }
    }

    #[test]
    fn input_without_tap_internal_key_is_foreign() {
        let key = our_key();
        let path = BitcoinBip32Path::external(idx(4));
        let mut input = owned_input(key, path, 100_000);
        input.tap_internal_key = None;

        let psbt = psbt_of(vec![input], vec![foreign_txout(90_000)]);
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        match err {
            PsbtValidationError::NothingToSign { foreign_count, .. } => {
                assert_eq!(foreign_count, 1)
            }
            other => panic!("expected NothingToSign, got {other:?}"),
        }
    }

    #[test]
    fn input_without_origin_for_its_internal_key_is_foreign() {
        let key = our_key();
        let path = BitcoinBip32Path::external(idx(4));
        let mut input = owned_input(key, path, 100_000);
        input.tap_key_origins.clear();

        let psbt = psbt_of(vec![input], vec![foreign_txout(90_000)]);
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        match err {
            PsbtValidationError::NothingToSign { foreign_count, .. } => {
                assert_eq!(foreign_count, 1)
            }
            other => panic!("expected NothingToSign, got {other:?}"),
        }
    }

    #[test]
    fn input_with_another_wallets_fingerprint_is_foreign() {
        let key = our_key();
        let path = BitcoinBip32Path::external(idx(4));
        let internal_key = internal_key_of(key, path);
        let mut input = owned_input(key, path, 100_000);
        input.tap_key_origins.insert(
            internal_key,
            (vec![], (fingerprint_of(sibling_key()), derivation_of(path))),
        );

        let psbt = psbt_of(vec![input], vec![foreign_txout(90_000)]);
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        match err {
            PsbtValidationError::NothingToSign { foreign_count, .. } => {
                assert_eq!(foreign_count, 1)
            }
            other => panic!("expected NothingToSign, got {other:?}"),
        }
    }

    /// Why the result is unscoped rather than scoped to the key that was passed: given two of
    /// our keys, `Local` records *which* one, and only `as_seen_by` collapses that to "ours".
    #[test]
    fn each_of_our_keys_owns_its_own_input() {
        let ours = our_key();
        let sibling = sibling_key();
        let our_path = BitcoinBip32Path::external(idx(0));
        let their_path = BitcoinBip32Path::external(idx(1));

        let psbt = psbt_of(
            vec![
                owned_input(ours, our_path, 100_000),
                owned_input(sibling, their_path, 100_000),
            ],
            vec![foreign_txout(150_000)],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[ours, sibling]).unwrap();

        let owner_of = |i: usize| {
            template.inputs()[i]
                .owner()
                .local_owner()
                .map(|local| local.master_appkey)
        };
        assert_eq!(owner_of(0), Some(ours));
        assert_eq!(owner_of(1), Some(sibling));

        assert_eq!(
            template
                .as_seen_by(ours)
                .iter_our_inputs()
                .map(|(i, _, _)| i)
                .collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            template
                .as_seen_by(sibling)
                .iter_our_inputs()
                .map(|(i, _, _)| i)
                .collect::<Vec<_>>(),
            vec![1]
        );
    }

    /// The key list decides what is ours, not the PSBT's annotations: the same bytes read for
    /// one key leave the other key's input foreign.
    #[test]
    fn a_key_left_out_of_the_list_is_foreign() {
        let ours = our_key();
        let sibling = sibling_key();
        let our_path = BitcoinBip32Path::external(idx(0));
        let their_path = BitcoinBip32Path::external(idx(1));

        let psbt = psbt_of(
            vec![
                owned_input(ours, our_path, 100_000),
                owned_input(sibling, their_path, 100_000),
            ],
            vec![foreign_txout(150_000)],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[ours]).unwrap();

        assert!(template.inputs()[0].owner().local_owner().is_some());
        assert!(matches!(template.inputs()[1].owner(), SpkOwner::Foreign(_)));
    }

    #[test]
    fn nothing_to_sign_when_no_key_in_the_list_owns_an_input() {
        let stranger = MasterAppkey::derive_from_rootkey(g!(5 * G).normalize());
        let path = BitcoinBip32Path::external(idx(0));

        let psbt = psbt_of(
            vec![owned_input(stranger, path, 100_000)],
            vec![foreign_txout(90_000)],
        );
        let err = TransactionTemplate::from_psbt(&psbt, &[our_key(), sibling_key()]).unwrap_err();

        match err {
            PsbtValidationError::NothingToSign {
                total_inputs,
                foreign_count,
                already_signed_count,
            } => {
                assert_eq!(
                    (total_inputs, foreign_count, already_signed_count),
                    (1, 1, 0)
                );
            }
            other => panic!("expected NothingToSign, got {other:?}"),
        }
    }

    #[test]
    fn input_whose_claimed_path_derives_a_different_spk_is_rejected() {
        let key = our_key();
        let claimed = BitcoinBip32Path::external(idx(4));
        let actual = BitcoinBip32Path::external(idx(5));

        let mut input = owned_input(key, claimed, 100_000);
        input.witness_utxo = Some(txout_of(key, actual, 100_000));

        let psbt = psbt_of(vec![input], vec![foreign_txout(90_000)]);
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        assert!(
            err_string(err).contains("didn't match what we expected"),
            "expected a SpkDoesntMatchPathError"
        );
    }

    #[test]
    fn input_with_a_hardened_child_is_rejected() {
        let key = our_key();
        let path = BitcoinBip32Path::external(idx(4));
        let internal_key = internal_key_of(key, path);
        let mut input = owned_input(key, path, 100_000);
        input.tap_key_origins.insert(
            internal_key,
            (
                vec![],
                (
                    fingerprint_of(key),
                    vec![
                        ChildNumber::from_normal_idx(0).unwrap(),
                        ChildNumber::from_normal_idx(0).unwrap(),
                        ChildNumber::from_normal_idx(0).unwrap(),
                        ChildNumber::from_hardened_idx(4).unwrap(),
                    ]
                    .into_iter()
                    .collect::<DerivationPath>(),
                ),
            ),
        );

        let psbt = psbt_of(vec![input], vec![foreign_txout(90_000)]);
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        assert!(
            err_string(err).contains("hardened"),
            "expected the hardened-path rejection"
        );
    }

    #[test]
    fn input_missing_both_utxos_is_rejected() {
        let key = our_key();
        let path = BitcoinBip32Path::external(idx(4));
        let mut input = owned_input(key, path, 100_000);
        input.witness_utxo = None;
        input.non_witness_utxo = None;

        let psbt = psbt_of(vec![input], vec![foreign_txout(90_000)]);
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        assert!(
            err_string(err).contains("missing witness and non-witness utxo"),
            "expected the missing-utxo rejection"
        );
    }

    #[test]
    fn our_output_is_owned_and_a_stranger_is_foreign() {
        let key = our_key();
        let input_path = BitcoinBip32Path::external(idx(4));
        let change_path = BitcoinBip32Path::internal(idx(1));
        let stranger = foreign_txout(40_000);

        let psbt = annotated_psbt_of(
            vec![owned_input(key, input_path, 100_000)],
            vec![txout_of(key, change_path, 50_000), stranger.clone()],
            vec![owned_output(key, change_path), psbt::Output::default()],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert_eq!(
            template.outputs()[0].owner().local_owner(),
            Some(&LocalSpk {
                master_appkey: key,
                bip32_path: change_path
            })
        );
        assert_eq!(
            template.outputs()[1].owner(),
            &SpkOwner::Foreign(stranger.script_pubkey)
        );
    }

    /// A sibling wallet's output is money leaving this key. Labelling it ours signs a
    /// transaction paying a different key than the PSBT asked for — #552.
    #[test]
    fn output_belonging_to_a_sibling_wallet_is_foreign() {
        let key = our_key();
        let sibling = sibling_key();
        let input_path = BitcoinBip32Path::external(idx(4));
        let sibling_path = BitcoinBip32Path::external(idx(9));
        let sibling_txout = txout_of(sibling, sibling_path, 50_000);

        let psbt = annotated_psbt_of(
            vec![owned_input(key, input_path, 100_000)],
            vec![sibling_txout.clone()],
            vec![owned_output(sibling, sibling_path)],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert_eq!(
            template.outputs()[0].owner(),
            &SpkOwner::Foreign(sibling_txout.script_pubkey.clone()),
        );
        assert_eq!(
            template.to_rust_bitcoin_tx().output[0],
            sibling_txout,
            "the signed transaction must pay what the PSBT asked for"
        );
    }

    #[test]
    fn nothing_to_sign_counts_each_reason_separately() {
        let key = our_key();
        let ours = BitcoinBip32Path::external(idx(4));

        let mut signed = owned_input(key, ours, 100_000);
        signed.final_script_witness = Some(Witness::from_slice(&[[3u8; 64].as_slice()]));

        let mut no_internal_key = owned_input(key, BitcoinBip32Path::external(idx(5)), 100_000);
        no_internal_key.tap_internal_key = None;

        let mut stranger = owned_input(key, BitcoinBip32Path::external(idx(6)), 100_000);
        let stranger_internal = internal_key_of(key, BitcoinBip32Path::external(idx(6)));
        stranger.tap_key_origins.insert(
            stranger_internal,
            (
                vec![],
                (
                    fingerprint_of(sibling_key()),
                    derivation_of(BitcoinBip32Path::external(idx(6))),
                ),
            ),
        );

        let psbt = psbt_of(
            vec![signed, no_internal_key, stranger],
            vec![foreign_txout(250_000)],
        );
        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        match err {
            PsbtValidationError::NothingToSign {
                total_inputs,
                foreign_count,
                already_signed_count,
            } => {
                assert_eq!(
                    (total_inputs, foreign_count, already_signed_count),
                    (3, 2, 1)
                );
                assert_eq!(
                    PsbtValidationError::NothingToSign {
                        total_inputs,
                        foreign_count,
                        already_signed_count
                    }
                    .to_string(),
                    "This PSBT has 3 inputs but this wallet can not sign any of them \
                     (2 not owned by this wallet, 1 already signed).",
                );
            }
            other => panic!("expected NothingToSign, got {other:?}"),
        }
    }

    #[test]
    fn output_without_tap_internal_key_is_foreign() {
        let key = our_key();
        let path = BitcoinBip32Path::internal(idx(1));
        let ours = txout_of(key, path, 50_000);
        let mut annotation = owned_output(key, path);
        annotation.tap_internal_key = None;

        let psbt = annotated_psbt_of(
            vec![owned_input(
                key,
                BitcoinBip32Path::external(idx(4)),
                100_000,
            )],
            vec![ours.clone()],
            vec![annotation],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert_eq!(
            template.outputs()[0].owner(),
            &SpkOwner::Foreign(ours.script_pubkey)
        );
    }

    #[test]
    fn output_without_origin_for_its_internal_key_is_foreign() {
        let key = our_key();
        let path = BitcoinBip32Path::internal(idx(1));
        let ours = txout_of(key, path, 50_000);
        let mut annotation = owned_output(key, path);
        annotation.tap_key_origins.clear();

        let psbt = annotated_psbt_of(
            vec![owned_input(
                key,
                BitcoinBip32Path::external(idx(4)),
                100_000,
            )],
            vec![ours.clone()],
            vec![annotation],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert_eq!(
            template.outputs()[0].owner(),
            &SpkOwner::Foreign(ours.script_pubkey)
        );
    }

    #[test]
    fn output_whose_claimed_path_derives_a_different_spk_is_rejected() {
        let key = our_key();
        let claimed = BitcoinBip32Path::internal(idx(1));
        let actual = BitcoinBip32Path::internal(idx(2));

        let psbt = annotated_psbt_of(
            vec![owned_input(
                key,
                BitcoinBip32Path::external(idx(4)),
                100_000,
            )],
            vec![txout_of(key, actual, 50_000)],
            vec![owned_output(key, claimed)],
        );

        let err = TransactionTemplate::from_psbt(&psbt, &[key]).unwrap_err();

        assert!(
            err_string(err).contains("didn't match what we expected"),
            "expected a SpkDoesntMatchPathError"
        );
    }

    /// Past any lookahead an indexer would have derived, which is what sourcing the path
    /// from the PSBT buys: the index could only ever answer for spks it had already seen.
    #[test]
    fn output_far_past_any_lookahead_is_still_ours() {
        let key = our_key();
        let deep = BitcoinBip32Path::external(idx(500_000));

        let psbt = annotated_psbt_of(
            vec![owned_input(
                key,
                BitcoinBip32Path::external(idx(4)),
                100_000,
            )],
            vec![txout_of(key, deep, 50_000)],
            vec![owned_output(key, deep)],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert_eq!(
            template.outputs()[0].owner().local_owner(),
            Some(&LocalSpk {
                master_appkey: key,
                bip32_path: deep
            })
        );
    }

    #[test]
    fn output_with_a_hardened_child_is_foreign_rather_than_fatal() {
        let key = our_key();
        let path = BitcoinBip32Path::internal(idx(1));
        let ours = txout_of(key, path, 50_000);
        let internal_key = internal_key_of(key, path);
        let mut annotation = owned_output(key, path);
        annotation.tap_key_origins.insert(
            internal_key,
            (
                vec![],
                (
                    fingerprint_of(key),
                    vec![
                        ChildNumber::from_normal_idx(0).unwrap(),
                        ChildNumber::from_normal_idx(0).unwrap(),
                        ChildNumber::from_normal_idx(1).unwrap(),
                        ChildNumber::from_hardened_idx(1).unwrap(),
                    ]
                    .into_iter()
                    .collect::<DerivationPath>(),
                ),
            ),
        );

        let psbt = annotated_psbt_of(
            vec![owned_input(
                key,
                BitcoinBip32Path::external(idx(4)),
                100_000,
            )],
            vec![ours.clone()],
            vec![annotation],
        );

        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert_eq!(
            template.outputs()[0].owner(),
            &SpkOwner::Foreign(ours.script_pubkey)
        );
    }

    fn signature(tag: u8) -> crate::message::EncodedSignature {
        let mut bytes = [1u8; 64];
        bytes[0] = tag;
        crate::message::EncodedSignature(bytes)
    }

    /// A PSBT input we cannot sign — no origin for its key, so the template calls it foreign.
    fn foreign_input(value: u64, tag: u8) -> psbt::Input {
        psbt::Input {
            witness_utxo: Some(TxOut {
                value: Amount::from_sat(value),
                script_pubkey: ScriptBuf::from_bytes(
                    [&[0x51u8, 0x20][..], &[tag; 32][..]].concat(),
                ),
            }),
            ..Default::default()
        }
    }

    /// The signature is for the *second* input. Placing it positionally would sign the
    /// foreign one and leave ours bare.
    #[test]
    fn attaching_skips_a_foreign_input() {
        let key = our_key();
        let ours = BitcoinBip32Path::external(idx(4));
        let psbt = psbt_of(
            vec![
                foreign_input(100_000, 0xaa),
                owned_input(key, ours, 100_000),
            ],
            vec![foreign_txout(150_000)],
        );
        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        let signed = template
            .attach_signatures_to_psbt(&[signature(7)], &psbt)
            .unwrap();

        assert!(
            signed.inputs[0].tap_key_sig.is_none(),
            "the foreign input must not receive our signature"
        );
        assert_eq!(
            signed.inputs[1].tap_key_sig.unwrap().signature.serialize()[0],
            7,
            "our signature belongs on input 1"
        );
    }

    #[test]
    fn attaching_places_each_signature_among_interleaved_foreign_inputs() {
        let key = our_key();
        let psbt = psbt_of(
            vec![
                foreign_input(100_000, 0xaa),
                owned_input(key, BitcoinBip32Path::external(idx(1)), 100_000),
                owned_input(key, BitcoinBip32Path::external(idx(2)), 100_000),
                foreign_input(100_000, 0xbb),
            ],
            vec![foreign_txout(350_000)],
        );
        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        let signed = template
            .attach_signatures_to_psbt(&[signature(1), signature(2)], &psbt)
            .unwrap();

        assert!(signed.inputs[0].tap_key_sig.is_none());
        assert_eq!(
            signed.inputs[1].tap_key_sig.unwrap().signature.serialize()[0],
            1
        );
        assert_eq!(
            signed.inputs[2].tap_key_sig.unwrap().signature.serialize()[0],
            2
        );
        assert!(signed.inputs[3].tap_key_sig.is_none());
    }

    #[test]
    fn attaching_the_wrong_number_of_signatures_is_refused() {
        let key = our_key();
        let psbt = psbt_of(
            vec![
                foreign_input(100_000, 0xaa),
                owned_input(key, BitcoinBip32Path::external(idx(1)), 100_000),
            ],
            vec![foreign_txout(150_000)],
        );
        let template = TransactionTemplate::from_psbt(&psbt, &[key])
            .unwrap()
            .as_seen_by(key);

        assert!(template.attach_signatures_to_psbt(&[], &psbt).is_err());
        assert!(template
            .attach_signatures_to_psbt(&[signature(1), signature(2)], &psbt)
            .is_err());
    }
}
