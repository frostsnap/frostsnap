use anyhow::{anyhow, Result};
use bitcoin::constants::genesis_block;
use bitcoin::Network as BitcoinNetwork;
use flutter_rust_bridge::frb;
use frostsnap_coordinator::bitcoin::chain_sync::{ChainClient, SUPPORTED_NETWORKS};
pub use frostsnap_coordinator::bitcoin::chain_sync::{ChainStatus, ChainStatusState};
use frostsnap_coordinator::persist::Persisted;
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
            let (chain_api, conn_handler) = ChainClient::new(genesis_hash);
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
                                    stream.add(txs.into()).unwrap();
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

    pub fn check_and_set_electrum_server(
        &mut self,
        network: BitcoinNetwork,
        url: String,
        is_backup: bool,
    ) -> Result<()> {
        let chain_api = self
            .chain_clients
            .get(&network)
            .ok_or_else(|| anyhow!("network not supported {}", network))?;
        chain_api.check_and_set_electrum_server_url(url.clone(), is_backup)?;
        let mut db = self.db.lock().unwrap();
        self.settings.mutate2(&mut *db, |settings, update| {
            settings.set_electrum_server(network, url, update);
            Ok(())
        })?;

        self.emit_electrum_settings();
        Ok(())
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

pub struct ElectrumServer {
    pub network: BitcoinNetwork,
    pub url: String,
    pub backup_url: String,
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
                ElectrumServer {
                    network,
                    url,
                    backup_url,
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
    Connected,
    Disconnected,
    Connecting,
}
