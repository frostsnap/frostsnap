use anyhow::{anyhow, Result};
use bitcoin::constants::genesis_block;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::bitcoin::chain_sync::{ChainClient, ElectrumConfig, SUPPORTED_NETWORKS};
pub use frostsnap_coordinator::bitcoin::chain_sync::{
    ChainStatus, ChainStatusState, ConnectionResult,
};
pub use frostsnap_coordinator::bitcoin::tofu::verifier::UntrustedCertificate;
use frostsnap_coordinator::persist::Persisted;
use frostsnap_coordinator::settings::Settings as RSettings;
pub use frostsnap_coordinator::settings::{ChainSource, ElectrumEnabled};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::frb_generated::StreamSink;
use crate::sink_wrap::SinkWrap;

use super::super_wallet::SuperWallet;
use crate::chain_api::ChainApi;
use frostsnap_coordinator::bitcoin::backend::ChainBackend;

#[frb(opaque)]
pub struct Settings {
    settings: Persisted<RSettings>,
    db: Arc<Mutex<rusqlite::Connection>>,
    chain_clients: HashMap<BitcoinNetwork, ChainApi>,

    #[allow(unused)]
    app_directory: PathBuf,
    loaded_wallets: HashMap<BitcoinNetwork, SuperWallet>,

    developer_settings_stream: Option<StreamSink<DeveloperSettings>>,
    display_settings_stream: Option<StreamSink<DisplaySettings>>,
    electrum_settings_stream: Option<StreamSink<ElectrumSettings>>,
}

/// The callback both handlers use to push freshly-seen transactions at the UI.
fn tx_sink(
    super_wallet: &SuperWallet,
) -> impl FnMut(frostsnap_core::MasterAppkey, Vec<frostsnap_coordinator::bitcoin::wallet::Transaction>)
       + Send
       + 'static {
    let wallet_streams = super_wallet.wallet_streams.clone();
    move |master_appkey, txs| {
        let wallet_streams = wallet_streams.lock().unwrap();
        if let Some(stream) = wallet_streams.get(&master_appkey) {
            if let Err(err) = stream.add(txs.into()) {
                tracing::error!(
                    {
                        master_appkey = master_appkey.to_redacted_string(),
                        err = err.to_string(),
                    },
                    "Failed to add txs to stream"
                );
            }
        }
    }
}

/// Build the configured chain source and its wallet; a change takes effect at the next launch.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn build_chain_api(
    settings: &RSettings,
    db: &Arc<Mutex<rusqlite::Connection>>,
    app_directory: &PathBuf,
    network: BitcoinNetwork,
) -> Result<(ChainApi, SuperWallet)> {
    match settings.get_chain_source(network) {
        ChainSource::Electrum => build_electrum(settings, db, app_directory, network),
        ChainSource::CompactFilters => build_compact_filters(settings, app_directory, network),
    }
}

/// Mobile has no filter node; falls back to Electrum so a desktop-synced wallet still opens.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn build_chain_api(
    settings: &RSettings,
    db: &Arc<Mutex<rusqlite::Connection>>,
    app_directory: &PathBuf,
    network: BitcoinNetwork,
) -> Result<(ChainApi, SuperWallet)> {
    if matches!(
        settings.get_chain_source(network),
        ChainSource::CompactFilters
    ) {
        tracing::error!(
            network = network.to_string(),
            "compact block filters are not available on this platform; using Electrum instead"
        );
    }
    build_electrum(settings, db, app_directory, network)
}

fn build_electrum(
    settings: &RSettings,
    db: &Arc<Mutex<rusqlite::Connection>>,
    app_directory: &PathBuf,
    network: BitcoinNetwork,
) -> Result<(ChainApi, SuperWallet)> {
    let electrum_config = ElectrumConfig {
        enabled: settings.get_electrum_enabled(network),
        primary: settings.get_electrum_server(network),
        backup: settings.get_backup_electrum_server(network),
    };
    let genesis_hash = genesis_block(bitcoin::params::Params::new(network)).block_hash();
    let trusted_certificates = {
        let mut db_ = db.lock().unwrap();
        use frostsnap_coordinator::bitcoin::tofu::trusted_certs::TrustedCertificates;
        Persisted::<TrustedCertificates>::new(&mut *db_, network)?
    };
    let (client, conn_handler) = ChainClient::new(
        genesis_hash,
        electrum_config,
        trusted_certificates,
        db.clone(),
    );
    let chain_api = ChainApi::Electrum(client);
    let super_wallet = SuperWallet::load_or_new(app_directory, network, chain_api.clone())?;
    thread::spawn({
        let super_wallet = super_wallet.clone();
        move || conn_handler.run(super_wallet.inner.clone(), tx_sink(&super_wallet))
    });
    Ok((chain_api, super_wallet))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn build_compact_filters(
    settings: &RSettings,
    app_directory: &PathBuf,
    network: BitcoinNetwork,
) -> Result<(ChainApi, SuperWallet)> {
    use frostsnap_coordinator::bitcoin::compact_filters::node::{self, FilterConfig};
    let (client, handler) = node::new(FilterConfig::from_settings(settings, network))?;
    let chain_api = ChainApi::CompactFilters(client);
    let super_wallet = SuperWallet::load_or_new(app_directory, network, chain_api.clone())?;
    thread::spawn({
        let super_wallet = super_wallet.clone();
        move || {
            if let Err(err) = handler.run(super_wallet.inner.clone(), tx_sink(&super_wallet)) {
                tracing::error!(err = err.to_string(), "compact filter node stopped");
            }
        }
    });
    Ok((chain_api, super_wallet))
}

macro_rules! settings_impl {
    ($stream_name:ident, $stream_emit_name:ident, $stream_sub:ident, $type_name:ident) => {
        pub fn $stream_sub(&mut self, stream: StreamSink<$type_name>) -> Result<()> {
            self.$stream_name.replace(stream);
            self.$stream_emit_name();
            Ok(())
        }

        fn $stream_emit_name(&self) {
            if let Some(stream) = &self.$stream_name {
                stream
                    .add(<$type_name>::from_settings(&self.settings))
                    .unwrap();
            }
        }
    };
}

impl Settings {
    pub(crate) fn new(
        db: Arc<Mutex<rusqlite::Connection>>,
        app_directory: PathBuf,
    ) -> anyhow::Result<Self> {
        let persisted: Persisted<RSettings> = {
            let mut db_ = db.lock().unwrap();
            Persisted::new(&mut *db_, ())?
        };

        let mut loaded_wallets: HashMap<BitcoinNetwork, SuperWallet> = Default::default();
        let mut chain_apis = HashMap::new();

        for network in SUPPORTED_NETWORKS {
            let (chain_api, super_wallet) =
                build_chain_api(&persisted, &db, &app_directory, network)?;
            loaded_wallets.insert(network, super_wallet);
            chain_apis.insert(network, chain_api);
        }

        Ok(Self {
            loaded_wallets,
            settings: persisted,
            app_directory,
            chain_clients: chain_apis,
            developer_settings_stream: Default::default(),
            display_settings_stream: Default::default(),
            electrum_settings_stream: Default::default(),
            db,
        })
    }

    settings_impl!(
        developer_settings_stream,
        emit_developer_settings,
        sub_developer_settings,
        DeveloperSettings
    );

    settings_impl!(
        display_settings_stream,
        emit_display_settings,
        sub_display_settings,
        DisplaySettings
    );

    settings_impl!(
        electrum_settings_stream,
        emit_electrum_settings,
        sub_electrum_settings,
        ElectrumSettings
    );

    #[frb(sync)]
    pub fn get_super_wallet(&self, network: BitcoinNetwork) -> Result<SuperWallet> {
        self.loaded_wallets
            .get(&network)
            .cloned()
            .ok_or(anyhow!("unsupported network {:?}", network))
    }

    pub fn set_developer_mode(&mut self, value: bool) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        self.settings.mutate2(&mut *db, |settings, update| {
            settings.set_developer_mode(value, update);
            Ok(())
        })?;

        self.emit_developer_settings();

        Ok(())
    }

    #[frb(sync)]
    pub fn is_in_developer_mode(&self) -> bool {
        self.settings.developer_mode
    }

    pub fn set_hide_balance(&mut self, value: bool) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        self.settings.mutate2(&mut *db, |settings, update| {
            settings.set_hide_balance(value, update);
            Ok(())
        })?;

        self.emit_display_settings();

        Ok(())
    }

    #[frb(sync)]
    pub fn hide_balance(&self) -> bool {
        self.settings.hide_balance
    }

    pub fn check_and_set_electrum_server(
        &mut self,
        network: BitcoinNetwork,
        url: String,
        is_backup: bool,
    ) -> Result<ConnectionResult> {
        let chain_api = self
            .chain_clients
            .get(&network)
            .ok_or_else(|| anyhow!("network not supported {}", network))?;

        match chain_api
            .electrum()?
            .check_and_set_electrum_server_url(url.clone(), is_backup)?
        {
            ConnectionResult::Success => {
                // Persist first — the persisted config is authoritative — then push the new
                // url into the live watch (which triggers a reconnect) and emit. Ordering it
                // this way means a persistence failure never leaves the handler on a url that
                // wasn't saved.
                {
                    let mut db = self.db.lock().unwrap();
                    self.settings.mutate2(&mut *db, |settings, update| {
                        if is_backup {
                            settings.set_backup_electrum_server(network, url.clone(), update);
                        } else {
                            settings.set_electrum_server(network, url.clone(), update);
                        }
                        Ok(())
                    })?;
                }
                chain_api.electrum()?.set_electrum_url(url, is_backup);
                self.emit_electrum_settings();
                Ok(ConnectionResult::Success)
            }
            result => {
                // Return TOFU prompt or failure without persisting
                Ok(result)
            }
        }
    }

    pub fn accept_certificate_and_retry(
        &mut self,
        network: BitcoinNetwork,
        server_url: String,
        certificate: Vec<u8>,
        is_backup: bool,
    ) -> Result<ConnectionResult> {
        // Use message-passing to trust the certificate
        let chain_api = self
            .chain_clients
            .get(&network)
            .ok_or_else(|| anyhow!("network not supported {}", network))?;

        // Send the trust certificate message - the backend will handle persistence
        chain_api
            .electrum()?
            .trust_certificate(server_url.clone(), certificate);

        // Retry connection now that we've trusted the certificate
        self.check_and_set_electrum_server(network, server_url, is_backup)
    }

    pub fn subscribe_chain_status(
        &self,
        network: BitcoinNetwork,
        sink: StreamSink<ChainStatus>,
    ) -> Result<()> {
        let chain_api = self
            .chain_clients
            .get(&network)
            .ok_or_else(|| anyhow!("network not supported {}", network))?;

        chain_api.set_status_sink(Box::new(SinkWrap(sink)));
        Ok(())
    }

    pub fn set_electrum_servers(
        &mut self,
        network: BitcoinNetwork,
        primary: String,
        backup: String,
    ) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        self.settings.mutate2(&mut *db, |settings, update| {
            settings.set_electrum_server(network, primary.clone(), update);
            settings.set_backup_electrum_server(network, backup.clone(), update);
            Ok(())
        })?;

        let chain_api = self
            .chain_clients
            .get(&network)
            .ok_or_else(|| anyhow!("network not supported {}", network))?;

        chain_api.electrum()?.set_urls(primary, backup);

        self.emit_electrum_settings();
        Ok(())
    }

    pub fn set_electrum_enabled(
        &mut self,
        network: BitcoinNetwork,
        enabled: ElectrumEnabled,
    ) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        self.settings.mutate2(&mut *db, |settings, update| {
            settings.set_electrum_enabled(network, enabled, update);
            Ok(())
        })?;

        let chain_api = self
            .chain_clients
            .get(&network)
            .ok_or_else(|| anyhow!("network not supported {}", network))?;

        chain_api.electrum()?.set_enabled(enabled);

        self.emit_electrum_settings();
        Ok(())
    }

    /// Configured chain source; changes apply at the next launch.
    #[frb(sync)]
    pub fn get_chain_source(&self, network: BitcoinNetwork) -> ChainSource {
        self.settings.get_chain_source(network)
    }

    /// Whether this build ships the compact filter node.
    #[frb(sync)]
    pub fn compact_filters_available(&self) -> bool {
        cfg!(not(any(target_os = "android", target_os = "ios")))
    }

    /// Choose the chain source for a network; refused on platforms without a filter node.
    pub fn set_chain_source(&mut self, network: BitcoinNetwork, source: ChainSource) -> Result<()> {
        if matches!(source, ChainSource::CompactFilters) && !self.compact_filters_available() {
            return Err(anyhow!(
                "compact block filters are not available on this platform"
            ));
        }
        let mut db = self.db.lock().unwrap();
        self.settings.mutate2(&mut *db, |settings, update| {
            settings.set_chain_source(network, source, update);
            Ok(())
        })?;
        Ok(())
    }

    pub fn connect_to(&self, network: BitcoinNetwork, use_backup: bool) -> Result<()> {
        let chain_api = self
            .chain_clients
            .get(&network)
            .ok_or_else(|| anyhow!("network not supported {}", network))?;

        chain_api.electrum()?.connect_to(use_backup);
        Ok(())
    }
}

pub struct DeveloperSettings {
    pub developer_mode: bool,
}

impl DeveloperSettings {
    fn from_settings(settings: &RSettings) -> Self {
        DeveloperSettings {
            developer_mode: settings.developer_mode,
        }
    }
}

pub struct DisplaySettings {
    pub hide_balance: bool,
}

impl DisplaySettings {
    fn from_settings(settings: &RSettings) -> Self {
        DisplaySettings {
            hide_balance: settings.hide_balance,
        }
    }
}

pub struct ElectrumServer {
    pub network: BitcoinNetwork,
    pub url: String,
    pub backup_url: String,
    pub enabled: ElectrumEnabled,
}

pub struct ElectrumSettings {
    pub electrum_servers: Vec<ElectrumServer>,
}

impl ElectrumSettings {
    fn from_settings(settings: &RSettings) -> Self {
        let electrum_servers = SUPPORTED_NETWORKS
            .into_iter()
            .map(|network| {
                let url = settings.get_electrum_server(network);
                let backup_url = settings.get_backup_electrum_server(network);
                let enabled = settings.get_electrum_enabled(network);
                ElectrumServer {
                    network,
                    url,
                    backup_url,
                    enabled,
                }
            })
            .collect::<Vec<_>>();
        ElectrumSettings { electrum_servers }
    }
}

#[frb(mirror(ChainStatus))]
pub struct _ChainStatus {
    pub primary_url: String,
    pub backup_url: String,
    pub on_backup: bool,
    pub state: ChainStatusState,
}

#[frb(mirror(ChainStatusState))]
pub enum _ChainStatusState {
    Idle,
    Connecting,
    Connected,
    Disconnected,
}

#[frb(mirror(ConnectionResult))]
pub enum _ConnectionResult {
    Success,
    CertificatePromptNeeded(UntrustedCertificate),
    Failed(String),
}

#[frb(mirror(UntrustedCertificate))]
pub struct _UntrustedCertificate {
    pub fingerprint: String,
    pub server_url: String,
    pub is_changed: bool,
    pub old_fingerprint: Option<String>,
    pub certificate_der: Vec<u8>,
    pub valid_for_names: Option<Vec<String>>,
}

#[frb(mirror(ElectrumEnabled))]
pub enum _ElectrumEnabled {
    All,
    PrimaryOnly,
    None,
}

/// Dart mirror of `ChainSource`; keep variants in step.
#[frb(mirror(ChainSource))]
pub enum _ChainSource {
    Electrum,
    CompactFilters,
}
