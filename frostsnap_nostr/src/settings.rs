use anyhow::Result;
use bdk_chain::rusqlite_impl::migrate_schema;
use frostsnap_coordinator::persist::Persist;
use frostsnap_core::KeyId;
use rusqlite::params;
use std::collections::HashSet;
use tracing::{event, Level};

/// Settings for the Nostr remote signing feature.
#[derive(Default)]
pub struct NostrSettings {
    /// User's nostr secret key (bech32 nsec format), unencrypted
    pub nsec: Option<String>,
    /// Relay URLs
    pub relay_urls: Vec<String>,
    /// Which wallets have remote signing enabled (by key_id hex)
    remote_signing_enabled: HashSet<String>,
}

/// Mutations to NostrSettings that can be persisted.
#[derive(Debug, Clone)]
pub enum Mutation {
    SetNsec { nsec: Option<String> },
    SetRelayUrls { urls: Vec<String> },
    SetRemoteSigningEnabled { key_id: KeyId, enabled: bool },
}

impl NostrSettings {
    pub fn is_remote_signing_enabled(&self, key_id: &KeyId) -> bool {
        self.remote_signing_enabled.contains(&hex::encode(key_id.0))
    }

    pub fn set_nsec(&mut self, nsec: Option<String>, mutations: &mut Vec<Mutation>) {
        self.nsec = nsec.clone();
        mutations.push(Mutation::SetNsec { nsec });
    }

    pub fn set_relay_urls(&mut self, urls: Vec<String>, mutations: &mut Vec<Mutation>) {
        self.relay_urls = urls.clone();
        mutations.push(Mutation::SetRelayUrls { urls });
    }

    pub fn set_remote_signing_enabled(
        &mut self,
        key_id: KeyId,
        enabled: bool,
        mutations: &mut Vec<Mutation>,
    ) {
        let key_hex = hex::encode(key_id.0);
        if enabled {
            self.remote_signing_enabled.insert(key_hex);
        } else {
            self.remote_signing_enabled.remove(&key_hex);
        }
        mutations.push(Mutation::SetRemoteSigningEnabled { key_id, enabled });
    }
}

impl Persist<rusqlite::Connection> for NostrSettings {
    type Update = Vec<Mutation>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_nostr_settings";
        const MIGRATIONS: &[&str] = &[
            "CREATE TABLE IF NOT EXISTS fs_nostr_settings ( \
                key TEXT PRIMARY KEY, \
                value TEXT \
            )",
        ];

        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;
        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, _: Self::LoadParams) -> Result<Self>
    where
        Self: Sized,
    {
        let mut settings = NostrSettings::default();

        let mut stmt = conn.prepare("SELECT key, value FROM fs_nostr_settings")?;
        let row_iter = stmt.query_map([], |row| {
            let key = row.get::<_, String>(0)?;
            let value = row.get::<_, Option<String>>(1)?;
            Ok((key, value))
        })?;

        for row in row_iter {
            let (key, value) = row?;
            let span = tracing::span!(Level::DEBUG, "nostr settings", key = key);
            let _ = span.enter();

            match key.as_str() {
                "nsec" => {
                    settings.nsec = value;
                }
                "relay_urls" => {
                    if let Some(json) = value {
                        match serde_json::from_str::<Vec<String>>(&json) {
                            Ok(urls) => settings.relay_urls = urls,
                            Err(e) => {
                                event!(Level::WARN, error = %e, "failed to parse relay_urls");
                            }
                        }
                    }
                }
                key if key.starts_with("remote_signing_enabled_") => {
                    if value.as_deref() == Some("true") {
                        let key_hex = key.strip_prefix("remote_signing_enabled_").unwrap();
                        settings.remote_signing_enabled.insert(key_hex.to_string());
                    }
                }
                _ => {
                    event!(Level::WARN, key = key, "unknown nostr setting");
                }
            }
        }

        Ok(settings)
    }

    fn persist_update(&self, conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        for mutation in update {
            match &mutation {
                Mutation::SetNsec { nsec } => {
                    event!(Level::DEBUG, "set nostr nsec");
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_nostr_settings (key, value) VALUES (?1, ?2)",
                        params!["nsec", nsec],
                    )?;
                }
                Mutation::SetRelayUrls { urls } => {
                    event!(Level::DEBUG, count = urls.len(), "set relay urls");
                    let json = serde_json::to_string(urls)?;
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_nostr_settings (key, value) VALUES (?1, ?2)",
                        params!["relay_urls", json],
                    )?;
                }
                Mutation::SetRemoteSigningEnabled { key_id, enabled } => {
                    let key_hex = hex::encode(key_id.0);
                    event!(
                        Level::DEBUG,
                        key_id = key_hex,
                        enabled = enabled,
                        "set remote signing enabled"
                    );
                    let db_key = format!("remote_signing_enabled_{}", key_hex);
                    if *enabled {
                        conn.execute(
                            "INSERT OR REPLACE INTO fs_nostr_settings (key, value) VALUES (?1, ?2)",
                            params![db_key, "true"],
                        )?;
                    } else {
                        conn.execute(
                            "DELETE FROM fs_nostr_settings WHERE key = ?1",
                            params![db_key],
                        )?;
                    }
                }
            }
        }

        Ok(())
    }
}
