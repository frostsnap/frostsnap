//! BIP157 node driver: turns filter events into wallet updates.

use super::{WatchSet, MATCH_LOOKAHEAD};
use crate::bitcoin::{
    backend::{ChainStatus, ChainStatusDetail, ChainStatusState, FilterStatus},
    wallet::{CoordSuperWallet, KeychainId},
};
use crate::Sink;
use anyhow::{anyhow, Context, Result};
use bdk_chain::{
    bitcoin::{self},
    spk_client::FullScanResponse,
    BlockId, CheckPoint, ConfirmationBlockTime,
};
use bip157::{
    chain::{BlockHeaderChanges, IndexedHeader},
    ChainState, Event, GossipMonitorRequestBuilder, HashCheckpoint, Info, Receiver, Requester,
    TrustedPeer, Warning,
};
use futures::{channel::mpsc, StreamExt};
use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{self, Arc},
};
use tracing::{event, Level};

/// Blocks below the wallet's tip to restart syncing from, so an offline reorg is still noticed.
const RESEED_DEPTH: u32 = 10;

/// Default peer count a filter node waits on before trusting a filter.
const DEFAULT_REQUIRED_PEERS: u8 = 2;

/// Worker threads for the node's runtime. They only carry the node and its peer connections:
/// the event loop runs on the handler's own thread and wallet writes on tokio's blocking pool.
const NODE_WORKER_THREADS: usize = 2;

/// Share of the tip block's average fee rate offered for a confirmation target of 1, 2 and 3 or
/// more blocks. The average is what the next block paid; a quarter of it can wait.
const FEE_SHARE_BY_TARGET: [f64; 3] = [1.0, 0.5, 0.25];

/// Pick the starting `ChainState` given the wallet's tip and birthday.
fn seed_state(tip: &CheckPoint, birthday: HashCheckpoint) -> ChainState {
    if tip.height() == 0 {
        return ChainState::Checkpoint(birthday);
    }
    let resume = tip
        .floor_below(RESEED_DEPTH)
        .unwrap_or_else(|| tip.iter().last().expect("a checkpoint always has a base"));
    ChainState::Checkpoint(HashCheckpoint::new(resume.height(), resume.hash()))
}

/// Whether the first sync can skip filters: a chain never synced before, with no wallet that
/// could have been paid in it, has nothing in past blocks to find. Filters resume at the tip.
fn can_sync_headers_only(tip: &CheckPoint, watch: &WatchSet) -> bool {
    tip.height() == 0 && watch.is_empty()
}

/// `average` scaled by the target's share, never below `floor`, the lowest fee rate peers relay.
fn fee_rate_for_target(
    average: bitcoin::FeeRate,
    target: usize,
    floor: Option<bitcoin::FeeRate>,
) -> bitcoin::FeeRate {
    let share = FEE_SHARE_BY_TARGET[target.clamp(1, FEE_SHARE_BY_TARGET.len()) - 1];
    let rate = bitcoin::FeeRate::from_sat_per_kwu((average.to_sat_per_kwu() as f64 * share) as u64);
    floor.map_or(rate, |floor| rate.max(floor))
}

/// Fold a header change into the chain being tracked; `ForkAdded` is deliberately ignored.
fn apply_header_changes(tip: CheckPoint, changes: &BlockHeaderChanges) -> CheckPoint {
    fn block_id(header: &IndexedHeader) -> BlockId {
        BlockId {
            height: header.height,
            hash: header.header.block_hash(),
        }
    }
    match changes {
        BlockHeaderChanges::Connected(header) => tip.insert(block_id(header)),
        BlockHeaderChanges::Reorganized { accepted, .. } => {
            let mut accepted = accepted.iter().collect::<Vec<_>>();
            accepted.sort_by_key(|header| header.height);
            accepted
                .into_iter()
                .fold(tip, |tip, header| tip.insert(block_id(header)))
        }
        BlockHeaderChanges::ForkAdded(_) => tip,
    }
}

/// Configuration for the filter node.
#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub network: bitcoin::Network,
    /// Peers required to agree on a filter; the crate clamps this to 1..=15.
    pub required_peers: u8,
    /// bip157 hands out each trusted peer once, so with `whitelist_only` the transaction gossip
    /// listener needs one beyond the `required_peers`.
    pub trusted_peers: Vec<TrustedPeer>,
    /// Connect only to `trusted_peers`, skipping DNS seeds and gossip.
    pub whitelist_only: bool,
    /// Starting point for a wallet with no history.
    pub birthday: HashCheckpoint,
}

impl FilterConfig {
    pub fn new(network: bitcoin::Network) -> Self {
        Self {
            network,
            required_peers: DEFAULT_REQUIRED_PEERS,
            trusted_peers: Vec::new(),
            whitelist_only: false,
            birthday: default_birthday(network),
        }
    }
}

/// Starting checkpoint for a wallet with no history; mainnet floors at taproot activation.
fn default_birthday(network: bitcoin::Network) -> HashCheckpoint {
    match network {
        bitcoin::Network::Bitcoin => HashCheckpoint::taproot_activation(),
        _ => HashCheckpoint::from_genesis(bitcoin::params::Params::new(network)),
    }
}

/// The node's status for the app. The run loop sets the fields from node events, then calls
/// `emit`.
struct FilterStatusTracker {
    state: ChainStatusState,
    peers: u32,
    /// Height the node resumed from; progress counts from here.
    start_height: u32,
    chain_height: u32,
    filters_height: u32,
    sink: Option<Box<dyn Sink<ChainStatus>>>,
    last_emitted: Option<ChainStatus>,
}

impl FilterStatusTracker {
    fn new(start_height: u32) -> Self {
        Self {
            state: ChainStatusState::Idle,
            peers: 0,
            start_height,
            chain_height: start_height,
            filters_height: start_height,
            sink: None,
            last_emitted: None,
        }
    }

    /// Fraction of the filters from `start_height` to the tip checked, rounded down to a percent
    /// so a download does not emit a status per filter.
    fn progress(&self) -> f32 {
        let total = self.chain_height.saturating_sub(self.start_height);
        if total == 0 {
            return 0.0;
        }
        let checked = self
            .filters_height
            .saturating_sub(self.start_height)
            .min(total);
        (100.0 * checked as f32 / total as f32).floor() / 100.0
    }

    /// Send the status to the sink if it changed since the last one sent.
    fn emit(&mut self) {
        let Some(sink) = &self.sink else {
            return;
        };
        let status = ChainStatus {
            state: self.state,
            detail: ChainStatusDetail::CompactFilters(FilterStatus {
                peers: self.peers,
                progress: self.progress(),
            }),
        };
        if self.last_emitted.as_ref() != Some(&status) {
            sink.send(status.clone());
            self.last_emitted = Some(status);
        }
    }
}

enum Message {
    RefreshWatchSet,
    SetStatusSink(Box<dyn Sink<ChainStatus>>),
}

/// Cheap, cloneable handle the rest of the app holds.
#[derive(Clone)]
pub struct FilterClient {
    req_sender: mpsc::UnboundedSender<Message>,
    requester: Arc<sync::Mutex<Option<Requester>>>,
    runtime: tokio::runtime::Handle,
}

impl FilterClient {
    fn requester(&self) -> Result<Requester> {
        self.requester
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("the compact filter node is not running"))
    }

    pub fn monitor_keychain(&self, _keychain: KeychainId, _next_index: u32) {
        let _ = self.req_sender.unbounded_send(Message::RefreshWatchSet);
    }

    /// Returns once the transaction has been sent to a peer.
    pub fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<bitcoin::Txid> {
        let txid = transaction.compute_txid();
        let requester = self.requester()?;
        self.runtime
            .block_on(async move { requester.submit_package(transaction).await })
            .map_err(|err| anyhow!("broadcasting {txid}: {err:?}"))?;
        event!(Level::INFO, %txid, "broadcast sent");
        Ok(txid)
    }

    /// The tip block's average fee rate, scaled down for less urgent targets.
    pub fn estimate_fee(
        &self,
        target_blocks: &[usize],
    ) -> Result<BTreeMap<usize, bitcoin::FeeRate>> {
        if target_blocks.is_empty() {
            return Ok(BTreeMap::new());
        }
        let requester = self.requester()?;
        self.runtime.block_on(async move {
            let floor = requester.broadcast_min_feerate().await.ok();
            let tip = requester
                .chain_tip()
                .await
                .map_err(|err| anyhow!("asking for the chain tip: {err:?}"))?;
            let average = requester
                .average_fee_rate(tip.hash)
                .await
                .map_err(|err| anyhow!("fetching the tip block's fee rate: {err:?}"))?;
            Ok(target_blocks
                .iter()
                .map(|&target| (target, fee_rate_for_target(average, target, floor)))
                .collect())
        })
    }

    pub fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        let _ = self.req_sender.unbounded_send(Message::SetStatusSink(sink));
    }

    /// No-op: the node keeps a rotating set of peers, and a rescan belongs behind its own action.
    pub fn reconnect(&self) {}
}

/// Owns the node and event loop; run on a dedicated thread.
pub struct FilterHandler {
    config: FilterConfig,
    req_recv: mpsc::UnboundedReceiver<Message>,
    requester: Arc<sync::Mutex<Option<Requester>>>,
    runtime: Arc<tokio::runtime::Runtime>,
}

/// Build a `FilterClient` and its `FilterHandler`.
pub fn new(config: FilterConfig) -> Result<(FilterClient, FilterHandler)> {
    let (req_sender, req_recv) = mpsc::unbounded();
    let requester = Arc::new(sync::Mutex::new(None));
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(NODE_WORKER_THREADS)
            .enable_all()
            .build()
            .context("building the tokio runtime for the filter node")?,
    );
    Ok((
        FilterClient {
            req_sender,
            requester: requester.clone(),
            runtime: runtime.handle().clone(),
        },
        FilterHandler {
            config,
            req_recv,
            requester,
            runtime,
        },
    ))
}

impl FilterHandler {
    /// Run the event loop until the node stops, applying updates to `super_wallet`.
    pub fn run<SW, F>(self, super_wallet: SW, update_action: F) -> Result<()>
    where
        SW: Deref<Target = sync::Mutex<CoordSuperWallet>> + Clone + Send + 'static,
        F: FnMut(frostsnap_core::MasterAppkey, Vec<crate::bitcoin::wallet::Transaction>)
            + Send
            + 'static,
    {
        let (mut tip, mut watch) = snapshot(&super_wallet);
        let seed = seed_state(&tip, self.config.birthday);
        let headers_only = can_sync_headers_only(&tip, &watch);
        event!(
            Level::INFO,
            network = self.config.network.to_string(),
            watched_scripts = watch.scripts().count(),
            headers_only,
            "starting compact filter node"
        );
        let network = self.config.network;
        let rt = self.runtime.clone();
        let mut req_recv = self.req_recv;
        let start_height = match &seed {
            ChainState::Checkpoint(checkpoint) => checkpoint.height,
            ChainState::Snapshot(_) => 0,
        };
        let mut builder = bip157::Builder::new(self.config.network)
            .required_peers(self.config.required_peers)
            .chain_state(seed)
            .add_peers(self.config.trusted_peers.clone());
        if self.config.whitelist_only {
            builder = builder.whitelist_only();
        }
        if headers_only {
            builder = builder.headers_only_sync();
        }
        let (node, client) = builder.build();
        let bip157::Client {
            requester,
            mut event_rx,
            mut info_rx,
            mut warn_rx,
        } = client;
        *self.requester.lock().unwrap() = Some(requester.clone());
        let (update_sender, update_recv) =
            mpsc::unbounded::<FullScanResponse<KeychainId, ConfirmationBlockTime>>();
        let _writer = rt.spawn_blocking({
            let super_wallet = super_wallet.clone();
            move || {
                crate::bitcoin::chain_sync::ConnectionHandler::handle_wallet_updates(
                    super_wallet,
                    update_recv,
                    update_action,
                )
            }
        });
        let required_peers = u32::from(self.config.required_peers);
        let mut status = FilterStatusTracker::new(start_height);
        let node_handle = rt.spawn(async move { node.run().await });
        rt.block_on(async move {
            let mut gossip = None;
            subscribe_watch_set_to_gossip(&requester, &watch, &mut gossip).await;
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        let Some(event) = event else { break };
                        match event {
                            Event::ChainUpdate(changes) => {
                                match &changes {
                                    BlockHeaderChanges::Reorganized { accepted, reorganized } => {
                                        event!(
                                            Level::INFO,
                                            disconnected = reorganized.len(),
                                            connected = accepted.len(),
                                            "the chain reorganised"
                                        );
                                    }
                                    BlockHeaderChanges::ForkAdded(header) => {
                                        event!(
                                            Level::DEBUG,
                                            height = header.height,
                                            "saw a competing fork"
                                        );
                                    }
                                    BlockHeaderChanges::Connected(_) => {}
                                }
                                tip = apply_header_changes(tip, &changes);
                            }
                            Event::IndexedFilter(filter) => {
                                if !filter.contains_any(watch.scripts()) {
                                    continue;
                                }
                                let hash = filter.block_hash();
                                let height = filter.height();
                                event!(
                                    Level::DEBUG,
                                    height,
                                    "a filter matched; fetching its block",
                                );
                                match requester.get_block(hash).await {
                                    Ok(block) => {
                                        let found = watch.scan_block(&block.block, block.height);
                                        if found.is_none() {
                                            event!(
                                                Level::DEBUG,
                                                height,
                                                "the block held nothing of ours"
                                            );
                                        }
                                        if let Some(mut update) = found {
                                            tip = tip.insert(BlockId { height, hash });
                                            update.chain_update = Some(tip.clone());
                                            event!(
                                                Level::INFO,
                                                height,
                                                transactions = update.tx_update.txs.len(),
                                                "found wallet transactions in a block"
                                            );
                                            let _ = update_sender.unbounded_send(update);
                                        }
                                    }
                                    Err(err) => {
                                        event!(
                                            Level::WARN,
                                            height,
                                            ?err,
                                            "could not fetch a block the filter matched"
                                        );
                                    }
                                }
                            }
                            Event::FiltersSynced(sync) => {
                                event!(
                                    Level::INFO,
                                    height = sync.tip.height,
                                    "filters synced to tip"
                                );
                                status.chain_height = status.chain_height.max(sync.tip.height);
                                status.filters_height = sync.tip.height;
                                status.emit();
                                let _ = update_sender.unbounded_send(FullScanResponse {
                                    tx_update: Default::default(),
                                    last_active_indices: Default::default(),
                                    chain_update: Some(tip.clone()),
                                });
                            }
                        }
                    }
                    info = info_rx.recv() => {
                        match info {
                            Some(Info::NewChainHeight(height)) => status.chain_height = height,
                            Some(Info::FilterAdded(height)) => status.filters_height = height,
                            Some(Info::ConnectionsMet) => {
                                event!(Level::INFO, "connected to enough peers");
                                status.state = ChainStatusState::Connected;
                                status.peers = required_peers;
                            }
                            Some(_) => {}
                            None => break,
                        }
                        status.emit();
                    }
                    warning = warn_rx.recv() => {
                        match warning {
                            Some(Warning::NeedConnections { connected, required }) => {
                                event!(Level::INFO, connected, required, "waiting for peers");
                                status.state = ChainStatusState::Connecting;
                                status.peers = connected as u32;
                                status.emit();
                            }
                            Some(warning) => {
                                event!(Level::DEBUG, %warning, "compact filter node warning");
                            }
                            None => break,
                        }
                    }
                    tx = async {
                        match gossip.as_mut() {
                            Some(receiver) => receiver.recv().await,
                            // Nothing subscribed yet, so nothing to hear.
                            None => std::future::pending().await,
                        }
                    } => {
                        match tx {
                            Some(tx) => {
                                if let Some(update) = watch.scan_unconfirmed(&tx, unix_now()) {
                                    event!(
                                        Level::INFO,
                                        txid = %tx.compute_txid(),
                                        "heard an unconfirmed wallet transaction",
                                    );
                                    let _ = update_sender.unbounded_send(update);
                                }
                            }
                            None => {
                                event!(Level::WARN, "transaction gossip listener stopped");
                                gossip = None;
                            }
                        }
                    }
                    message = req_recv.next() => {
                        match message {
                            Some(Message::RefreshWatchSet) => {
                                let (_fresh_tip, fresh_watch) = snapshot(&super_wallet);
                                watch.merge(fresh_watch);
                                event!(
                                    Level::DEBUG,
                                    scripts = watch.scripts().count(),
                                    "refreshed the watched scripts",
                                );
                                subscribe_watch_set_to_gossip(&requester, &watch, &mut gossip)
                                    .await;
                            }
                            Some(Message::SetStatusSink(sink)) => {
                                // A new sink is sent the current status, changed or not.
                                status.sink = Some(sink);
                                status.last_emitted = None;
                                status.emit();
                            }
                            None => break,
                        }
                    }
                }
            }
        });
        event!(Level::INFO, network = %network, "compact filter node stopped");
        node_handle.abort();
        Ok(())
    }
}

/// Watch mempool gossip for `watch`'s scripts and coins, so a payment shows before it confirms.
/// The first subscription opens the listener and returns its receiver; later ones add to it.
async fn subscribe_watch_set_to_gossip(
    requester: &Requester,
    watch: &WatchSet,
    gossip: &mut Option<Receiver<bitcoin::Transaction>>,
) {
    if watch.is_empty() {
        return;
    }
    let request =
        GossipMonitorRequestBuilder::from_expected_output_scripts(watch.scripts().cloned())
            .add_outpoints(watch.owned().copied())
            .build();
    match requester.subscribe_to_gossip(request).await {
        Ok(Some(receiver)) => {
            event!(Level::INFO, "listening for transaction gossip");
            *gossip = Some(receiver);
        }
        Ok(None) => event!(Level::DEBUG, "widened the transaction gossip subscription"),
        Err(err) => event!(
            Level::WARN,
            ?err,
            "could not subscribe to transaction gossip"
        ),
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// Snapshot the wallet's tip and match set.
fn snapshot<SW>(super_wallet: &SW) -> (CheckPoint, WatchSet)
where
    SW: Deref<Target = sync::Mutex<CoordSuperWallet>>,
{
    let wallet = super_wallet.lock().expect("must lock");
    let tip = wallet.chain_tip();
    let watch = WatchSet::from_indexer(&wallet.tx_graph.index, MATCH_LOOKAHEAD);
    (tip, watch)
}

#[cfg(test)]
mod test {
    use super::*;
    use bdk_chain::bitcoin::{hashes::Hash, BlockHash};

    fn hash(byte: u8) -> BlockHash {
        BlockHash::from_byte_array([byte; 32])
    }

    fn chain(blocks: &[(u32, u8)]) -> CheckPoint {
        CheckPoint::from_block_ids(blocks.iter().map(|&(height, byte)| BlockId {
            height,
            hash: hash(byte),
        }))
        .expect("ascending")
    }

    fn header(height: u32, prev: u8) -> IndexedHeader {
        IndexedHeader {
            height,
            header: bdk_chain::bitcoin::block::Header {
                version: bdk_chain::bitcoin::block::Version::TWO,
                prev_blockhash: hash(prev),
                merkle_root: bdk_chain::bitcoin::TxMerkleNode::all_zeros(),
                time: 1_700_000_000,
                bits: bdk_chain::bitcoin::CompactTarget::from_consensus(0x1d00_ffff),
                nonce: 0,
            },
        }
    }

    #[test]
    fn seed_state_resumes_reseed_depth_below_tip() {
        let tip = chain(&[(0, 0), (100, 1), (105, 2), (110, 3)]);
        let ChainState::Checkpoint(seed) = seed_state(&tip, HashCheckpoint::new(0, hash(0))) else {
            panic!("a wallet with history resumes from a checkpoint")
        };
        assert_eq!(seed.height, 100);
        assert_eq!(seed.hash, hash(1));
    }

    #[test]
    fn seed_state_starts_unsynced_wallet_at_birthday() {
        let tip = chain(&[(0, 0)]);
        let birthday = HashCheckpoint::new(800_000, hash(9));
        let ChainState::Checkpoint(seed) = seed_state(&tip, birthday) else {
            panic!("expected a checkpoint")
        };
        assert_eq!(seed.height, 800_000);
    }

    #[test]
    fn headers_only_sync_requires_unsynced_chain_and_empty_watch_set() {
        let k = (
            frostsnap_core::MasterAppkey::derive_from_rootkey(
                frostsnap_core::schnorr_fun::fun::Point::random(&mut rand::thread_rng()),
            ),
            frostsnap_core::tweak::BitcoinAccountKeychain::external(),
        );
        let script = bitcoin::ScriptBuf::from_bytes(vec![0x51; 34]);
        let fresh = chain(&[(0, 0)]);
        let synced = chain(&[(0, 0), (100, 1)]);
        let nothing = WatchSet::default();
        let a_wallet = WatchSet::new([(script, (k, 0))], []);
        assert!(can_sync_headers_only(&fresh, &nothing));
        assert!(
            !can_sync_headers_only(&fresh, &a_wallet),
            "a wallet on a chain never synced may have been restored, so it gets a full scan"
        );
        assert!(
            !can_sync_headers_only(&synced, &nothing),
            "a synced chain resumes as before"
        );
    }

    #[test]
    fn status_progress_measured_from_resume_height() {
        let mut status = FilterStatusTracker::new(100);
        assert_eq!(status.progress(), 0.0, "no tip known yet");
        status.chain_height = 200;
        status.filters_height = 150;
        assert_eq!(status.progress(), 0.5);
        status.filters_height = 200;
        assert_eq!(status.progress(), 1.0);
    }

    /// The node reports every filter through a long download; forwarding a status for each
    /// would be a stream of identical ones.
    #[test]
    fn status_sends_only_changes() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Log(Arc<Mutex<Vec<ChainStatus>>>);
        impl crate::Sink<ChainStatus> for Log {
            fn send(&self, status: ChainStatus) {
                self.0.lock().unwrap().push(status);
            }
        }

        let log = Log(Default::default());
        let mut status = FilterStatusTracker::new(0);
        status.sink = Some(Box::new(log.clone()));
        status.emit();
        status.emit();
        assert_eq!(log.0.lock().unwrap().len(), 1);
        status.state = ChainStatusState::Connected;
        status.emit();
        assert_eq!(log.0.lock().unwrap().len(), 2);
    }

    #[test]
    fn fee_rate_scales_with_urgency_above_the_relay_floor() {
        let average = bitcoin::FeeRate::from_sat_per_vb_unchecked(40);
        let rate = |target, floor| fee_rate_for_target(average, target, floor).to_sat_per_vb_ceil();
        assert_eq!(rate(1, None), 40);
        assert_eq!(rate(2, None), 20);
        assert_eq!(rate(3, None), 10);
        assert_eq!(rate(10, None), 10, "beyond the table, the lowest share");
        let floor = Some(bitcoin::FeeRate::from_sat_per_vb_unchecked(15));
        assert_eq!(rate(3, floor), 15, "never below what peers relay");
    }

    /// bip157 does not promise the accepted headers in height order, and a lower block inserted
    /// after a higher one would displace it.
    #[test]
    fn apply_header_changes_replaces_reorged_blocks() {
        let tip = chain(&[(0, 0), (1, 1), (2, 2), (3, 3)]);
        let changes = BlockHeaderChanges::Reorganized {
            accepted: vec![header(3, 0xb2), header(2, 1)],
            reorganized: vec![header(2, 1), header(3, 2)],
        };
        let next = apply_header_changes(tip, &changes);
        assert_eq!(next.height(), 3);
        let heights = next.iter().map(|cp| cp.height()).collect::<Vec<_>>();
        assert_eq!(heights, vec![3, 2, 1, 0]);
        assert_ne!(next.hash(), hash(3), "the old tip is gone");
    }
}
