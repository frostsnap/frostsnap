use frostsnap_coordinator::frostsnap_core::{
    self,
    schnorr_fun::{frost::FrostKey, fun::marker::Normal},
    FrostCoordinator,
};
use llsdb::{
    index::{CellOption, IndexStore},
    Backend, Result, Transaction, TxIo,
};
use std::cell::RefMut;

/// persiting frostsnap_core state
#[derive(Debug)]
pub struct PersistCore {
    key_cell: CellOption<frostsnap_core::CoordinatorFrostKeyState>,
    signing_cell: CellOption<frostsnap_core::SigningSessionState>,
}

impl PersistCore {
    pub fn new(tx: &mut Transaction<'_, impl Backend>) -> Result<Self> {
        let key_list = tx.take_list("frostsnap/keys")?;
        let key_cell = CellOption::new(key_list, tx)?;
        let signing_list = tx.take_list("frostsnap/signing")?;
        let signing_cell = CellOption::new(signing_list, tx)?;
        Ok(Self {
            key_cell,
            signing_cell,
        })
    }
}

impl<'i, F: Backend> PersistApi<'i, F> {
    pub fn frost_keys(&self) -> Result<Vec<FrostKey<Normal>>> {
        self.key_cell
            .get()
            .transpose()
            .into_iter()
            .map(|key| Ok(key?.frost_key().clone()))
            .collect()
    }

    pub fn core_coordinator(&self) -> Result<FrostCoordinator> {
        Ok(match self.key_cell.get()? {
            None => FrostCoordinator::new(),
            Some(key) => FrostCoordinator::from_stored_key(key),
        })
    }

    pub fn set_key_state(
        &mut self,
        key_state: frostsnap_core::CoordinatorFrostKeyState,
    ) -> Result<()> {
        self.key_cell.replace(Some(&key_state))?;
        Ok(())
    }

    pub fn persisted_signing(&self) -> Result<Option<frostsnap_core::SigningSessionState>> {
        self.signing_cell.get()
    }

    pub fn is_sign_session_persisted(&self) -> bool {
        self.signing_cell.is_some()
    }

    pub fn store_sign_session(&self, state: frostsnap_core::SigningSessionState) -> Result<()> {
        self.signing_cell.replace(Some(&state))?;
        Ok(())
    }

    pub fn clear_signing_session(&self) -> Result<()> {
        self.signing_cell.clear()
    }
}

// Everything below can be auto-derived in the future
#[derive(Debug)]
pub struct PersistApi<'i, F> {
    key_cell: <CellOption<frostsnap_core::CoordinatorFrostKeyState> as IndexStore>::Api<'i, F>,
    signing_cell: <CellOption<frostsnap_core::SigningSessionState> as IndexStore>::Api<'i, F>,
}

impl IndexStore for PersistCore {
    type Api<'i, F> = PersistApi<'i, F>;

    fn tx_fail_rollback(&mut self) {
        self.key_cell.tx_fail_rollback();
        self.signing_cell.tx_fail_rollback();
    }

    fn tx_success(&mut self) {
        self.key_cell.tx_success();
        self.signing_cell.tx_success()
    }

    fn owned_lists(&self) -> std::vec::Vec<llsdb::ListSlot> {
        self.key_cell
            .owned_lists()
            .into_iter()
            .chain(self.signing_cell.owned_lists())
            .collect()
    }

    fn create_api<'s, F>(store: RefMut<'s, Self>, io: TxIo<'s, F>) -> Self::Api<'s, F>
    where
        Self: Sized,
    {
        let (key_cell, signing_cell) = RefMut::map_split(store, |persist| {
            (&mut persist.key_cell, &mut persist.signing_cell)
        });
        let key_cell = CellOption::create_api(key_cell, io.clone());
        let signing_cell = CellOption::create_api(signing_cell, io.clone());
        PersistApi {
            key_cell,
            signing_cell,
        }
    }
}
