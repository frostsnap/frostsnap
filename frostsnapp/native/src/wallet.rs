use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin,
    indexed_tx_graph::IndexedTxGraph,
    keychain::KeychainTxOutIndex,
    local_chain::{self, LocalChain},
    miniscript::{
        descriptor::{DescriptorXKey, Tr, Wildcard},
        Descriptor, DescriptorPublicKey,
    },
    tx_graph::{self},
    Append, ChainPosition, ConfirmationTimeHeightAnchor,
};
use flutter_rust_bridge::RustOpaque;
use frostsnap_coordinator::frostsnap_core::{
    self,
    schnorr_fun::{frost::FrostKey, fun::marker::Normal},
    FrostKeyExt, KeyId,
};
use llsdb::{IndexHandle, LinkedList, LlsDb};
use std::{
    collections::VecDeque,
    fs::File,
    sync::{Arc, Mutex},
};
use tracing::{event, Level};

use crate::{
    api,
    chain_sync::{self, SyncRequest},
    persist_core::PersistCore,
};

// Flutter rust bridge is annoyed if I call this `Wallet`
pub struct _Wallet {
    graph: IndexedTxGraph<ConfirmationTimeHeightAnchor, KeychainTxOutIndex<KeyId>>,
    chain: LocalChain,
    pub network: bitcoin::Network,
    db: Arc<Mutex<LlsDb<File>>>,
    chain_list_handle: LinkedList<bincode::serde::Compat<ChainChangeSet>>,
    tx_graph_list_handle: LinkedList<bincode::serde::Compat<TxGraphChangeSet>>,
    keychain_handle: IndexHandle<llsdb::index::BTreeMap<KeyId, u32>>,
    persist_core: IndexHandle<PersistCore>,
}

pub type TxGraphChangeSet = tx_graph::ChangeSet<ConfirmationTimeHeightAnchor>;
pub type ChainChangeSet = local_chain::ChangeSet;

impl _Wallet {
    pub fn load_or_init(
        db: Arc<Mutex<LlsDb<File>>>,
        network: bitcoin::Network,
        persist_core: IndexHandle<PersistCore>,
    ) -> anyhow::Result<Self> {
        event!(Level::INFO, "initializing wallet");
        let mut db_ = db.lock().unwrap();
        let (chain, graph, network, tx_graph_list_handle, chain_list_handle, keychain_handle) = db_
            .execute(|tx| {
                // store the graph and chain data in differnet lists so they're easier to work with e.g.
                // can delete all tx data without deleting chain data and vis versa
                let chain_list_handle = tx
                    .take_list::<bincode::serde::Compat<crate::wallet::ChainChangeSet>>(
                        "wallet/chain",
                    )
                    .context("loading chain list")?;
                let tx_graph_list_handle = tx
                    .take_list::<bincode::serde::Compat<crate::wallet::TxGraphChangeSet>>(
                        "wallet/tx_graph",
                    )
                    .context("loading tx data list")?;

                let keychain_list_handle = tx
                    .take_list::<(KeyId, u32)>("wallet/keychain")
                    .context("loading keychain list")?;
                let keychain_index = llsdb::index::BTreeMap::new(keychain_list_handle, &tx)
                    .context("indexing keychain list")?;
                let chain_list = chain_list_handle.api(&tx);
                let tx_list = tx_graph_list_handle.api(&tx);
                let keychain_handle = tx.store_index(keychain_index);
                let keychain = tx.take_index(keychain_handle);
                let (chain, graph) = if chain_list.is_empty() {
                    event!(
                        Level::INFO,
                        "seting up wallet for the first time -- writing to disk"
                    );
                    let genesis_hash = bitcoin::constants::genesis_block(network).block_hash();
                    let (chain, chain_changeset) = LocalChain::from_genesis_hash(genesis_hash);

                    let graph = Default::default();
                    chain_list.push(&bincode::serde::Compat(chain_changeset))?;
                    (chain, graph)
                } else {
                    event!(Level::INFO, "loading existing wallet from disk");
                    let chain_data = {
                        // unfortunately, local_chain::ChangeSet is not monotone so the order matters,
                        // so we have to reverse the iterator
                        let mut changes = VecDeque::new();
                        for changeset in chain_list.iter() {
                            changes.push_front(changeset?)
                        }
                        let mut full_changeset = crate::wallet::ChainChangeSet::default();
                        for mut changeset in changes {
                            full_changeset.append(&mut changeset.0);
                        }
                        full_changeset
                    };

                    let tx_data = tx_list.iter().try_fold(
                        TxGraphChangeSet::default(),
                        |mut acc, next| -> anyhow::Result<_> {
                            acc.append(next?.0);
                            Ok(acc)
                        },
                    )?;

                    let mut graph = IndexedTxGraph::<
                        ConfirmationTimeHeightAnchor,
                        KeychainTxOutIndex<KeyId>,
                    >::default();

                    let persisted_frost_keys = tx
                        .take_index(persist_core)
                        .frost_keys()
                        .context("reading persisted frost keys")?;
                    for frost_key in persisted_frost_keys {
                        graph
                            .index
                            .add_keychain(frost_key.key_id(), Self::get_descriptor(&frost_key))
                    }

                    let keychain_data = keychain.iter().collect::<Result<_>>()?;
                    let _ = graph.index.reveal_to_target_multi(&keychain_data);

                    graph.apply_changeset(tx_data.into());
                    let chain = LocalChain::from_changeset(chain_data)?;
                    (chain, graph)
                };
                Ok((
                    chain,
                    graph,
                    network,
                    tx_graph_list_handle,
                    chain_list_handle,
                    keychain_handle,
                ))
            })
            .context("Initializing wallet from ")?;

        event!(Level::INFO, "wallet initialization finished");

        drop(db_);
        Ok(Self {
            db,
            chain,
            graph,
            network,
            tx_graph_list_handle,
            chain_list_handle,
            keychain_handle,
            persist_core,
        })
    }

    fn get_descriptor(frost_key: &FrostKey<Normal>) -> Descriptor<DescriptorPublicKey> {
        use bitcoin::bip32::DerivationPath;
        let frost_xpub = frostsnap_core::xpub::Xpub::new(frost_key.clone());
        let key = DescriptorPublicKey::XPub(DescriptorXKey {
            origin: None,
            xkey: *frost_xpub.xpub(),
            derivation_path: DerivationPath::master(),
            wildcard: Wildcard::Unhardened,
        });

        let tr = Tr::new(key, None).expect("infallible since it's None");
        Descriptor::Tr(tr)
    }

    fn ensure_key_exists(&mut self, key_id: KeyId) -> Result<()> {
        if !self.graph.index.keychains().contains_key(&key_id) {
            let frost_keys = self
                .db
                .lock()
                .unwrap()
                .execute(|tx| tx.take_index(self.persist_core).frost_keys())?;
            let found = frost_keys
                .into_iter()
                .find(|frost_key| frost_key.key_id() == key_id)
                .ok_or(anyhow!("key {key_id} doesn't exist in database"))?;
            self.graph
                .index
                .add_keychain(key_id, Self::get_descriptor(&found));
        }
        Ok(())
    }

    pub fn list_addresses(&self, key_id: KeyId) -> Vec<api::Address> {
        self.graph
            .index
            .revealed_keychain_spks(&key_id)
            .rev()
            .map(|(i, spk)| api::Address {
                index: i,
                address_string: bitcoin::Address::from_script(spk, self.network)
                    .expect("has address form")
                    .to_string(),
                used: self.graph.index.is_used(key_id, i),
            })
            .collect()
    }

    pub fn next_address(&mut self, key_id: KeyId) -> Result<api::Address> {
        // reveal_next_spk will panic if the key doesn't exist
        self.ensure_key_exists(key_id)?;
        let ((index, spk), changeset) = self.graph.index.reveal_next_spk(&key_id);
        self.db.lock().unwrap().execute(|tx| {
            let mut keychain = tx.take_index(self.keychain_handle);
            for (key_id, derivation_index) in changeset.0 {
                keychain.insert(key_id, &derivation_index)?;
            }
            Ok(())
        })?;
        // TODO: There should be a way of unrevealing index if we fail to persist:
        // https://github.com/bitcoindevkit/bdk/issues/1322
        Ok(api::Address {
            index,
            address_string: bitcoin::Address::from_script(spk, self.network)
                .expect("has address form")
                .to_string(),
            used: self.graph.index.is_used(key_id, index),
        })
    }

    pub fn list_transactions(&mut self, key_id: KeyId) -> Vec<api::Transaction> {
        let mut txs = self
            .graph
            .graph()
            .list_chain_txs(&self.chain, self.chain.tip().block_id())
            .collect::<Vec<_>>();

        txs.sort_unstable_by_key(|tx| core::cmp::Reverse(tx.chain_position));

        txs.into_iter()
            .filter_map(|canonical_tx| {
                let confirmation_time = match canonical_tx.chain_position {
                    ChainPosition::Confirmed(conf_time) => Some(api::ConfirmationTime {
                        height: conf_time.confirmation_height,
                        time: conf_time.confirmation_time,
                    }),
                    _ => None,
                };
                let net_value = self
                    .graph
                    .index
                    .net_value(&canonical_tx.tx_node.tx, key_id..=key_id);

                if net_value == 0 {
                    return None;
                }
                Some(api::Transaction {
                    inner: RustOpaque::new(canonical_tx.tx_node.tx.clone()),
                    confirmation_time,
                    net_value,
                })
            })
            .collect()
    }

    pub fn sync_txs(&self, txids: Vec<bitcoin::Txid>) -> SyncRequest {
        let mut sync = SyncRequest::new(self.chain.tip(), self.graph.graph().clone());
        sync.add_txids(txids);
        sync
    }

    pub fn start_sync(&self, key_id: KeyId) -> SyncRequest {
        // We want to sync all spks for now!
        let interesting_spks = self
            .graph
            .index
            .revealed_spks(key_id..=key_id)
            .map(|(_, _, spk)| spk.to_owned())
            .collect::<Vec<_>>();

        let mut sync = SyncRequest::new(self.chain.tip(), self.graph.graph().clone());
        sync.add_spks(interesting_spks);
        sync
    }

    pub fn finish_sync(&mut self, update: chain_sync::Update) -> Result<bool> {
        self.db.lock().unwrap().execute(|tx| {
            let chain_list = self.chain_list_handle.api(&tx);
            let tx_graph_list = self.tx_graph_list_handle.api(&tx);
            let chain_changeset = self.chain.apply_update(update.chain)?;
            if !chain_changeset.is_empty() {
                chain_list.push(&bincode::serde::Compat(chain_changeset))?;
            }
            let graph_changeset = self.graph.apply_update(update.tx_graph);
            // See bug: https://github.com/bitcoindevkit/bdk/pull/1335 for why this disjunction is needed
            if !(graph_changeset.is_empty() && graph_changeset.graph.anchors.is_empty()) {
                tx_graph_list.push(&bincode::serde::Compat(graph_changeset.graph))?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }
}
