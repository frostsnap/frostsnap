use crate::{
    bitcoin::chain_sync::default_electrum_server,
    persist::{Persist, ToStringWrapper},
};
use anyhow::Context as _;
use bdk_chain::bitcoin;
use core::str::FromStr;
use frostsnap_core::KeyId;
use rusqlite::params;
use std::collections::{BTreeMap, HashMap};
use tracing::{event, Level};

#[derive(Default)]
pub struct Settings {
    pub wallet_networks: HashMap<KeyId, bitcoin::Network>,
    pub electrum_servers: BTreeMap<bitcoin::Network, String>,
    pub developer_mode: bool,
}

impl Settings {
    pub fn set_wallet_network(
        &mut self,
        key_id: KeyId,
        network: bitcoin::Network,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(Mutation::SetWalletNetwork { key_id, network }, mutations);
    }

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
            Mutation::SetWalletNetwork { key_id, network } => {
                self.wallet_networks.insert(key_id, network);
            }
            Mutation::SetDeveloperMode { value } => {
                self.developer_mode = value;
            }
            Mutation::SetElectrumServer {
                network,
                url: value,
            } => {
                self.electrum_servers.insert(network, value);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    SetWalletNetwork {
        key_id: KeyId,
        network: bitcoin::Network,
    },
    SetDeveloperMode {
        value: bool,
    },
    SetElectrumServer {
        network: bitcoin::Network,
        url: String,
    },
}

impl Persist<rusqlite::Connection> for Settings {
    type Update = Vec<Mutation>;
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: Self::InitParams) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS fs_wallet_settings (
                key_id TEXT PRIMARY KEY,
                network TEXT
            )",
            [],
        )
        .context("creating fs_wallet_settings table")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS fs_app_global_settings (
                key TEXT PRIMARY KEY,
                value TEXT
             )",
            [],
        )
        .context("creating fs_app_global_settings")?;
        let mut settings = Settings::default();

        {
            let mut stmt = conn.prepare("SELECT key_id, network FROM fs_wallet_settings")?;
            let row_iter = stmt.query_map([], |row| {
                let key_id = row.get::<_, ToStringWrapper<KeyId>>(0)?.0;
                let network = row.get::<_, ToStringWrapper<bitcoin::Network>>(1)?.0;
                Ok((key_id, network))
            })?;

            for row in row_iter {
                let (key_id, network) = row?;
                settings.apply_mutation(Mutation::SetWalletNetwork { key_id, network });
            }
        }

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

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> anyhow::Result<()> {
        for mutation in update {
            match mutation {
                Mutation::SetWalletNetwork { key_id, network } => {
                    event!(
                        Level::DEBUG,
                        name = key_id.to_redacted_string(),
                        network = network.to_string(),
                        "set wallet's network",
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_wallet_settings (key_id, network) VALUES (?1, ?2)",
                        params![ToStringWrapper(key_id), ToStringWrapper(network)],
                    )?;
                }
                Mutation::SetDeveloperMode { value } => {
                    event!(Level::DEBUG, value = value, "changed developer mode");
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params!["developer_mode", value.to_string()],
                    )?;
                }
                Mutation::SetElectrumServer {
                    network,
                    url: value,
                } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        value = value.to_string(),
                        "set electrum server for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![format!("electrum_server_{}", network), value.to_string()],
                    )?;
                }
            }
        }

        Ok(())
    }
}
