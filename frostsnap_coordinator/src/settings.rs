use crate::{
    bitcoin::chain_sync::{default_backup_electrum_server, default_electrum_server},
    persist::Persist,
};
use bdk_chain::{bitcoin, rusqlite_impl::migrate_schema};
use core::str::FromStr;
use rusqlite::params;
use std::collections::BTreeMap;
use tracing::{event, Level};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElectrumEnabled {
    #[default]
    All,
    PrimaryOnly,
    None,
}

impl std::fmt::Display for ElectrumEnabled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElectrumEnabled::All => write!(f, "all"),
            ElectrumEnabled::PrimaryOnly => write!(f, "primary_only"),
            ElectrumEnabled::None => write!(f, "none"),
        }
    }
}

impl FromStr for ElectrumEnabled {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "all" => Ok(ElectrumEnabled::All),
            "primary_only" => Ok(ElectrumEnabled::PrimaryOnly),
            "none" => Ok(ElectrumEnabled::None),
            _ => Err(anyhow::anyhow!("invalid electrum enabled value: {}", s)),
        }
    }
}

#[derive(Default)]
pub struct Settings {
    pub electrum_servers: BTreeMap<bitcoin::Network, String>,
    pub backup_electrum_servers: BTreeMap<bitcoin::Network, String>,
    pub electrum_enabled: BTreeMap<bitcoin::Network, ElectrumEnabled>,
    pub developer_mode: bool,
    pub hide_balance: bool,
}

impl Settings {
    pub fn set_developer_mode(&mut self, value: bool, mutations: &mut Vec<Mutation>) {
        self.mutate(Mutation::SetDeveloperMode { value }, mutations);
    }

    pub fn set_hide_balance(&mut self, value: bool, mutations: &mut Vec<Mutation>) {
        self.mutate(Mutation::SetHideBalance { value }, mutations);
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

    pub fn set_backup_electrum_server(
        &mut self,
        network: bitcoin::Network,
        url: String,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(
            Mutation::SetBackupElectrumServer { network, url },
            mutations,
        )
    }

    pub fn get_electrum_enabled(&self, network: bitcoin::Network) -> ElectrumEnabled {
        self.electrum_enabled
            .get(&network)
            .copied()
            .unwrap_or_default()
    }

    pub fn set_electrum_enabled(
        &mut self,
        network: bitcoin::Network,
        enabled: ElectrumEnabled,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(Mutation::SetElectrumEnabled { network, enabled }, mutations)
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
            Mutation::SetHideBalance { value } => {
                self.hide_balance = value;
            }
            Mutation::SetElectrumEnabled { network, enabled } => {
                self.electrum_enabled.insert(network, enabled);
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
    SetHideBalance {
        value: bool,
    },
    SetElectrumEnabled {
        network: bitcoin::Network,
        enabled: ElectrumEnabled,
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
                    "hide_balance" => Mutation::SetHideBalance {
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
                    backup if backup.starts_with("backup_electrum_server_") => {
                        let network = backup.strip_prefix("backup_electrum_server_").unwrap();
                        match bitcoin::Network::from_str(network) {
                            Ok(network) => Mutation::SetBackupElectrumServer {
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
                    enabled if enabled.starts_with("electrum_enabled_") => {
                        let network = enabled.strip_prefix("electrum_enabled_").unwrap();
                        match (
                            bitcoin::Network::from_str(network),
                            ElectrumEnabled::from_str(&value),
                        ) {
                            (Ok(network), Ok(enabled)) => {
                                Mutation::SetElectrumEnabled { network, enabled }
                            }
                            _ => {
                                event!(
                                    Level::WARN,
                                    key = key,
                                    value = value,
                                    "invalid electrum_enabled setting",
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
                Mutation::SetHideBalance { value } => {
                    event!(Level::DEBUG, value = value, "changed hide balance");
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params!["hide_balance", value.to_string()],
                    )?;
                }
                Mutation::SetElectrumEnabled { network, enabled } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        enabled = enabled.to_string(),
                        "set electrum enabled for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![format!("electrum_enabled_{}", network), enabled.to_string()],
                    )?;
                }
            }
        }

        Ok(())
    }
}
