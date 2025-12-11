use bitcoin::{Psbt, Txid};
use flutter_rust_bridge::frb;
use frostsnap_coordinator::{
    bitcoin::psbt::{LoadSignSessionPsbtParams, PersistedPsbtBySsid, PersistedPsbtsByTxid},
    persist::Persisted,
};
use frostsnap_core::SignSessionId;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone)]
#[frb(opaque)]
pub struct PsbtManager {
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl PsbtManager {
    pub fn new(db: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self { db }
    }

    #[frb(sync)]
    pub(crate) fn with_ssid(&self, ssid: SignSessionId) -> anyhow::Result<Option<Psbt>> {
        let mut db = self.db.lock().unwrap();
        let persisted_psbt = Persisted::<Option<PersistedPsbtBySsid>>::new(&mut db, ssid)?;
        Ok(persisted_psbt
            .as_ref()
            .as_ref()
            .map(|ss_psbt| ss_psbt.psbt.clone()))
    }

    #[frb(sync)]
    pub fn with_txid(&self, txid: Txid) -> anyhow::Result<BTreeMap<SignSessionId, Psbt>> {
        let mut db = self.db.lock().unwrap();
        let persisted_psbt = Persisted::<PersistedPsbtsByTxid>::new(&mut db, *txid)?;
        Ok(persisted_psbt.as_ref().psbt_by_ssid.clone())
    }

    /// Assumes non-mutable txid.
    #[frb(sync)]
    pub fn insert(&self, ssid: &SignSessionId, psbt: &Psbt) -> anyhow::Result<()> {
        let txid = psbt.unsigned_tx.compute_txid();

        let mut db = self.db.lock().unwrap();
        let mut persisted_psbt = Persisted::<Option<PersistedPsbtBySsid>>::new(
            &mut db,
            LoadSignSessionPsbtParams::Ssid(*ssid),
        )?;
        persisted_psbt.mutate2(&mut db, |ss_psbt, update| {
            *ss_psbt = Some(PersistedPsbtBySsid {
                ssid: *ssid,
                txid,
                psbt: psbt.clone(),
            });
            *update = ss_psbt.clone();
            Ok(())
        })
    }
}
