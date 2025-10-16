use crate::{
    bitcoin::chain_sync::{default_backup_electrum_server, default_electrum_server},
    persist::Persist,
};
use bdk_chain::{bitcoin, rusqlite_impl::migrate_schema};
use core::str::FromStr;
use rusqlite::params;
use std::collections::BTreeMap;
use tracing::{event, Level};

#[derive(Default)]
pub struct Settings {
    pub electrum_servers: BTreeMap<bitcoin::Network, String>,
    pub backup_electrum_servers: BTreeMap<bitcoin::Network, String>,
    pub developer_mode: bool,
}

impl Settings {
    pub fn set_developer_mode(&mut self, value: bool, mutations: &mut Vec<Mutation>) {
        self.mutate(Mutation::SetDeveloperMode { value }, mutations);
    }

    pub fn get_electrum_server(&self, network: bitcoin::Network) -> String {
        self.electrum_servers
            .get(&network)
            .cloned()
            .or(Some(default_electrum_server(network).to_string()))
            .expect("unsupported network")
    }

    pub fn get_backup_electrum_server(&self, network: bitcoin::Network) -> String {
        self.backup_electrum_servers
            .get(&network)
            .cloned()
            .unwrap_or(default_backup_electrum_server(network).to_string())
    }

    pub fn set_electrum_server(
        &mut self,
        network: bitcoin::Network,
        url: String,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(Mutation::SetElectrumServer { network, url }, mutations)
    }

    fn mutate(&mut self, mutation: Mutation, mutations: &mut Vec<Mutation>) {
        self.apply_mutation(mutation.clone());
        mutations.push(mutation);
    }

    fn apply_mutation(&mut self, mutation: Mutation) {
        match mutation {
            Mutation::SetDeveloperMode { value } => {
                self.developer_mode = value;
            }
            Mutation::SetElectrumServer { network, url } => {
                self.electrum_servers.insert(network, url);
            }
            Mutation::SetBackupElectrumServer { network, url } => {
                self.backup_electrum_servers.insert(network, url);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    SetDeveloperMode {
        value: bool,
    },
    SetElectrumServer {
        network: bitcoin::Network,
        url: String,
    },
    SetBackupElectrumServer {
        network: bitcoin::Network,
        url: String,
    },
}

impl Persist<rusqlite::Connection> for Settings {
    type Update = Vec<Mutation>;
    type LoadParams = ();

    fn migrate(conn: &mut rusqlite::Connection) -> anyhow::Result<()> {
        const SCHEMA_NAME: &str = "frostsnap_settings";
        const MIGRATIONS: &[&str] = &[
            // Version 0
            "CREATE TABLE IF NOT EXISTS fs_app_global_settings ( \
                key TEXT PRIMARY KEY, \
                value TEXT \
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
        let mut settings = Settings::default();

        {
            let mut stmt = conn.prepare("SELECT key, value FROM fs_app_global_settings")?;
            let row_iter = stmt.query_map([], |row| {
                let key = row.get::<_, String>(0)?;
                let value = row.get::<_, String>(1)?;
                Ok((key, value))
            })?;

            for row in row_iter {
                let (key, value) = row?;
                let span =
                    tracing::span!(Level::DEBUG, "global settings", key = key, value = value);
                let _ = span.enter();
                let mutation = match key.as_str() {
                    "developer_mode" => Mutation::SetDeveloperMode {
                        value: bool::from_str(value.as_str())?,
                    },
                    electrum_server if electrum_server.starts_with("electrum_server_") => {
                        let network = electrum_server.strip_prefix("electrum_server_").unwrap();
                        match bitcoin::Network::from_str(network) {
                            Ok(network) => Mutation::SetElectrumServer {
                                network,
                                url: value.to_string(),
                            },
                            Err(_) => {
                                event!(
                                    Level::WARN,
                                    network = network,
                                    "bitcoin network not supported",
                                );
                                continue;
                            }
                        }
                    }
                    _ => {
                        event!(
                            Level::WARN,
                            key = key,
                            value = value,
                            "unknown global setting",
                        );
                        continue;
                    }
                };

                settings.apply_mutation(mutation);
            }
        }

        Ok(settings)
    }

    fn persist_update(
        &self,
        conn: &mut rusqlite::Connection,
        update: Self::Update,
    ) -> anyhow::Result<()> {
        for mutation in update {
            match mutation {
                Mutation::SetDeveloperMode { value } => {
                    event!(Level::DEBUG, value = value, "changed developer mode");
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params!["developer_mode", value.to_string()],
                    )?;
                }
                Mutation::SetElectrumServer { network, url } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        url,
                        "set electrum server for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![format!("electrum_server_{}", network), url.to_string()],
                    )?;
                }
                Mutation::SetBackupElectrumServer { network, url } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        url,
                        "set backup electrum server for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![format!("backup_electrum_server_{}", network), url.to_string()],
                    )?;
                }
            }
        }

        Ok(())
    }
}
