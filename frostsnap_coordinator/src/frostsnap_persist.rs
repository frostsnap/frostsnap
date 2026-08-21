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

/// What a device's verified certificate told us about it.
///
/// Display data, deliberately: it is what lets the app show a known device's colour
/// and serial the moment it appears — and while it is disconnected — without
/// spending a DS exponentiation. It is never evidence. A stored record does not
/// suppress a challenge and does not by itself render a "genuine" verdict; see
/// [`crate::genuine_check`] for why honouring a remembered verdict would be worse
/// than having no check at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenuineRecord {
    /// The case colour's name, via `CaseColor`'s `Display`. A colour this build has
    /// no name for stores as "Unknown" and reads back as no colour — `FromStr`
    /// rejects it — rather than being recorded as the wrong one. Nothing is lost by
    /// that: a device is re-challenged on every connect, so a later build that does
    /// know the colour learns it the next time the device is plugged in.
    pub case_color: String,
    pub serial: String,
    pub revision: String,
}

/// Persisted [`GenuineRecord`]s keyed by [`DeviceId`].
///
/// Kept apart from `DeviceNames` on purpose: the check resolves before a device is
/// named, and a device may never be named at all, so storing this alongside the name
/// would drop the colour for exactly the devices the user most needs to tell apart.
#[derive(Default)]
pub struct GenuineDeviceInfo {
    info: HashMap<DeviceId, GenuineRecord>,
    /// `None` means "delete this row".
    mutations: VecDeque<(DeviceId, Option<GenuineRecord>)>,
}

impl GenuineDeviceInfo {
    pub fn set(&mut self, device_id: DeviceId, record: GenuineRecord) {
        if self.info.get(&device_id) != Some(&record) {
            self.info.insert(device_id, record.clone());
            self.mutations.push_back((device_id, Some(record)));
        }
    }

    /// Forget a device, e.g. after it failed a live check or was erased. What we
    /// stored describes hardware we can no longer vouch for, so it should stop
    /// being shown rather than linger as a stale colour.
    pub fn remove(&mut self, device_id: DeviceId) {
        if self.info.remove(&device_id).is_some() {
            self.mutations.push_back((device_id, None));
        }
    }

    pub fn get(&self, device_id: DeviceId) -> Option<&GenuineRecord> {
        self.info.get(&device_id)
    }
}

impl TakeStaged<VecDeque<(DeviceId, Option<GenuineRecord>)>> for GenuineDeviceInfo {
    fn take_staged_update(&mut self) -> Option<VecDeque<(DeviceId, Option<GenuineRecord>)>> {
        if self.mutations.is_empty() {
            None
        } else {
            Some(core::mem::take(&mut self.mutations))
        }
    }
}

impl Persist<rusqlite::Connection> for GenuineDeviceInfo {
    type Update = VecDeque<(DeviceId, Option<GenuineRecord>)>;
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
            match record {
                Some(record) => conn.execute(
                    "INSERT OR REPLACE INTO fs_genuine_devices (id, case_color, serial, revision) \
                     VALUES (?1, ?2, ?3, ?4)",
                    params![id, record.case_color, record.serial, record.revision],
                )?,
                None => conn.execute(
                    "DELETE FROM fs_genuine_devices WHERE id = ?1",
                    params![id],
                )?,
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod genuine_device_info_test {
    use super::*;
    use crate::persist::Persist;
    use tempfile::NamedTempFile;

    fn record(color: &str) -> GenuineRecord {
        GenuineRecord {
            case_color: color.to_string(),
            serial: "220825002".to_string(),
            revision: "2.7-1625".to_string(),
        }
    }

    fn device(byte: u8) -> DeviceId {
        DeviceId([byte; 33])
    }

    #[test]
    fn records_survive_a_reload_and_removal_is_persisted() -> anyhow::Result<()> {
        let temp_file = NamedTempFile::new()?;
        let mut conn = rusqlite::Connection::open(temp_file.path())?;
        GenuineDeviceInfo::migrate(&mut conn)?;

        let mut info = GenuineDeviceInfo::load(&mut conn, ())?;
        info.set(device(1), record("Orange"));
        info.set(device(2), record("Red"));
        // Re-setting the same value shouldn't stage a redundant write.
        info.set(device(1), record("Orange"));
        let update = info.take_staged_update().expect("two devices staged");
        assert_eq!(update.len(), 2);
        info.persist_update(&mut conn, update)?;

        let reloaded = GenuineDeviceInfo::load(&mut conn, ())?;
        assert_eq!(reloaded.get(device(1)), Some(&record("Orange")));
        assert_eq!(reloaded.get(device(2)), Some(&record("Red")));

        // A device that fails a live check stops being vouched for, and that has to
        // reach the database — otherwise it comes back on next launch.
        let mut info = reloaded;
        info.remove(device(1));
        let update = info.take_staged_update().expect("removal staged");
        info.persist_update(&mut conn, update)?;

        let reloaded = GenuineDeviceInfo::load(&mut conn, ())?;
        assert_eq!(reloaded.get(device(1)), None);
        assert_eq!(reloaded.get(device(2)), Some(&record("Red")));

        Ok(())
    }

    /// A colour this build has no name for must read back as no colour, never as
    /// some other colour — colour is what the user matches against the object in
    /// their hand.
    #[test]
    fn an_unrecognised_colour_reads_back_as_no_colour() {
        use frostsnap_comms::genuine_certificate::CaseColor;
        use std::str::FromStr;

        let stored = CaseColor::Unused3.to_string();
        assert!(CaseColor::from_str(&stored).is_err());
        assert!(CaseColor::from_str(&CaseColor::Orange.to_string()).is_ok());
    }
}
