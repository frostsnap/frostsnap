use frostsnap_coordinator::{
    frostsnap_core::{
        self,
        message::{CoordinatorToStorageMessage, SignTask},
        CoordinatorFrostKey, FrostCoordinator, KeyId,
    },
    UiToStorageMessage,
};
use llsdb::{
    index::{self, IndexStore},
    Backend, Result, Transaction, TxIo,
};
use std::cell::RefMut;

/// persiting frostsnap_core state
#[derive(Debug)]
pub struct PersistCore {
    log: index::Vec<CoordinatorToStorageMessage>,
    signing_cell: index::CellOption<frostsnap_core::SigningSessionState>,
}

impl PersistCore {
    pub fn new(tx: &mut Transaction<'_, impl Backend>) -> Result<Self> {
        let log_list = tx.take_list("frostsnap/log")?;
        let log = index::Vec::new(log_list, tx)?;
        let signing_list = tx.take_list("frostsnap/signing")?;
        let signing_cell = index::CellOption::new(signing_list, tx)?;
        Ok(Self { log, signing_cell })
    }
}

impl<'i, F: Backend> PersistApi<'i, F> {
    pub fn coord_frost_keys(&self) -> Result<Vec<CoordinatorFrostKey>> {
        Ok(self.core_coordinator()?.iter_keys().cloned().collect())
    }

    pub fn core_coordinator(&self) -> Result<FrostCoordinator> {
        let mut coord = FrostCoordinator::new();
        for change in self.log.iter() {
            coord.apply_change(change?);
        }
        Ok(coord)
    }

    pub fn consume_core_message(&mut self, message: CoordinatorToStorageMessage) -> Result<()> {
        match message {
            // handle store signing state separately because it's transient
            CoordinatorToStorageMessage::StoreSigningState(signing_state) => {
                self.signing_cell.replace(Some(&signing_state))?;
                Ok(())
            }
            message => self.log.push(&message),
        }
    }

    pub fn consume_ui_message(&mut self, message: UiToStorageMessage) -> Result<()> {
        use frostsnap_coordinator::UiToStorageMessage;
        match message {
            UiToStorageMessage::ClearSigningSession => self.signing_cell.clear(),
        }
    }

    pub fn persisted_signing(&self) -> Result<Option<frostsnap_core::SigningSessionState>> {
        self.signing_cell.get()
    }

    pub fn persisted_sign_session_task(&self, key_id: KeyId) -> Result<Option<SignTask>> {
        let opt = self.signing_cell.get()?;
        Ok(opt.and_then(|sign_session_state| {
            if sign_session_state.request.key_id == key_id {
                Some(sign_session_state.request.sign_task)
            } else {
                None
            }
        }))
    }
}

// Everything below can be auto-derived in the future
#[derive(Debug)]
pub struct PersistApi<'i, F> {
    log: <index::Vec<frostsnap_core::message::CoordinatorToStorageMessage> as IndexStore>::Api<
        'i,
        F,
    >,
    signing_cell:
        <index::CellOption<frostsnap_core::SigningSessionState> as IndexStore>::Api<'i, F>,
}

impl IndexStore for PersistCore {
    type Api<'i, F> = PersistApi<'i, F>;

    fn tx_fail_rollback(&mut self) {
        self.log.tx_fail_rollback();
        self.signing_cell.tx_fail_rollback();
    }

    fn tx_success(&mut self) {
        self.log.tx_success();
        self.signing_cell.tx_success()
    }

    fn owned_lists(&self) -> std::vec::Vec<llsdb::ListSlot> {
        self.log
            .owned_lists()
            .into_iter()
            .chain(self.signing_cell.owned_lists())
            .collect()
    }

    fn create_api<'s, F>(store: RefMut<'s, Self>, io: TxIo<'s, F>) -> Self::Api<'s, F>
    where
        Self: Sized,
    {
        let (log, signing_cell) = RefMut::map_split(store, |persist| {
            (&mut persist.log, &mut persist.signing_cell)
        });
        let log = index::Vec::create_api(log, io.clone());
        let signing_cell = index::CellOption::create_api(signing_cell, io.clone());
        PersistApi { log, signing_cell }
    }
}
