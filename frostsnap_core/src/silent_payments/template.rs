//! Silent payment outputs of a [`TransactionTemplate`]: adding them, and deriving their scripts
//! from the signed contact each one pays.

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use bitcoin::{
    key::{TweakedPublicKey, XOnlyPublicKey},
    ScriptBuf,
};
use schnorr_fun::fun::{marker::*, Point, Scalar};

use super::{
    a_sum_public, input_hash, output_keys, shared_secret, smallest_outpoint, SignedSpContact,
};
use crate::{
    bitcoin_transaction::{
        Output, ScopedTo, SilentPaymentOutput, SpkOwner, TransactionTemplate, Unscoped,
    },
    tweak::{BitcoinAccount, TweakContext},
};

impl TransactionTemplate<Unscoped> {
    /// A silent payment to a signed contact, whose script is filled in by
    /// [`Self::with_contact_sp_scripts`] once the inputs are final. `address` is the contact's
    /// address, shown to the signer.
    pub fn push_silent_payment_output(
        &mut self,
        value: bitcoin::Amount,
        contact: SignedSpContact,
        address: alloc::string::String,
    ) {
        self.outputs.push(Output {
            owner: SpkOwner::SilentPayment(Box::new(SilentPaymentOutput {
                contact,
                address,
                script: None,
            })),
            value: value.to_sat(),
        });
    }

    /// This template with every silent payment script derived from the contacts it pays and the
    /// inputs of `master_appkey`.
    pub fn with_contact_sp_scripts(&self, master_appkey: crate::MasterAppkey) -> Option<Self> {
        let shared_secrets = self.as_seen_by(master_appkey).sp_shared_secrets()?;
        self.with_sp_scripts(&shared_secrets)
    }
}

impl<S: Clone> TransactionTemplate<S> {
    /// The silent payment outputs in transaction order.
    pub fn silent_payment_outputs(&self) -> impl Iterator<Item = &SilentPaymentOutput> {
        self.outputs()
            .iter()
            .filter_map(|output| match &output.owner {
                SpkOwner::SilentPayment(sp) => Some(sp.as_ref()),
                _ => None,
            })
    }

    pub fn has_silent_payment_outputs(&self) -> bool {
        self.silent_payment_outputs().next().is_some()
    }

    /// The distinct scan keys of the silent payment outputs, in order of first appearance.
    pub fn silent_payment_scan_keys(&self) -> Vec<Point> {
        let mut seen = BTreeSet::new();
        self.silent_payment_outputs()
            .map(|sp| sp.recipient().scan_key)
            .filter(|scan_key| seen.insert(*scan_key))
            .collect()
    }

    /// This template with every silent payment output's script derived from `shared_secrets`.
    pub fn with_sp_scripts(&self, shared_secrets: &BTreeMap<Point, Point>) -> Option<Self> {
        let recipients = self
            .silent_payment_outputs()
            .map(|sp| sp.recipient())
            .collect::<Vec<_>>();
        let mut keys = output_keys(&recipients, shared_secrets)?.into_iter();

        let mut template = self.clone();
        for output in &mut template.outputs {
            if let SpkOwner::SilentPayment(sp) = &mut output.owner {
                let key = XOnlyPublicKey::from_slice(&keys.next()?).ok()?;
                sp.script = Some(ScriptBuf::new_p2tr_tweaked(
                    TweakedPublicKey::dangerous_assume_tweaked(key),
                ));
            }
        }
        Some(template)
    }
}

/// Which inputs are ours is only known once the template names a key.
impl TransactionTemplate<ScopedTo> {
    /// The BIP352 `input_hash` and the tweak context of every input, for silent payment ECDH.
    /// `None` unless every input is a coin of this key in the default account, since the combined
    /// ECDH is only defined over one account's input secrets.
    pub fn sp_ecdh_inputs(&self) -> Option<(Scalar<Public>, Vec<TweakContext>)> {
        let account = BitcoinAccount::default();
        let account_key = account.derive_account_key(&self.master_appkey().to_xpub());
        let mut outpoints = Vec::new();
        let mut tweaks = Vec::new();
        for input in self.inputs() {
            let owner = input.owner().local_owner()?;
            if owner.bip32_path.account_keychain.account != account {
                return None;
            }
            outpoints.push(input.outpoint());
            tweaks.push(owner.bip32_path.tweak_context(&account_key));
        }
        let smallest = smallest_outpoint(&outpoints)?;
        let a_sum = a_sum_public(*account_key.key(), &tweaks)?;
        Some((input_hash(&smallest, a_sum)?, tweaks))
    }

    /// The shared secret of each scan key, from the `x * B_scan` of the contacts paid. `None` if
    /// two contacts disagree about a scan key's value.
    pub fn sp_shared_secrets(&self) -> Option<BTreeMap<Point, Point>> {
        let (input_hash, tweaks) = self.sp_ecdh_inputs()?;
        let mut account_ecdh = BTreeMap::<Point, Point>::new();
        for sp in self.silent_payment_outputs() {
            let scan_key = sp.recipient().scan_key;
            let value = sp.contact.contact.account_ecdh;
            if *account_ecdh.entry(scan_key).or_insert(value) != value {
                return None;
            }
        }
        account_ecdh
            .into_iter()
            .map(|(scan_key, value)| {
                let secret = shared_secret(input_hash, scan_key, &tweaks, value)?;
                Some((scan_key, secret))
            })
            .collect()
    }
}
