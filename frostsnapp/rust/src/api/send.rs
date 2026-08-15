//! The send flow's plan-then-commit boundary as Dart drives it: confirming the amount builds a
//! [`SendPlan`] (see [`super::transaction::BuildTxState::try_finish`]), the review UI reads its
//! numbers, and signing passes it back through [`SuperWallet::commit_send`]. The plan is a pure
//! value — holding, dropping, or rebuilding one costs the wallet nothing.

use flutter_rust_bridge::frb;
use frostsnap_coordinator::bitcoin::send as coord_send;

use super::signing::UnsignedTx;
use super::super_wallet::SuperWallet;

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
}

impl SuperWallet {
    /// Turn a plan into a signable transaction — the single point that allocates the change
    /// address. Fails if the wallet no longer holds a planned input (spent since planning);
    /// the plan is dead then and the flow builds a new one.
    #[frb(sync)]
    pub fn commit_send(&self, plan: &SendPlan) -> anyhow::Result<UnsignedTx> {
        let mut inner = self.inner.lock().unwrap();
        Ok(UnsignedTx {
            template_tx: inner.commit_send(&plan.0)?,
        })
    }
}
