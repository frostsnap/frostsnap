use crate::{
    frostsnap_core::{
        self,
        coordinator::{ActiveSignSession, FrostCoordinator},
    },
    persist::{BincodeWrapper, Persist, TakeStaged},
};
use anyhow::Context;
use bdk_chain::rusqlite_impl::migrate_schema;
use frostsnap_core::{
    coordinator::{self, restoration::RestorationMutation},
    DeviceId,
};
use rusqlite::params;
use std::collections::{HashMap, VecDeque};
use tracing::{event, Level};

impl Persist<rusqlite::Connection> for FrostCoordinator {
    type Update = VecDeque<coordinator::Mutation>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_coordinator";
        const MIGRATIONS: &[&str] = &[
            // Version 0
            "CREATE TABLE IF NOT EXISTS fs_coordinator_mutations (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               mutation BLOB NOT NULL,
               tied_to_key TEXT,
               tied_to_restoration TEXT,
               version INTEGER NOT NULL
             )",
        ];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _: Self::LoadParams) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut coordinator = FrostCoordinator::new();
        let mut stmt =
            conn.prepare("SELECT mutation, version FROM fs_coordinator_mutations ORDER BY id")?;

        let row_iter = stmt.query_map([], |row| {
            let version = row.get::<_, usize>(1)?;
            if version != 0 {
                event!(
                    Level::ERROR,
                    "Version of database is newer than the app. Upgrade the app"
                )
            }

            let mutation = row.get::<_, BincodeWrapper<coordinator::Mutation>>(0)?.0;

            Ok(mutation)
        })?;

        for mutation in row_iter {
            let mutation = mutation.context("failed to decode an fs_coordinator_mutation")?;
            let _ = coordinator.apply_mutation(mutation);
        }

        Ok(coordinator)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> anyhow::Result<()> {
        for mutation in update {
            match mutation {
                coordinator::Mutation::Keygen(coordinator::keys::KeyMutation::DeleteKey(key_id)) => {
                    conn.execute(
                        "DELETE FROM fs_coordinator_mutations WHERE tied_to_key=?1",
                        params![key_id],
                    )?;
                }
                coordinator::Mutation::Restoration(RestorationMutation::DeleteRestoration {
                    restoration_id,
                }) => {
                    conn.execute(
                        "DELETE FROM fs_coordinator_mutations WHERE tied_to_restoration=?1",
                        params![restoration_id],
                    )?;
                }
                mutation => {
                    conn.execute(
                        "INSERT INTO fs_coordinator_mutations (tied_to_key, tied_to_restoration, mutation, version) VALUES (?1, ?2, ?3, 0)",
                        params![mutation.tied_to_key(), mutation.tied_to_restoration(), BincodeWrapper(mutation)],
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl TakeStaged<VecDeque<coordinator::Mutation>> for FrostCoordinator {
    fn take_staged_update(&mut self) -> Option<VecDeque<coordinator::Mutation>> {
        let mutations = self.take_staged_mutations();
        if mutations.is_empty() {
            None
        } else {
            Some(mutations)
        }
    }
}

impl Persist<rusqlite::Connection> for Option<ActiveSignSession> {
    type Update = Self;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_active_sign_session";
        const MIGRATIONS: &[&str] = &[
            // Version 0
            "CREATE TABLE IF NOT EXISTS fs_signing_session_state ( state BLOB )",
        ];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _params: Self::LoadParams) -> anyhow::Result<Self> {
        let signing_session_state =
            conn.query_row("SELECT state FROM fs_signing_session_state", [], |row| {
                Ok(row.get::<_, BincodeWrapper<ActiveSignSession>>(0)?.0)
            });

        let state = match signing_session_state {
            Ok(signing_session_state) => Some(signing_session_state),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.into()),
        };
        Ok(state)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> anyhow::Result<()> {
        match update {
            Some(signing_session_state) => {
                conn.execute(
                    "INSERT INTO fs_signing_session_state (state) VALUES (?1)",
                    params![BincodeWrapper(signing_session_state)],
                )?;
            }
            None => {
                conn.execute("DELETE FROM fs_signing_session_state", [])?;
            }
        }

        Ok(())
    }
}

impl TakeStaged<Option<ActiveSignSession>> for Option<ActiveSignSession> {
    fn take_staged_update(&mut self) -> Option<Option<ActiveSignSession>> {
        Some(self.clone())
    }
}

#[derive(Default)]
pub struct DeviceNames {
    names: HashMap<DeviceId, String>,
    mutations: VecDeque<(DeviceId, String)>,
}

impl DeviceNames {
    pub fn insert(&mut self, device_id: DeviceId, name: String) {
        if self.names.insert(device_id, name.clone()).as_ref() != Some(&name) {
            self.mutations.push_back((device_id, name));
        }
    }

    pub fn get(&self, device_id: DeviceId) -> Option<String> {
        self.names.get(&device_id).cloned()
    }
}

impl TakeStaged<VecDeque<(DeviceId, String)>> for DeviceNames {
    fn take_staged_update(&mut self) -> Option<VecDeque<(DeviceId, String)>> {
        if self.mutations.is_empty() {
            None
        } else {
            Some(core::mem::take(&mut self.mutations))
        }
    }
}

impl Persist<rusqlite::Connection> for DeviceNames {
    type Update = VecDeque<(DeviceId, String)>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_device_names";
        const MIGRATIONS: &[&str] = &[
            // Version 0
            "CREATE TABLE IF NOT EXISTS fs_devices ( \
                id BLOB PRIMARY KEY, \
                name TEXT NOT NULL \
            )",
        ];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _params: Self::LoadParams) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut stmt = conn.prepare("SELECT id, name FROM fs_devices")?;
        let mut device_names = DeviceNames::default();

        let row_iter = stmt.query_map([], |row| {
            let device_id = row.get::<_, DeviceId>(0)?;
            let name = row.get::<_, String>(1)?;
            Ok((device_id, name))
        })?;

        for row in row_iter {
            let (device_id, name) = row?;
            device_names.names.insert(device_id, name);
        }

        Ok(device_names)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> anyhow::Result<()> {
        for (id, name) in update {
            conn.execute(
                "INSERT OR REPLACE INTO fs_devices (id, name) VALUES (?1, ?2)",
                params![id, name],
            )?;
        }

        Ok(())
    }
}
