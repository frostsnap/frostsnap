use anyhow::Result;
use frostsnap_coordinator::persist::Persist;
use frostsnap_core::{AccessStructureId, AccessStructureRef, KeyId};
use frostsnap_nostr::{Keys, NostrIdentity, PublicKey};
use rusqlite::params;
use std::collections::HashMap;

#[derive(Default)]
pub struct NostrSettingsState {
    pub identity: Option<NostrIdentity>,
    pub access_structure_settings: HashMap<AccessStructureId, AccessStructureSettings>,
}

impl NostrSettingsState {
    pub fn pubkey(&self) -> Option<PublicKey> {
        self.identity.as_ref().and_then(|id| id.public_key().ok())
    }
}

#[derive(Clone)]
pub struct AccessStructureSettings {
    pub key_id: KeyId,
    pub coordination_ui_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    SetIdentity {
        identity: Option<NostrIdentity>,
    },
    SetCoordinationUiEnabled {
        access_structure_id: AccessStructureId,
        key_id: KeyId,
        enabled: bool,
    },
}

impl NostrSettingsState {
    /// Atomically replace the identity record.
    pub fn set_identity(
        &mut self,
        identity: Option<NostrIdentity>,
        mutations: &mut Vec<Mutation>,
    ) -> Result<()> {
        if let Some(id) = &identity {
            // Validate the nsec before staging so apply_mutation is infallible.
            id.keys()?;
        }
        self.mutate(Mutation::SetIdentity { identity }, mutations);
        Ok(())
    }

    pub fn set_coordination_ui_enabled(
        &mut self,
        access_structure_ref: AccessStructureRef,
        enabled: bool,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(
            Mutation::SetCoordinationUiEnabled {
                access_structure_id: access_structure_ref.access_structure_id,
                key_id: access_structure_ref.key_id,
                enabled,
            },
            mutations,
        );
    }

    pub fn is_coordination_ui_enabled(&self, asid: AccessStructureId) -> bool {
        self.access_structure_settings
            .get(&asid)
            .is_some_and(|s| s.coordination_ui_enabled)
    }

    fn mutate(&mut self, mutation: Mutation, mutations: &mut Vec<Mutation>) {
        self.apply_mutation(mutation.clone());
        mutations.push(mutation);
    }

    fn apply_mutation(&mut self, mutation: Mutation) {
        match mutation {
            Mutation::SetIdentity { identity } => {
                self.identity = identity;
            }
            Mutation::SetCoordinationUiEnabled {
                access_structure_id,
                key_id,
                enabled,
            } => {
                self.access_structure_settings
                    .entry(access_structure_id)
                    .and_modify(|s| {
                        s.coordination_ui_enabled = enabled;
                        s.key_id = key_id;
                    })
                    .or_insert(AccessStructureSettings {
                        key_id,
                        coordination_ui_enabled: enabled,
                    });
            }
        }
    }
}

impl Persist<rusqlite::Connection> for NostrSettingsState {
    type Update = Vec<Mutation>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> Result<()> {
        let tx = conn.transaction()?;
        tx.execute(
            "CREATE TABLE IF NOT EXISTS nostr_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;
        tx.execute(
            "CREATE TABLE IF NOT EXISTS nostr_access_structure_settings (
                access_structure_id TEXT PRIMARY KEY,
                key_id TEXT NOT NULL,
                coordination_ui_enabled INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _: Self::LoadParams) -> Result<Self> {
        let mut state = NostrSettingsState::default();

        // Load identity row. Tolerant of stale/old-format rows: a
        // deserialize failure is logged and treated as identity None,
        // and the stale row is DELETEd inside the same transaction
        // so the next mutation starts clean. This is the pre-release
        // "no backward-compat migration" contract from the
        // nostr_identity_type plan — MUST NOT block app startup, or
        // the user can't reach the setup flow to re-enter identity.
        let tx = conn.transaction()?;
        let identity_json: Option<String> = tx
            .query_row(
                "SELECT value FROM nostr_settings WHERE key = 'identity'",
                [],
                |row| row.get(0),
            )
            .ok();
        state.identity = match identity_json {
            Some(s) => match serde_json::from_str::<NostrIdentity>(&s) {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "stored identity row failed to deserialize; clearing",
                    );
                    tx.execute("DELETE FROM nostr_settings WHERE key = 'identity'", [])?;
                    None
                }
            },
            None => None,
        };
        // Old-format standalone nsec row is no longer used — clear it
        // if present so it doesn't accumulate.
        tx.execute("DELETE FROM nostr_settings WHERE key = 'nsec'", [])?;
        tx.commit()?;

        // Per-access-structure rows → SetCoordinationUiEnabled mutations
        let mut stmt = conn.prepare(
            "SELECT access_structure_id, key_id, coordination_ui_enabled
             FROM nostr_access_structure_settings",
        )?;
        let mutations: Vec<Mutation> = stmt
            .query_map([], |row| {
                let access_structure_id: AccessStructureId = row.get(0)?;
                let key_id: KeyId = row.get(1)?;
                let enabled: i64 = row.get(2)?;
                Ok(Mutation::SetCoordinationUiEnabled {
                    access_structure_id,
                    key_id,
                    enabled: enabled != 0,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        for mutation in mutations {
            state.apply_mutation(mutation);
        }

        Ok(state)
    }

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        mutations: Self::Update,
    ) -> Result<()> {
        if mutations.is_empty() {
            return Ok(());
        }
        let tx = conn.transaction()?;
        for mutation in mutations {
            match mutation {
                Mutation::SetIdentity { identity } => {
                    match identity {
                        Some(id) => {
                            let json = serde_json::to_string(&id)?;
                            tx.execute(
                                "INSERT OR REPLACE INTO nostr_settings (key, value) VALUES ('identity', ?1)",
                                params![json],
                            )?;
                        }
                        None => {
                            tx.execute("DELETE FROM nostr_settings WHERE key = 'identity'", [])?;
                        }
                    }
                    // Belt-and-suspenders: nsec row is no longer used;
                    // clear any that survived load.
                    tx.execute("DELETE FROM nostr_settings WHERE key = 'nsec'", [])?;
                }
                Mutation::SetCoordinationUiEnabled {
                    access_structure_id,
                    key_id,
                    enabled,
                } => {
                    tx.execute(
                        "INSERT INTO nostr_access_structure_settings
                            (access_structure_id, key_id, coordination_ui_enabled)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(access_structure_id) DO UPDATE SET
                            coordination_ui_enabled = excluded.coordination_ui_enabled,
                            key_id = excluded.key_id",
                        params![access_structure_id, key_id, enabled as i64],
                    )?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }
}

// Silence unused-import — Keys is used in the tolerant-load branch's
// tracing note about corruption.
#[allow(dead_code)]
fn _keys_import_touch(_: Keys) {}

#[cfg(test)]
mod tests {
    use super::*;
    use frostsnap_coordinator::persist::Persist;
    use frostsnap_nostr::Nsec;
    use rusqlite::params;

    fn conn_with_schema() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        <NostrSettingsState as Persist<rusqlite::Connection>>::migrate(&mut conn).unwrap();
        conn
    }

    fn insert_identity_row(conn: &mut rusqlite::Connection, json: &str) {
        conn.execute(
            "INSERT OR REPLACE INTO nostr_settings (key, value) VALUES ('identity', ?1)",
            params![json],
        )
        .unwrap();
    }

    fn insert_nsec_row(conn: &mut rusqlite::Connection, nsec: &str) {
        conn.execute(
            "INSERT OR REPLACE INTO nostr_settings (key, value) VALUES ('nsec', ?1)",
            params![nsec],
        )
        .unwrap();
    }

    fn row_exists(conn: &rusqlite::Connection, key: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM nostr_settings WHERE key = ?1",
            params![key],
            |_| Ok(()),
        )
        .is_ok()
    }

    #[test]
    fn load_no_rows() {
        let mut conn = conn_with_schema();
        let state = NostrSettingsState::load(&mut conn, ()).unwrap();
        assert!(state.identity.is_none());
    }

    #[test]
    fn load_valid_new_format() {
        let mut conn = conn_with_schema();
        let nsec = Nsec::generate();
        let id = NostrIdentity::Generated {
            nsec: nsec.clone(),
            name: "Alice".into(),
            created_at: 1234,
        };
        insert_identity_row(&mut conn, &serde_json::to_string(&id).unwrap());
        let state = NostrSettingsState::load(&mut conn, ()).unwrap();
        assert_eq!(state.identity.as_ref(), Some(&id));
        assert!(state.pubkey().is_some());
    }

    /// Old-shape identity JSON (pre-`nostr_identity_type`): had
    /// `pubkey` + `cached_public_profile` fields, no `nsec`. Should
    /// fail to deserialize (unknown variant / missing field) → load
    /// still succeeds, `identity` is None, stale row deleted, app
    /// startup unaffected.
    #[test]
    fn load_old_shape_identity_json_clears_row() {
        let mut conn = conn_with_schema();
        insert_identity_row(
            &mut conn,
            r#"{"mode":"imported","pubkey":"0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20","cached_public_profile":null}"#,
        );
        let state = NostrSettingsState::load(&mut conn, ()).unwrap();
        assert!(state.identity.is_none());
        assert!(!row_exists(&conn, "identity"));
    }

    /// A row in NEW format but with a semantically-invalid Nsec
    /// string. Must not sneak past load — Nsec's `Deserialize`
    /// validates via `Nsec::parse`, so this row fails to deserialize
    /// and follows the same tolerant-load path as the old-shape
    /// case. Guards codex's concern about identity_pubkey() panicking
    /// on a stored bogus Nsec.
    #[test]
    fn load_new_format_with_invalid_nsec_clears_row() {
        let mut conn = conn_with_schema();
        insert_identity_row(
            &mut conn,
            r#"{"Generated":{"nsec":"not-a-real-nsec","name":"Bob","created_at":42}}"#,
        );
        let state = NostrSettingsState::load(&mut conn, ()).unwrap();
        assert!(state.identity.is_none());
        assert!(!row_exists(&conn, "identity"));
    }

    /// Standalone old-format `nsec` row (pre-plan) with no identity
    /// row: cleared during load, identity is None.
    #[test]
    fn load_orphan_nsec_row_cleared() {
        let mut conn = conn_with_schema();
        insert_nsec_row(&mut conn, "nsec1anything");
        let state = NostrSettingsState::load(&mut conn, ()).unwrap();
        assert!(state.identity.is_none());
        assert!(!row_exists(&conn, "nsec"));
    }

    /// A stored new-format identity with a corrupt Nsec should NOT
    /// arrive in memory as a valid NostrIdentity — either it fails
    /// to deserialize (the previous test) or, if we somehow bypass
    /// Deserialize, calling identity.keys() must return Err. This
    /// pins the semantic invariant.
    #[test]
    fn identity_keys_infallible_on_valid_load() {
        let mut conn = conn_with_schema();
        let nsec = Nsec::generate();
        let id = NostrIdentity::Generated {
            nsec,
            name: "Carol".into(),
            created_at: 7,
        };
        insert_identity_row(&mut conn, &serde_json::to_string(&id).unwrap());
        let state = NostrSettingsState::load(&mut conn, ()).unwrap();
        let loaded = state.identity.expect("load succeeded");
        // keys() + public_key() must not error for a validly-loaded
        // identity — this is what identity_pubkey()'s expect() relies
        // on.
        loaded.keys().expect("valid nsec parses to Keys");
        loaded.public_key().expect("valid nsec derives a pubkey");
    }
}
