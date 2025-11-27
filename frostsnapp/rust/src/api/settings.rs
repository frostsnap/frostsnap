use anyhow::{anyhow, Result};
use bitcoin::constants::genesis_block;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::bitcoin::chain_sync::{ChainClient, SUPPORTED_NETWORKS};
pub use frostsnap_coordinator::bitcoin::chain_sync::{
    ChainStatus, ChainStatusState, ConnectionResult,
};
pub use frostsnap_coordinator::bitcoin::tofu::verifier::UntrustedCertificate;
use frostsnap_coordinator::persist::Persisted;
pub use frostsnap_coordinator::settings::ElectrumEnabled;
use frostsnap_coordinator::settings::Settings as RSettings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::frb_generated::StreamSink;
use crate::sink_wrap::SinkWrap;

use super::super_wallet::SuperWallet;

#[frb(opaque)]
pub struct Settings {
    settings: Persisted<RSettings>,
    db: Arc<Mutex<rusqlite::Connection>>,
    chain_clients: HashMap<BitcoinNetwork, ChainClient>,

    #[allow(unused)]
    app_directory: PathBuf,
    loaded_wallets: HashMap<BitcoinNetwork, SuperWallet>,

    developer_settings_stream: Option<StreamSink<DeveloperSettings>>,
    display_settings_stream: Option<StreamSink<DisplaySettings>>,
    electrum_settings_stream: Option<StreamSink<ElectrumSettings>>,
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
            let electrum_url = persisted.get_electrum_server(network);
            let backup_electrum_url = persisted.get_backup_electrum_server(network);

            let genesis_hash = genesis_block(bitcoin::params::Params::new(network)).block_hash();

            // Load trusted certificates for this network from the database
            let trusted_certificates = {
                let mut db_ = db.lock().unwrap();
                use frostsnap_coordinator::bitcoin::tofu::trusted_certs::TrustedCertificates;
                use frostsnap_coordinator::persist::Persisted;
                Persisted::<TrustedCertificates>::new(&mut *db_, network)?
            };

            let (chain_api, conn_handler) =
                ChainClient::new(genesis_hash, trusted_certificates, db.clone());
            let super_wallet =
                SuperWallet::load_or_new(&app_directory, network, chain_api.clone())?;
            // FIXME: the dependency relationship here is overly convoluted.
            thread::spawn({
                let super_wallet = super_wallet.clone();
                move || {
                    conn_handler.run(
                        electrum_url,
                        backup_electrum_url,
                        super_wallet.inner.clone(),
                        {
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
                        },
                    )
                }
            });
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

        match chain_api.check_and_set_electrum_server_url(url.clone(), is_backup)? {
            ConnectionResult::Success => {
                // Connection succeeded, persist the setting
                let mut db = self.db.lock().unwrap();
                self.settings.mutate2(&mut *db, |settings, update| {
                    if is_backup {
                        settings.set_backup_electrum_server(network, url, update);
                    } else {
                        settings.set_electrum_server(network, url, update);
                    }
                    Ok(())
                })?;
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
        chain_api.trust_certificate(server_url.clone(), certificate);

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

        chain_api.set_urls(primary, backup);

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

        chain_api.set_enabled(enabled);

        self.emit_electrum_settings();
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
    pub electrum_url: String,
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
