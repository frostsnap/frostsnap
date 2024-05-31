use anyhow::{Context, Result};
pub use bdk_chain::spk_client::SyncRequest;
use bdk_chain::{bitcoin, spk_client, ConfirmationTimeHeightAnchor};
use bdk_electrum::{
    electrum_client::{self, Client, ElectrumApi},
    ElectrumExt,
};
use std::sync::Arc;
use tracing::{event, Level};

#[derive(Clone)]
pub struct ChainSync {
    client: Arc<Client>,
}

impl ChainSync {
    pub fn new(network: bitcoin::Network) -> Result<Self> {
        let electrum_url = match network {
            bitcoin::Network::Bitcoin => "ssl://electrum.blockstream.info:50002",
            bitcoin::Network::Testnet => "ssl://electrum.blockstream.info:60002",
            bitcoin::Network::Regtest => "tcp://localhost:60401",
            bitcoin::Network::Signet => "tcp://signet-electrumx.wakiyamap.dev:50001",
            _ => panic!("Unknown network"),
        };

        let config = electrum_client::Config::builder()
            .validate_domain(matches!(network, bitcoin::Network::Bitcoin))
            .build();

        event!(
            Level::INFO,
            url = electrum_url,
            "initializing to electrum server"
        );
        let electrum_client = Client::from_config(electrum_url, config)
            .context(format!("initializing electrum client to {}", electrum_url))?;
        event!(
            Level::INFO,
            url = electrum_url,
            "initializing electrum server successful"
        );

        Ok(Self {
            client: Arc::new(electrum_client),
        })
    }

    pub fn sync(
        &self,
        sync_request: SyncRequest,
    ) -> Result<spk_client::SyncResult<ConfirmationTimeHeightAnchor>> {
        let electrum_update = self.client.sync(sync_request, 10, true)?;
        Ok(electrum_update.with_confirmation_time_height_anchor(self.client.as_ref())?)
    }

    pub fn broadcast(&self, tx: &bitcoin::Transaction) -> Result<()> {
        self.client.transaction_broadcast(tx)?;
        Ok(())
    }
}
