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

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
        for mutation in update {
            match mutation {
                coordinator::Mutation::Keygen(coordinator::keys::KeyMutation::DeleteKey(
                    key_id,
                )) => {
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
                        params![mutation.tied_to_key(self), mutation.tied_to_restoration(), BincodeWrapper(mutation)],
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

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
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

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
        for (id, name) in update {
            conn.execute(
                "INSERT OR REPLACE INTO fs_devices (id, name) VALUES (?1, ?2)",
                params![id, name],
            )?;
        }

        Ok(())
    }
}

/// A genuine device's attested info, extracted from its verified certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenuineRecord {
    pub case_color: String,
    pub serial: String,
    pub revision: String,
}

/// Persisted genuine-device info keyed by [`DeviceId`], written the instant a
/// device passes the genuine check.
///
/// Decoupled from [`DeviceNames`] on purpose: the check fires before a device is
/// named (and it may never be named), so storing this with the name would drop
/// the colour for unnamed devices. Its own table means a known device shows its
/// colour on reconnect and while disconnected.
#[derive(Default)]
pub struct GenuineDeviceInfo {
    info: HashMap<DeviceId, GenuineRecord>,
    mutations: VecDeque<(DeviceId, GenuineRecord)>,
}

impl GenuineDeviceInfo {
    pub fn set(&mut self, device_id: DeviceId, record: GenuineRecord) {
        if self.info.get(&device_id) != Some(&record) {
            self.info.insert(device_id, record.clone());
            self.mutations.push_back((device_id, record));
        }
    }

    pub fn get(&self, device_id: DeviceId) -> Option<&GenuineRecord> {
        self.info.get(&device_id)
    }

    pub fn get_case_color(&self, device_id: DeviceId) -> Option<String> {
        self.info.get(&device_id).map(|r| r.case_color.clone())
    }

    /// Ids of all devices with a stored genuine verdict.
    pub fn device_ids(&self) -> impl Iterator<Item = DeviceId> + '_ {
        self.info.keys().copied()
    }
}

impl TakeStaged<VecDeque<(DeviceId, GenuineRecord)>> for GenuineDeviceInfo {
    fn take_staged_update(&mut self) -> Option<VecDeque<(DeviceId, GenuineRecord)>> {
        if self.mutations.is_empty() {
            None
        } else {
            Some(core::mem::take(&mut self.mutations))
        }
    }
}

impl Persist<rusqlite::Connection> for GenuineDeviceInfo {
    type Update = VecDeque<(DeviceId, GenuineRecord)>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_genuine_devices";
        const MIGRATIONS: &[&str] = &[
            // Version 0
            "CREATE TABLE IF NOT EXISTS fs_genuine_devices ( \
                id BLOB PRIMARY KEY, \
                case_color TEXT NOT NULL, \
                serial TEXT NOT NULL, \
                revision TEXT NOT NULL \
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
        let mut stmt =
            conn.prepare("SELECT id, case_color, serial, revision FROM fs_genuine_devices")?;
        let mut genuine = GenuineDeviceInfo::default();

        let row_iter = stmt.query_map([], |row| {
            let device_id = row.get::<_, DeviceId>(0)?;
            let record = GenuineRecord {
                case_color: row.get::<_, String>(1)?,
                serial: row.get::<_, String>(2)?,
                revision: row.get::<_, String>(3)?,
            };
            Ok((device_id, record))
        })?;

        for row in row_iter {
            let (device_id, record) = row?;
            genuine.info.insert(device_id, record);
        }

        Ok(genuine)
    }

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
        for (id, record) in update {
            conn.execute(
                "INSERT OR REPLACE INTO fs_genuine_devices (id, case_color, serial, revision) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, record.case_color, record.serial, record.revision],
            )?;
        }

        Ok(())
    }
}
