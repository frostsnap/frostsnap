use bitcoin::{Psbt, Txid};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::{
    bitcoin::psbt::{LoadSignSessionPsbtParams, SignSessionPsbt},
    persist::Persisted,
};
use frostsnap_core::SignSessionId;
use std::sync::{Arc, Mutex};

#[frb(opaque)]
pub struct PsbtManager {
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl PsbtManager {
    pub(crate) fn new(db: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { db }
    }

    #[frb(sync)]
    pub fn with_ssid(&self, ssid: &SignSessionId) -> anyhow::Result<Option<Psbt>> {
        let mut db = self.db.lock().unwrap();
        let persisted_psbt = Persisted::<Option<SignSessionPsbt>>::new(
            &mut db,
            LoadSignSessionPsbtParams::Ssid(*ssid),
        )?;
        Ok(persisted_psbt
            .as_ref()
            .as_ref()
            .map(|ss_psbt| ss_psbt.psbt.clone()))
    }

    #[frb(sync)]
    pub fn with_txid(&self, txid: &Txid) -> anyhow::Result<Option<Psbt>> {
        let mut db = self.db.lock().unwrap();
        let persisted_psbt = Persisted::<Option<SignSessionPsbt>>::new(
            &mut db,
            LoadSignSessionPsbtParams::Txid(*txid),
        )?;
        Ok(persisted_psbt
            .as_ref()
            .as_ref()
            .map(|ss_psbt| ss_psbt.psbt.clone()))
    }

    /// Assumes non-mutable txid.
    #[frb(sync)]
    pub fn insert(&self, ssid: &SignSessionId, psbt: &Psbt) -> anyhow::Result<()> {
        let txid = psbt.unsigned_tx.compute_txid();

        let mut db = self.db.lock().unwrap();
        let mut persisted_psbt = Persisted::<Option<SignSessionPsbt>>::new(
            &mut db,
            LoadSignSessionPsbtParams::Ssid(*ssid),
        )?;
        persisted_psbt.mutate2(&mut db, |ss_psbt, update| {
            *ss_psbt = Some(SignSessionPsbt {
                ssid: *ssid,
                txid,
                psbt: psbt.clone(),
            });
            *update = ss_psbt.clone();
            Ok(())
        })
    }
}
