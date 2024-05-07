use anyhow::{Context, Result};
use bdk_chain::{bitcoin, local_chain, tx_graph, ConfirmationTimeHeightAnchor};
use bdk_electrum::{
    electrum_client::{self, Client, ElectrumApi},
    ElectrumExt,
};
use std::sync::{atomic::AtomicUsize, Arc};
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

    pub fn sync(&self, mut start_sync: SyncRequest) -> Result<Update> {
        let electrum_update = self.client.sync(
            start_sync.current_tip.clone(),
            start_sync.take_spks(),
            start_sync.take_txids(),
            vec![],
            10,
        )?;
        let missing = electrum_update
            .relevant_txids
            .missing_full_txs(&start_sync.existing_graph);
        let update_graph = electrum_update
            .relevant_txids
            .into_confirmation_time_tx_graph(&self.client, missing)?;
        let update_chain = electrum_update.chain_update;
        Ok(Update {
            chain: update_chain,
            tx_graph: update_graph,
        })
    }

    pub fn broadcast(&self, tx: &bitcoin::Transaction) -> Result<()> {
        self.client.transaction_broadcast(tx)?;
        Ok(())
    }
}

pub struct Update {
    pub chain: local_chain::CheckPoint,
    pub tx_graph: tx_graph::TxGraph<ConfirmationTimeHeightAnchor>,
}

// Something like this is meant to be part of bdk soon
pub struct SyncRequest {
    pub current_tip: local_chain::CheckPoint,
    spks: Vec<bitcoin::ScriptBuf>,
    txids: Vec<bitcoin::Txid>,
    total_items: usize,
    #[allow(clippy::type_complexity)] // allowing because this will be in bdk eventually
    inspect_spks: Option<Box<dyn FnMut(&bitcoin::ScriptBuf, usize, usize, usize)>>,
    #[allow(clippy::type_complexity)]
    inspect_txids: Option<Box<dyn FnMut(&bitcoin::Txid, usize, usize, usize)>>,
    processed_count: Arc<AtomicUsize>,
    /// this is not meant to be here but BDK electrum desin is a bit off
    existing_graph: tx_graph::TxGraph<ConfirmationTimeHeightAnchor>,
}

impl SyncRequest {
    pub fn new(
        current_tip: local_chain::CheckPoint,
        existing_graph: tx_graph::TxGraph<ConfirmationTimeHeightAnchor>,
    ) -> Self {
        Self {
            current_tip,
            spks: Default::default(),
            txids: Default::default(),
            inspect_spks: Default::default(),
            inspect_txids: Default::default(),
            total_items: Default::default(),
            existing_graph,
            processed_count: Default::default(),
        }
    }

    pub fn add_spks(&mut self, spks: impl IntoIterator<Item = bitcoin::ScriptBuf>) {
        self.spks
            .extend(spks.into_iter().inspect(|_| self.total_items += 1))
    }

    pub fn spks(&self) -> &Vec<bitcoin::ScriptBuf> {
        &self.spks
    }

    pub fn txids(&self) -> &Vec<bitcoin::Txid> {
        &self.txids
    }

    pub fn total_items(&self) -> usize {
        self.total_items
    }

    pub fn take_spks(&mut self) -> impl Iterator<Item = bitcoin::ScriptBuf> {
        fn null_inspect_spks(
            _spks: &bitcoin::ScriptBuf,
            _i: usize,
            _total_processed: usize,
            _total_items: usize,
        ) {
        }
        let total_items = self.total_items;
        let spks = core::mem::take(&mut self.spks);
        let processed_count = self.processed_count.clone();
        let mut inspect = self
            .inspect_spks
            .take()
            .unwrap_or(Box::new(null_inspect_spks));
        spks.into_iter()
            .enumerate()
            .inspect(move |(i, spk)| {
                let processed_total =
                    processed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                inspect(spk, *i, processed_total, total_items);
            })
            .map(|(_, spk)| spk)
    }

    pub fn inspect_spks(
        &mut self,
        inspect: impl FnMut(&bitcoin::ScriptBuf, usize, usize, usize) + 'static,
    ) {
        self.inspect_spks = Some(Box::new(inspect))
    }

    pub fn take_txids(&mut self) -> impl Iterator<Item = bitcoin::Txid> {
        fn null_inspect_txids(
            _spks: &bitcoin::Txid,
            _up_to: usize,
            _total: usize,
            _total_items: usize,
        ) {
        }
        let total_items = self.total_items;
        let processed_count = self.processed_count.clone();
        let txids = core::mem::take(&mut self.txids);
        let mut inspect = self
            .inspect_txids
            .take()
            .unwrap_or(Box::new(null_inspect_txids));
        txids
            .into_iter()
            .enumerate()
            .inspect(move |(i, txid)| {
                let processed_total =
                    processed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                inspect(txid, *i, processed_total, total_items)
            })
            .map(|(_, txid)| txid)
    }

    pub fn inspect_all(
        &mut self,
        inspect: impl Fn(SyncItem, usize, usize, usize) + 'static + Clone,
    ) {
        let inspect_spks = inspect.clone();
        self.inspect_spks(move |spk, i, total, total_items| {
            inspect_spks(SyncItem::Spk(spk), i, total, total_items)
        });
        let inspect_txids = inspect.clone();
        self.inspect_txids(move |txid, i, total, total_items| {
            inspect_txids(SyncItem::Txid(txid), i, total, total_items)
        });
    }

    pub fn add_txids(&mut self, txids: impl IntoIterator<Item = bitcoin::Txid>) {
        self.txids
            .extend(txids.into_iter().inspect(|_| self.total_items += 1));
    }
    pub fn inspect_txids(
        &mut self,
        inspect: impl FnMut(&bitcoin::Txid, usize, usize, usize) + 'static,
    ) {
        self.inspect_txids = Some(Box::new(inspect))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SyncItem<'a> {
    Spk(&'a bitcoin::ScriptBuf),
    Txid(&'a bitcoin::Txid),
}
