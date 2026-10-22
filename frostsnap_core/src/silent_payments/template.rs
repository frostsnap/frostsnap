//! Silent payment outputs of a [`TransactionTemplate`]: adding them, and deriving their scripts
//! from the threshold ECDH.

use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};
use bitcoin::{
    key::{TweakedPublicKey, XOnlyPublicKey},
    ScriptBuf,
};
use schnorr_fun::{
    frost::ShareIndex,
    fun::{marker::*, Point, Scalar},
};

use super::{
    a_sum_public, combine_ecdh_shares, input_hash, output_keys, smallest_outpoint, EcdhShare,
    SilentPaymentRecipient,
};
use crate::{
    bitcoin_transaction::{Output, SilentPaymentOutput, SpkOwner, TransactionTemplate},
    tweak::{BitcoinAccount, TweakContext},
    MasterAppkey,
};

impl TransactionTemplate {
    /// A silent payment to `recipient`, whose script is filled in by [`Self::with_sp_scripts`]
    /// once the ECDH shares are combined. `address` is the recipient's address as the user gave
    /// it, shown to the signer.
    pub fn push_silent_payment_output(
        &mut self,
        value: bitcoin::Amount,
        recipient: SilentPaymentRecipient,
        address: String,
    ) {
        self.outputs.push(Output {
            owner: SpkOwner::SilentPayment(Box::new(SilentPaymentOutput {
                recipient,
                address,
                script: None,
            })),
            value: value.to_sat(),
        });
    }

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
            .map(|sp| sp.recipient.scan_key)
            .filter(|scan_key| seen.insert(*scan_key))
            .collect()
    }

    /// The BIP352 `input_hash` and the tweak context of every input, for silent payment ECDH
    /// under `master_appkey`. `None` unless every input is a coin of that key in the default
    /// account, since the combined ECDH is only defined over one account's input secrets.
    pub fn sp_ecdh_inputs(
        &self,
        master_appkey: MasterAppkey,
    ) -> Option<(Scalar<Public>, Vec<TweakContext>)> {
        let account = BitcoinAccount::default();
        let account_key = account.derive_account_key(&master_appkey.to_xpub());
        let mut outpoints = Vec::new();
        let mut tweaks = Vec::new();
        for input in self.inputs() {
            let owner = input.owner().local_owner()?;
            if owner.master_appkey != master_appkey
                || owner.bip32_path.account_keychain.account != account
            {
                return None;
            }
            outpoints.push(input.outpoint());
            tweaks.push(owner.bip32_path.tweak_context(&account_key));
        }
        let smallest = smallest_outpoint(&outpoints)?;
        let a_sum = a_sum_public(*account_key.key(), &tweaks)?;
        Some((input_hash(&smallest, a_sum)?, tweaks))
    }

    /// The shared secret of each scan key, from threshold shares of the ECDH.
    pub fn sp_shared_secrets(
        &self,
        master_appkey: MasterAppkey,
        shares: &[EcdhShare],
    ) -> Option<BTreeMap<Point, Point>> {
        let (input_hash, tweaks) = self.sp_ecdh_inputs(master_appkey)?;
        let mut by_scan_key = BTreeMap::<Point, Vec<(ShareIndex, Point)>>::new();
        for share in shares {
            by_scan_key
                .entry(share.scan_key)
                .or_default()
                .push((share.share_index, share.ecdh_share));
        }
        by_scan_key
            .into_iter()
            .map(|(scan_key, shares)| {
                let secret = combine_ecdh_shares(input_hash, scan_key, &tweaks, &shares)?;
                Some((scan_key, secret))
            })
            .collect()
    }

    /// This template with every silent payment output's script derived from `shared_secrets`.
    pub fn with_sp_scripts(&self, shared_secrets: &BTreeMap<Point, Point>) -> Option<Self> {
        let recipients = self
            .silent_payment_outputs()
            .map(|sp| sp.recipient)
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
