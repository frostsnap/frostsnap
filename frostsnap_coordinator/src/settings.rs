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

/// Which chain source follows the chain for a network; Electrum is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChainSource {
    #[default]
    Electrum,
    /// BIP157 compact block filters (desktop only).
    CompactFilters,
}

impl std::fmt::Display for ChainSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainSource::Electrum => write!(f, "electrum"),
            ChainSource::CompactFilters => write!(f, "compact_filters"),
        }
    }
}

impl FromStr for ChainSource {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "electrum" => Ok(ChainSource::Electrum),
            "compact_filters" => Ok(ChainSource::CompactFilters),
            _ => Err(anyhow::anyhow!("invalid chain source value: {s}")),
        }
    }
}

/// Separator between filter peers in the persisted `host:port` list.
const PEER_SEPARATOR: char = ',';

/// Default peer count a filter node waits on before trusting a filter.
const DEFAULT_REQUIRED_PEERS: u8 = 2;

#[derive(Default)]
pub struct Settings {
    pub electrum_servers: BTreeMap<bitcoin::Network, String>,
    pub backup_electrum_servers: BTreeMap<bitcoin::Network, String>,
    pub electrum_enabled: BTreeMap<bitcoin::Network, ElectrumEnabled>,
    pub chain_source: BTreeMap<bitcoin::Network, ChainSource>,
    pub filter_peers: BTreeMap<bitcoin::Network, Vec<String>>,
    pub filter_required_peers: BTreeMap<bitcoin::Network, u8>,
    pub filter_whitelist_only: BTreeMap<bitcoin::Network, bool>,
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

    pub fn get_chain_source(&self, network: bitcoin::Network) -> ChainSource {
        self.chain_source.get(&network).copied().unwrap_or_default()
    }

    pub fn set_chain_source(
        &mut self,
        network: bitcoin::Network,
        source: ChainSource,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(Mutation::SetChainSource { network, source }, mutations)
    }

    pub fn get_filter_peers(&self, network: bitcoin::Network) -> Vec<String> {
        self.filter_peers.get(&network).cloned().unwrap_or_default()
    }

    pub fn set_filter_peers(
        &mut self,
        network: bitcoin::Network,
        peers: Vec<String>,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(Mutation::SetFilterPeers { network, peers }, mutations)
    }

    pub fn get_filter_required_peers(&self, network: bitcoin::Network) -> u8 {
        self.filter_required_peers
            .get(&network)
            .copied()
            .unwrap_or(DEFAULT_REQUIRED_PEERS)
    }

    pub fn set_filter_required_peers(
        &mut self,
        network: bitcoin::Network,
        peers: u8,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(
            Mutation::SetFilterRequiredPeers { network, peers },
            mutations,
        )
    }

    /// Whether to talk only to configured peers, skipping DNS seeds and gossip.
    pub fn get_filter_whitelist_only(&self, network: bitcoin::Network) -> bool {
        self.filter_whitelist_only
            .get(&network)
            .copied()
            .unwrap_or(false)
    }

    pub fn set_filter_whitelist_only(
        &mut self,
        network: bitcoin::Network,
        whitelist_only: bool,
        mutations: &mut Vec<Mutation>,
    ) {
        self.mutate(
            Mutation::SetFilterWhitelistOnly {
                network,
                whitelist_only,
            },
            mutations,
        )
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
            Mutation::SetChainSource { network, source } => {
                self.chain_source.insert(network, source);
            }
            Mutation::SetFilterPeers { network, peers } => {
                self.filter_peers.insert(network, peers);
            }
            Mutation::SetFilterRequiredPeers { network, peers } => {
                self.filter_required_peers.insert(network, peers);
            }
            Mutation::SetFilterWhitelistOnly {
                network,
                whitelist_only,
            } => {
                self.filter_whitelist_only.insert(network, whitelist_only);
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
    SetChainSource {
        network: bitcoin::Network,
        source: ChainSource,
    },
    SetFilterPeers {
        network: bitcoin::Network,
        peers: Vec<String>,
    },
    SetFilterRequiredPeers {
        network: bitcoin::Network,
        peers: u8,
    },
    SetFilterWhitelistOnly {
        network: bitcoin::Network,
        whitelist_only: bool,
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
                    source if source.starts_with("chain_source_") => {
                        let network = source.strip_prefix("chain_source_").unwrap();
                        match (
                            bitcoin::Network::from_str(network),
                            ChainSource::from_str(&value),
                        ) {
                            (Ok(network), Ok(source)) => {
                                Mutation::SetChainSource { network, source }
                            }
                            _ => {
                                event!(
                                    Level::WARN,
                                    key = key,
                                    value = value,
                                    "invalid chain_source setting",
                                );
                                continue;
                            }
                        }
                    }
                    required if required.starts_with("filter_required_peers_") => {
                        let network = required.strip_prefix("filter_required_peers_").unwrap();
                        match (bitcoin::Network::from_str(network), u8::from_str(&value)) {
                            (Ok(network), Ok(peers)) => {
                                Mutation::SetFilterRequiredPeers { network, peers }
                            }
                            _ => {
                                event!(
                                    Level::WARN,
                                    key = key,
                                    value = value,
                                    "invalid filter_required_peers setting",
                                );
                                continue;
                            }
                        }
                    }
                    whitelist if whitelist.starts_with("filter_whitelist_only_") => {
                        let network = whitelist.strip_prefix("filter_whitelist_only_").unwrap();
                        match (bitcoin::Network::from_str(network), bool::from_str(&value)) {
                            (Ok(network), Ok(whitelist_only)) => Mutation::SetFilterWhitelistOnly {
                                network,
                                whitelist_only,
                            },
                            _ => {
                                event!(
                                    Level::WARN,
                                    key = key,
                                    value = value,
                                    "invalid filter_whitelist_only setting",
                                );
                                continue;
                            }
                        }
                    }
                    peers if peers.starts_with("filter_peers_") => {
                        let network = peers.strip_prefix("filter_peers_").unwrap();
                        match bitcoin::Network::from_str(network) {
                            Ok(network) => Mutation::SetFilterPeers {
                                network,
                                peers: value
                                    .split(PEER_SEPARATOR)
                                    .map(str::trim)
                                    .filter(|peer| !peer.is_empty())
                                    .map(str::to_owned)
                                    .collect(),
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
                Mutation::SetChainSource { network, source } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        source = source.to_string(),
                        "set chain source for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![format!("chain_source_{}", network), source.to_string()],
                    )?;
                }
                Mutation::SetFilterPeers { network, peers } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        count = peers.len(),
                        "set compact filter peers for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![
                            format!("filter_peers_{}", network),
                            peers.join(&PEER_SEPARATOR.to_string())
                        ],
                    )?;
                }
                Mutation::SetFilterRequiredPeers { network, peers } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        peers = peers,
                        "set required peer count for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![format!("filter_required_peers_{}", network), peers.to_string()],
                    )?;
                }
                Mutation::SetFilterWhitelistOnly {
                    network,
                    whitelist_only,
                } => {
                    event!(
                        Level::DEBUG,
                        network = network.to_string(),
                        whitelist_only = whitelist_only,
                        "set whitelist-only peering for network"
                    );
                    conn.execute(
                        "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
                        params![
                            format!("filter_whitelist_only_{}", network),
                            whitelist_only.to_string()
                        ],
                    )?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::persist::Persist;

    const NETWORK: bitcoin::Network = bitcoin::Network::Bitcoin;

    fn store() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        Settings::migrate(&mut conn).unwrap();
        conn
    }

    fn round_trip(settings: &Settings, mutations: Vec<Mutation>) -> Settings {
        let mut conn = store();
        settings.persist_update(&mut conn, mutations).unwrap();
        Settings::load(&mut conn, ()).unwrap()
    }

    #[test]
    fn a_database_that_has_never_heard_of_chain_sources_reads_as_electrum() {
        let mut conn = store();
        let loaded = Settings::load(&mut conn, ()).unwrap();
        assert_eq!(loaded.get_chain_source(NETWORK), ChainSource::Electrum);
        assert_eq!(loaded.get_filter_required_peers(NETWORK), 2);
        assert!(!loaded.get_filter_whitelist_only(NETWORK));
        assert!(loaded.get_filter_peers(NETWORK).is_empty());
    }

    #[test]
    fn a_chosen_chain_source_survives_a_restart() {
        let mut settings = Settings::default();
        let mut mutations = vec![];
        settings.set_chain_source(NETWORK, ChainSource::CompactFilters, &mut mutations);
        let loaded = round_trip(&settings, mutations);
        assert_eq!(
            loaded.get_chain_source(NETWORK),
            ChainSource::CompactFilters
        );
    }

    #[test]
    fn filter_settings_survive_a_restart() {
        let mut settings = Settings::default();
        let mut mutations = vec![];
        settings.set_filter_peers(
            NETWORK,
            vec!["10.0.0.1:8333".into(), "node.example:8333".into()],
            &mut mutations,
        );
        settings.set_filter_required_peers(NETWORK, 5, &mut mutations);
        settings.set_filter_whitelist_only(NETWORK, true, &mut mutations);
        let loaded = round_trip(&settings, mutations);
        assert_eq!(
            loaded.get_filter_peers(NETWORK),
            vec!["10.0.0.1:8333".to_string(), "node.example:8333".to_string()]
        );
        assert_eq!(loaded.get_filter_required_peers(NETWORK), 5);
        assert!(loaded.get_filter_whitelist_only(NETWORK));
    }

    #[test]
    fn clearing_the_peer_list_leaves_no_peers() {
        let mut settings = Settings::default();
        let mut mutations = vec![];
        settings.set_filter_peers(NETWORK, vec!["10.0.0.1:8333".into()], &mut mutations);
        settings.set_filter_peers(NETWORK, vec![], &mut mutations);
        let loaded = round_trip(&settings, mutations);
        assert!(loaded.get_filter_peers(NETWORK).is_empty());
    }

    #[test]
    fn networks_keep_their_own_chain_source() {
        let mut settings = Settings::default();
        let mut mutations = vec![];
        settings.set_chain_source(
            bitcoin::Network::Signet,
            ChainSource::CompactFilters,
            &mut mutations,
        );
        let loaded = round_trip(&settings, mutations);
        assert_eq!(
            loaded.get_chain_source(bitcoin::Network::Signet),
            ChainSource::CompactFilters
        );
        assert_eq!(
            loaded.get_chain_source(bitcoin::Network::Bitcoin),
            ChainSource::Electrum
        );
    }

    #[test]
    fn an_unrecognised_key_is_skipped_rather_than_fatal() {
        let mut conn = store();
        conn.execute(
            "INSERT OR REPLACE INTO fs_app_global_settings (key, value) VALUES (?1, ?2)",
            params!["something_from_the_future_bitcoin", "1"],
        )
        .unwrap();
        let loaded = Settings::load(&mut conn, ()).unwrap();
        assert_eq!(loaded.get_chain_source(NETWORK), ChainSource::Electrum);
    }
}
