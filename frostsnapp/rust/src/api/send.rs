//! The send flow's plan-then-commit boundary as Dart drives it: confirming the amount builds a
//! [`SendPlan`] (see [`super::transaction::BuildTxState::try_finish`]), the review UI reads its
//! numbers, and signing passes it back through [`SuperWallet::commit_send`]. The plan is a pure
//! value — holding, dropping, or rebuilding one costs the wallet nothing.

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
}

impl SuperWallet {
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
        Ok(UnsignedTx {
            template_tx: inner.commit_send(&plan.0, reserved)?,
        })
    }
}
