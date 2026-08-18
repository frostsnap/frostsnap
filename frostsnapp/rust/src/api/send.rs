//! The send flow's plan-then-commit boundary as Dart drives it: confirming the amount builds a
//! [`SendPlan`] (see [`super::transaction::BuildTxState::try_finish`]), the review UI reads its
//! numbers, and signing passes it back through [`SuperWallet::commit_send`]. The plan is a pure
//! value — holding, dropping, or rebuilding one costs the wallet nothing.

use bitcoin::OutPoint;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::bitcoin::send as coord_send;
use frostsnap_coordinator::frostsnap_core::tweak::BitcoinAccount;

use super::coordinator::Coordinator;
use super::signing::UnsignedTx;
use super::super_wallet::SuperWallet;
use crate::frb_generated::RustAutoOpaque;

/// A finished coin selection that has reserved nothing.
#[frb(opaque)]
pub struct SendPlan(pub(crate) coord_send::SendPlan);

impl SendPlan {
    #[frb(sync, type_64bit_int)]
    pub fn fee(&self) -> u64 {
        self.0.fee()
    }

    #[frb(sync, type_64bit_int)]
    pub fn change_value(&self) -> Option<u64> {
        self.0.change_value()
    }

    /// What the plan pays this recipient. The review screen must read the amount from here
    /// rather than re-asking the wallet: under send max the wallet's answer tracks deposits
    /// that land after planning, and the plan does not.
    #[frb(sync, type_64bit_int)]
    pub fn recipient_value(&self, index: u32) -> Option<u64> {
        self.0.recipient_value(index as usize)
    }

    #[frb(sync)]
    pub fn input_count(&self) -> u32 {
        self.0.input_count() as u32
    }

    #[frb(sync, type_64bit_int)]
    pub fn input_total(&self) -> u64 {
        self.0.input_total()
    }
}

impl SuperWallet {
    /// Plan a consolidation of exactly `outpoints`: they all go in, one change output back, no
    /// recipient. Which coins is the caller's question — see [`Self::gap_stranded_outpoints`]
    /// and [`Self::all_unspent_outpoints`].
    /// Local and cheap; commit through [`Self::commit_send`] like any send plan.
    #[frb(sync)]
    pub fn plan_consolidate(
        &self,
        master_appkey: frostsnap_core::MasterAppkey,
        outpoints: Vec<OutPoint>,
        feerate: f32,
    ) -> anyhow::Result<SendPlan> {
        Ok(SendPlan(self.inner.lock().unwrap().plan_consolidate(
            master_appkey,
            outpoints,
            feerate,
        )?))
    }

    /// The coins a future restore could miss and that are worth moving at `feerate` — the input
    /// set the nudge's remedy consolidates.
    #[frb(sync)]
    pub fn gap_stranded_outpoints(
        &self,
        master_appkey: frostsnap_core::MasterAppkey,
        feerate: f32,
    ) -> Vec<OutPoint> {
        self.inner
            .lock()
            .unwrap()
            .gap_stranded_outpoints(master_appkey, feerate)
    }

    /// Every coin worth moving at `feerate` — the input set a whole-wallet consolidation spends.
    /// A coin carrying less than its own input cost is left alone rather than merged at a loss.
    #[frb(sync)]
    pub fn all_unspent_outpoints(
        &self,
        master_appkey: frostsnap_core::MasterAppkey,
        feerate: f32,
    ) -> Vec<OutPoint> {
        self.inner
            .lock()
            .unwrap()
            .all_unspent_outpoints(master_appkey, feerate)
    }

    /// How many coins the wallet holds — the "Consolidate (n)" menu action's badge and gate.
    #[frb(sync)]
    pub fn utxo_count(&self, master_appkey: frostsnap_core::MasterAppkey) -> u32 {
        self.inner.lock().unwrap().utxo_count(master_appkey) as u32
    }

    /// Turn a plan into a signable transaction — the single point that allocates the change
    /// address. The change index skips every index reserved by an in-flight signing session
    /// (active AND finished-but-unbroadcast, queried live from the coordinator — the wallet
    /// stores no reservation state). Fails if the wallet no longer holds a planned input
    /// (spent since planning); the plan is dead then and the flow builds a new one.
    #[frb(sync)]
    pub fn commit_send(
        &self,
        coord: RustAutoOpaque<Coordinator>,
        plan: &SendPlan,
    ) -> anyhow::Result<UnsignedTx> {
        // The wallet only has the default account today; naming it here keeps that assumption
        // at one visible call site.
        let reserved = coord
            .blocking_read()
            .0
            .inner()
            .reserved_change_indices(plan.0.master_appkey(), BitcoinAccount::default());
        let mut inner = self.inner.lock().unwrap();
        let template_tx = inner.commit_send(&plan.0, reserved)?;
        UnsignedTx::new(template_tx, plan.0.master_appkey())
            .ok_or_else(|| anyhow::anyhow!("the committed transaction spends none of our coins"))
    }
}
