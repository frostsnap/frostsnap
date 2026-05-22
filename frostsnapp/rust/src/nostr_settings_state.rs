use anyhow::{anyhow, Result};
use frostsnap_coordinator::persist::Persist;
use frostsnap_core::{AccessStructureId, AccessStructureRef, KeyId};
use frostsnap_nostr::{Keys, PublicKey};
use rusqlite::params;
use std::collections::HashMap;

#[derive(Default)]
pub struct NostrSettingsState {
    pub nsec: Option<String>,
    pub pubkey: Option<PublicKey>,
    pub access_structure_settings: HashMap<AccessStructureId, AccessStructureSettings>,
}

#[derive(Clone)]
pub struct AccessStructureSettings {
    pub key_id: KeyId,
    pub coordination_ui_enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// `nsec` is the bech32 string. Setters validate it before constructing
    /// this variant; `apply_mutation` re-parses to derive the pubkey, but
    /// silently leaves pubkey as `None` if the stored value is corrupt
    /// (only reachable on load — at that point the row was once valid).
    SetNsec { nsec: Option<String> },
    SetCoordinationUiEnabled {
        access_structure_id: AccessStructureId,
        key_id: KeyId,
        enabled: bool,
    },
}

impl NostrSettingsState {
    pub fn set_nsec(&mut self, nsec: Option<String>, mutations: &mut Vec<Mutation>) -> Result<()> {
        if let Some(n) = &nsec {
            // Validate before staging the mutation so apply_mutation can stay
            // infallible.
            Keys::parse(n)?;
        }
        self.mutate(Mutation::SetNsec { nsec }, mutations);
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
            Mutation::SetNsec { nsec } => {
                self.pubkey = nsec
                    .as_deref()
                    .and_then(|n| Keys::parse(n).ok())
                    .map(|k| k.public_key().into());
                self.nsec = nsec;
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

        // Identity row → SetNsec mutation
        let nsec: Option<String> = conn
            .query_row(
                "SELECT value FROM nostr_settings WHERE key = 'nsec'",
                [],
                |row| row.get(0),
            )
            .ok();
        if let Some(n) = &nsec {
            // Validate at load time so a corrupt row surfaces early.
            Keys::parse(n).map_err(|e| anyhow!("stored nsec failed to parse: {e}"))?;
        }
        state.apply_mutation(Mutation::SetNsec { nsec });

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
                Mutation::SetNsec { nsec: Some(nsec) } => {
                    tx.execute(
                        "INSERT OR REPLACE INTO nostr_settings (key, value) VALUES ('nsec', ?1)",
                        params![nsec],
                    )?;
                }
                Mutation::SetNsec { nsec: None } => {
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
