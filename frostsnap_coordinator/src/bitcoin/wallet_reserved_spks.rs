use crate::persist::{Persist, ToStringWrapper};
use bdk_chain::bitcoin::bip32::DerivationPath;
use frostsnap_core::{tweak::BitcoinBip32Path, Gist, KeyId};
use rusqlite::params;
use std::collections::BTreeMap;
use tracing::{event, Level};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddressMetadata {
    addresses: BTreeMap<(KeyId, DerivationPath), Option<String>>,
}

#[derive(Debug, Clone)]
pub enum Mutation {
    Reveal {
        key_and_path: (KeyId, DerivationPath),
        label: Option<String>,
    },
}

impl Gist for Mutation {
    fn gist(&self) -> String {
        match self {
            Mutation::Reveal { .. } => "Reveal",
        }
        .to_string()
    }
}

impl AddressMetadata {
    fn apply_mutation(&mut self, mutation: &Mutation) -> bool {
        match mutation {
            Mutation::Reveal {
                key_and_path,
                label,
            } => self
                .addresses
                .insert(key_and_path.clone(), label.clone())
                .is_none(),
        }
    }

    fn mutate(&mut self, mutation: Mutation, mutations: &mut Vec<Mutation>) {
        if self.apply_mutation(&mutation) {
            event!(Level::DEBUG, gist = mutation.gist(), "mutating");
            mutations.push(mutation);
        }
    }

    pub fn reveal(
        &mut self,
        key_id: KeyId,
        bip32_path: BitcoinBip32Path,
        label: Option<String>,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(
            Mutation::Reveal {
                key_and_path: (key_id, bip32_path.into()),
                label,
            },
            mutations,
        );
    }

    pub fn is_revealed(&self, key_id: KeyId, bip32_path: BitcoinBip32Path) -> bool {
        self.addresses.contains_key(&(key_id, bip32_path.into()))
    }
}

impl Persist<rusqlite::Connection> for AddressMetadata {
    type Update = Vec<Mutation>;
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: ()) -> anyhow::Result<Self> {
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS address_metadata (
                key_id TEXT NOT NULL,
                path TEXT NOT NULL,
                label TEXT,
                PRIMARY KEY (key_id, path)
            )
            "#,
            [],
        )?;

        let mut stmt = conn.prepare(
            r#"
            SELECT key_id, path, label
            FROM address_metadata
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, ToStringWrapper<KeyId>>(0)?.0,
                row.get::<_, ToStringWrapper<DerivationPath>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        let mut state = AddressMetadata::default();
        for row in rows.into_iter() {
            let (key_id, path, label) = row?;

            state.apply_mutation(&Mutation::Reveal {
                key_and_path: (key_id, path.0),
                label,
            });
        }

        Ok(state)
    }

    fn persist_update(
        conn: &mut rusqlite::Connection,
        update: Vec<Mutation>,
    ) -> anyhow::Result<()> {
        let tx = conn.transaction()?;

        for mutation in update {
            match mutation {
                Mutation::Reveal {
                    key_and_path,
                    label,
                } => {
                    let (key_id, derivation_path) = key_and_path;

                    tx.execute(
                        r#"
                        INSERT OR REPLACE INTO address_metadata (key_id, path, label)
                        VALUES (?1, ?2, ?3)
                        "#,
                        params![
                            ToStringWrapper(key_id),
                            ToStringWrapper(derivation_path),
                            label
                        ],
                    )?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }
}
