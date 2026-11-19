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
                .iter_our_outputs()
                .filter_map(move |(_, _, path)| {
                    (path.account_keychain == internal).then_some(path.index.to_u32())
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
                // Outputs only. A key-path input is self-certifying: `Input::txout` builds the
                // prevout from the claimed path's own spk, so the sighash commits to it and a
                // forged path signs against a prevout that is not on chain. A script-path input
                // is not: its prevout spk is the coordinator's word. What binds it is the leaf
                // sighash, which commits to every prevout's spk and amount and to the leaf hash,
                // so a signature over a prevout or leaf that is not the real one is valid for
                // nothing. Policing inputs would buy nothing and would strand coins a PSBT
                // legitimately imports.
                //
                // The known cost is that only this side is bounded: an honest coordinator that
                // allocated change or revealed an address past the ceiling would have its own task
                // refused here. Reaching that is implausible today, and closing it properly means
                // one shared issuance boundary in the wallet, which is the cleanup.
                for (_, _, path) in tx_template.iter_our_outputs() {
                    if path.account_keychain.account != BitcoinAccount::default() {
                        return Err(SignTaskError::UnwatchedAccount {
                            account: path.account_keychain.account,
                        });
                    }

                    if path.index.to_u32() >= OUTPUT_INDEX_LIMIT {
                        return Err(SignTaskError::OutputIndexOutOfRange { index: path.index });
                    }
                }

                // The sighash binds a script-path input's leaf but not which leaves we are given
                // or their control blocks, and both size the witness and so the feerate we show.
                // A padded control block, or a big leaf of the coin that holds no key of ours,
                // could make any fee look modest; each leaf must be one we can sign.
                for (input, _, owner) in tx_template.iter_our_inputs() {
                    if !owner
                        .leaves()
                        .iter()
                        .all(|leaf| leaf.is_ours_in(owner.master_appkey, &owner.spk()))
                    {
                        return Err(SignTaskError::UnprovenLeaf { input });
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
                .iter_our_input_sighashes()
                .map(|(app_tweak, sighash)| SignItem {
                    message: sighash.as_raw_hash().to_byte_array().to_vec(),
                    app_tweak,
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
    UnprovenLeaf { input: usize },
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
            SignTaskError::UnprovenLeaf { input } => write!(
                f,
                "input {input} has a leaf that holds no key of ours or is not committed to the coin it spends"
            ),
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
    use crate::bitcoin_transaction::{LocalSpk, TransactionTemplate};
    use crate::bitcoin_transaction::{PromptDestination, PromptRecipient};
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
        let their_spk = LocalSpk::key_spend(other, BitcoinBip32Path::internal(NormalIndex::ZERO));

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk::key_spend(signing, BitcoinBip32Path::external(NormalIndex::ZERO)),
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
            tx_template.iter_our_outputs().count(),
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
                destination: PromptDestination::Address(their_address),
                amount: Amount::from_sat(90_000),
                owned: None,
            }],
            "their output is disclosed at their address, and not as ours"
        );
        assert_eq!(prompt.fee, Amount::from_sat(10_000));
    }

    /// Only a plain BIP86 output is ours. A coordinator could otherwise claim an output it
    /// controls as our change — by naming any script as a script-path owner, or a script tree
    /// as a key-spend's merkle root — and the device would hide the payment.
    #[test]
    fn a_scripted_output_claimed_as_ours_is_shown_as_leaving() {
        let signing = signing_key();
        let change_path = BitcoinBip32Path::internal(NormalIndex::new(1).unwrap());
        let attacker_spk = ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xee; 32][..]].concat());

        for disguised in [
            LocalSpk::script_spend(
                signing,
                attacker_spk.clone(),
                alloc::vec![crate::bitcoin_transaction::LeafSpend::new(
                    change_path,
                    ScriptBuf::new(),
                    bitcoin::taproot::LeafVersion::TapScript,
                    alloc::vec![],
                )],
            ),
            LocalSpk::key_spend_with_merkle_root(
                signing,
                change_path,
                Some(bitcoin::taproot::TapNodeHash::from_byte_array([0xee; 32])),
            ),
        ] {
            let mut tx_template = TransactionTemplate::new();
            tx_template.push_imaginary_owned_input(
                LocalSpk::key_spend(signing, BitcoinBip32Path::external(NormalIndex::ZERO)),
                Amount::from_sat(100_000),
            );
            tx_template.push_foreign_output(TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: ScriptBuf::from_bytes(
                    [&[0x51u8, 0x20][..], &[0xcc; 32][..]].concat(),
                ),
            });
            // The shape of an ordinary send's single change output, which the prompt hides.
            tx_template.push_owned_output(Amount::from_sat(49_000), disguised.clone());

            let checked = WireSignTask::BitcoinTransaction(tx_template)
                .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin))
                .expect("the task is signable; it just isn't change");
            let SignTask::BitcoinTransaction { tx_template, .. } = &checked.inner else {
                panic!("expected a bitcoin task")
            };
            assert_eq!(
                tx_template.to_rust_bitcoin_tx().output[1].script_pubkey,
                disguised.spk(),
                "the signed transaction pays the disguised script"
            );
            assert_eq!(tx_template.iter_our_outputs().count(), 0);

            let prompt = tx_template.user_prompt(Network::Bitcoin);
            let address =
                bitcoin::Address::from_script(&disguised.spk(), bitcoin::params::Params::MAINNET)
                    .expect("p2tr has an address");
            assert_eq!(
                prompt.recipients.get(1),
                Some(&PromptRecipient {
                    destination: PromptDestination::Address(address),
                    amount: Amount::from_sat(49_000),
                    owned: None,
                }),
                "the disguised output is shown, as a recipient and not as ours"
            );
        }
    }

    /// The sighash does not cover a script-path input's control block, or whether its leaf holds
    /// our key, yet both size the witness behind the feerate shown. A coordinator padding a
    /// control block, forging one that commits to nothing, or handing over a real leaf that is
    /// not ours, must be refused rather than shown a flattering rate. Spliced into the
    /// encoding, because pushing such an input is already refused on this side.
    #[test]
    fn a_leaf_we_cannot_sign_is_refused() {
        use bitcoin::taproot::{LeafVersion, TaprootBuilder};
        let signing = signing_key();
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let leaf_path = BitcoinBip32Path::external(NormalIndex::new(3).unwrap());
        let leaf_key: bitcoin::XOnlyPublicKey =
            crate::tweak::AppTweak::Bitcoin(crate::tweak::BitcoinTweak::ScriptSpend {
                leaf_path: leaf_path.into(),
            })
            .derive_xonly_key(&signing.to_xpub())
            .into();
        let leaf_script = bitcoin::script::Builder::new()
            .push_x_only_key(&leaf_key)
            .push_opcode(bitcoin::opcodes::all::OP_CHECKSIG)
            .into_script();
        let internal_key = bitcoin::secp256k1::Keypair::from_seckey_slice(&secp, &[9u8; 32])
            .unwrap()
            .x_only_public_key()
            .0;
        // A real leaf of the same coin, the same size as ours, that holds someone else's key.
        let stranger_leaf = bitcoin::script::Builder::new()
            .push_x_only_key(&internal_key)
            .push_opcode(bitcoin::opcodes::all::OP_CHECKSIG)
            .into_script();
        let spend_info = TaprootBuilder::new()
            .add_leaf(1, leaf_script.clone())
            .unwrap()
            .add_leaf(1, stranger_leaf.clone())
            .unwrap()
            .finalize(&secp, internal_key)
            .unwrap();
        let stranger_control_block = spend_info
            .control_block(&(stranger_leaf.clone(), LeafVersion::TapScript))
            .unwrap()
            .serialize();
        let control_block = spend_info
            .control_block(&(leaf_script.clone(), LeafVersion::TapScript))
            .unwrap()
            .serialize();

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk::script_spend(
                signing,
                ScriptBuf::new_p2tr(&secp, internal_key, spend_info.merkle_root()),
                alloc::vec![crate::bitcoin_transaction::LeafSpend::new(
                    leaf_path,
                    leaf_script.clone(),
                    LeafVersion::TapScript,
                    control_block.clone(),
                )],
            ),
            Amount::from_sat(100_000),
        );
        tx_template.push_foreign_output(TxOut {
            value: Amount::from_sat(10_000),
            script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xcc; 32][..]].concat()),
        });
        let cfg = bincode::config::standard();
        let bytes =
            bincode::encode_to_vec(WireSignTask::BitcoinTransaction(tx_template), cfg).unwrap();
        let encoded_control_block = bincode::encode_to_vec(&control_block, cfg).unwrap();
        let at = bytes
            .windows(encoded_control_block.len())
            .position(|w| w == encoded_control_block)
            .expect("the control block is in the encoding");

        let splice = |replacement: Vec<u8>| {
            let replacement = bincode::encode_to_vec(&replacement, cfg).unwrap();
            let patched = [
                &bytes[..at],
                &replacement[..],
                &bytes[at + encoded_control_block.len()..],
            ]
            .concat();
            let (task, _): (WireSignTask, _) = bincode::decode_from_slice(&patched, cfg).unwrap();
            task.check(signing, KeyPurpose::Bitcoin(Network::Bitcoin))
        };

        assert!(
            splice(control_block.clone()).is_ok(),
            "the honest one is signed"
        );
        let padded = [&control_block[..], &[0xab; 32 * 8]].concat();
        let mut wrong_parity = control_block.clone();
        wrong_parity[0] ^= 1;
        let mut other_internal_key = control_block.clone();
        other_internal_key[1..33].copy_from_slice(&leaf_key.serialize());
        for forged in [
            padded,
            wrong_parity,
            other_internal_key,
            alloc::vec![0xc0; 40],
        ] {
            assert!(matches!(
                splice(forged),
                Err(SignTaskError::UnprovenLeaf { input: 0 })
            ));
        }

        // The stranger's leaf with its own, honest control block: committed to the coin, but
        // not ours to sign, so it must not be signed or weighed into the feerate.
        assert_eq!(stranger_leaf.len(), leaf_script.len());
        assert_eq!(stranger_control_block.len(), control_block.len());
        let swap = |bytes: &[u8], from: &[u8], to: &[u8]| {
            let at = bytes
                .windows(from.len())
                .position(|w| w == from)
                .expect("present in the encoding");
            [&bytes[..at], to, &bytes[at + from.len()..]].concat()
        };
        let patched = swap(&bytes, leaf_script.as_bytes(), stranger_leaf.as_bytes());
        let patched = swap(&patched, &control_block, &stranger_control_block);
        let (task, _): (WireSignTask, _) = bincode::decode_from_slice(&patched, cfg).unwrap();
        assert!(matches!(
            task.check(signing, KeyPurpose::Bitcoin(Network::Bitcoin)),
            Err(SignTaskError::UnprovenLeaf { input: 0 })
        ));
    }

    /// An OP_RETURN output is legal and a PSBT can carry one. `Address::from_script` has no
    /// rendering for it, so building the prompt used to panic — on the device.
    #[test]
    fn an_output_with_no_address_form_is_shown_rather_than_fatal() {
        let signing = signing_key();

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk::key_spend(signing, BitcoinBip32Path::external(NormalIndex::ZERO)),
            Amount::from_sat(100_000),
        );
        let op_return = ScriptBuf::from_bytes(vec![0x6a, 0x04, 0xde, 0xad, 0xbe, 0xef]);
        tx_template.push_foreign_output(TxOut {
            value: Amount::from_sat(0),
            script_pubkey: op_return.clone(),
        });
        tx_template.push_foreign_output(TxOut {
            value: Amount::from_sat(90_000),
            script_pubkey: ScriptBuf::from_bytes([&[0x51u8, 0x20][..], &[0xcc; 32][..]].concat()),
        });

        let checked = WireSignTask::BitcoinTransaction(tx_template)
            .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin))
            .expect("an unaddressable output is not grounds for refusing to sign");

        let SignTask::BitcoinTransaction { tx_template, .. } = &checked.inner else {
            panic!("expected a bitcoin task")
        };

        let prompt = tx_template.user_prompt(Network::Bitcoin);

        assert_eq!(
            prompt.recipients.len(),
            2,
            "both outputs are disclosed: one the signer cannot see is worse than an awkward one"
        );
        assert!(
            matches!(
                &prompt.recipients[0].destination,
                PromptDestination::UnrecognizedScript(spk) if *spk == op_return
            ),
            "the script we cannot render is carried, not dropped"
        );
        assert!(
            matches!(
                &prompt.recipients[1].destination,
                PromptDestination::Address(_)
            ),
            "the payment still renders as an address"
        );
    }

    /// The case that keeps the variant honest. A bare multisig has no address form either, but
    /// it is perfectly spendable — so `UnrecognizedScript` must mean "we cannot render this",
    /// never "nobody can spend this".
    #[test]
    fn a_spendable_script_with_no_address_form_is_also_unrecognized() {
        let signing = signing_key();

        let mut bare = alloc::vec![0x51u8, 0x21];
        bare.extend_from_slice(&[0x02; 33]);
        bare.extend_from_slice(&[0x51, 0xae]);
        let bare_multisig = ScriptBuf::from_bytes(bare);
        assert!(
            !bare_multisig.is_op_return(),
            "this output is spendable, which is the whole point of the case"
        );

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk::key_spend(signing, BitcoinBip32Path::external(NormalIndex::ZERO)),
            Amount::from_sat(100_000),
        );
        tx_template.push_foreign_output(TxOut {
            value: Amount::from_sat(90_000),
            script_pubkey: bare_multisig.clone(),
        });

        let checked = WireSignTask::BitcoinTransaction(tx_template)
            .check(signing, KeyPurpose::Bitcoin(Network::Bitcoin))
            .expect("a script we cannot render is not grounds for refusing to sign");

        let SignTask::BitcoinTransaction { tx_template, .. } = &checked.inner else {
            panic!("expected a bitcoin task")
        };

        assert_eq!(
            tx_template.user_prompt(Network::Bitcoin).recipients,
            alloc::vec![PromptRecipient {
                destination: PromptDestination::UnrecognizedScript(bare_multisig),
                amount: Amount::from_sat(90_000),
                owned: None,
            }],
            "carried as a script we cannot show, with its real amount"
        );
    }

    /// An input under another key is one we cannot sign, so it must not be counted as ours —
    /// leaving nothing here to sign at all.
    #[test]
    fn an_input_under_another_key_is_not_ours_to_sign() {
        let signing = signing_key();
        let other = other_key();

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk::key_spend(other, BitcoinBip32Path::external(NormalIndex::ZERO)),
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

    /// A send to an external recipient is accepted: the owner check only applies to outputs
    /// claiming to be ours.
    #[test]
    fn external_recipient_output_is_accepted() {
        let signing = signing_key();

        let mut tx_template = TransactionTemplate::new();
        tx_template.push_imaginary_owned_input(
            LocalSpk::key_spend(signing, BitcoinBip32Path::external(NormalIndex::ZERO)),
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
                LocalSpk::key_spend(signing, input),
                Amount::from_sat(100_000),
            );
            tx.push_owned_output(
                Amount::from_sat(90_000),
                LocalSpk::key_spend(signing, output),
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
