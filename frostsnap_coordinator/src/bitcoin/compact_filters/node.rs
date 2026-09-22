//! BIP157 node driver: turns filter events into wallet updates.

use super::{connect, WatchSet, MATCH_LOOKAHEAD};
use crate::bitcoin::{
    backend::{ChainBackend, ChainStatus, ChainStatusDetail, ChainStatusState, FilterStatus},
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
    ChainState, Event, HashCheckpoint, Info, Progress, Requester, TrustedPeer, Warning,
};
use futures::channel::mpsc;
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    ops::Deref,
    str::FromStr,
    sync::{self, Arc},
    time::Duration,
};
use tracing::{event, Level};

/// Blocks below the wallet's tip to restart syncing from, so an offline reorg is still noticed.
pub const RESEED_DEPTH: u32 = 10;

/// Pick the starting `ChainState` given the wallet's tip and optional birthday.
pub fn seed_state(tip: &CheckPoint, birthday: Option<HashCheckpoint>, depth: u32) -> ChainState {
    if tip.height() == 0 {
        if let Some(birthday) = birthday {
            return ChainState::Checkpoint(birthday);
        }
    }
    let resume = tip
        .floor_below(depth)
        .unwrap_or_else(|| tip.iter().last().expect("a checkpoint always has a base"));
    ChainState::Checkpoint(HashCheckpoint::new(resume.height(), resume.hash()))
}

/// Fold a header change into the chain being tracked; `ForkAdded` is deliberately ignored.
pub fn apply_header_changes(tip: CheckPoint, changes: &BlockHeaderChanges) -> CheckPoint {
    fn block_id(header: &IndexedHeader) -> BlockId {
        BlockId {
            height: header.height,
            hash: header.header.block_hash(),
        }
    }
    match changes {
        BlockHeaderChanges::Connected(header) => connect(tip, block_id(header)),
        BlockHeaderChanges::Reorganized { accepted, .. } => {
            let mut accepted = accepted.iter().collect::<Vec<_>>();
            accepted.sort_by_key(|header| header.height);
            accepted
                .into_iter()
                .fold(tip, |tip, header| connect(tip, block_id(header)))
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
    pub trusted_peers: Vec<TrustedPeer>,
    /// Connect only to `trusted_peers`, skipping DNS seeds and gossip.
    pub whitelist_only: bool,
    /// Starting point for a wallet with no history.
    pub birthday: Option<HashCheckpoint>,
}

impl FilterConfig {
    pub fn new(network: bitcoin::Network) -> Self {
        Self {
            network,
            required_peers: crate::settings::Settings::default().get_filter_required_peers(network),
            trusted_peers: Vec::new(),
            whitelist_only: false,
            birthday: None,
        }
    }

    /// Build from persisted settings, dropping unparseable peers with a warning.
    pub fn from_settings(settings: &crate::settings::Settings, network: bitcoin::Network) -> Self {
        let default_port = default_p2p_port(network);
        let trusted_peers = settings
            .get_filter_peers(network)
            .into_iter()
            .filter_map(|spec| match parse_peer(&spec, default_port) {
                Some(peer) => Some(peer),
                None => {
                    event!(Level::WARN, peer = spec, "ignoring unparseable filter peer");
                    None
                }
            })
            .collect::<Vec<_>>();
        let whitelist_only = settings.get_filter_whitelist_only(network);
        if whitelist_only && trusted_peers.is_empty() {
            event!(
                Level::ERROR,
                network = network.to_string(),
                "whitelist-only peering with no usable peers: the node will not connect"
            );
        }
        Self {
            network,
            required_peers: settings.get_filter_required_peers(network),
            trusted_peers,
            whitelist_only,
            birthday: Some(default_birthday(network)),
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

/// Default P2P port when the peer setting omits one.
fn default_p2p_port(network: bitcoin::Network) -> u16 {
    match network {
        bitcoin::Network::Bitcoin => 8333,
        bitcoin::Network::Signet => 38333,
        bitcoin::Network::Regtest => 18444,
        _ => 18333,
    }
}

/// Parse a `host:port`, bare IP, or bare hostname into a `TrustedPeer`.
fn parse_peer(spec: &str, default_port: u16) -> Option<TrustedPeer> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    if let Ok(socket) = SocketAddr::from_str(spec) {
        return Some(TrustedPeer::from_socket_addr(socket));
    }
    if let Ok(ip) = IpAddr::from_str(spec) {
        return Some(TrustedPeer::from((ip, Some(default_port))));
    }
    let (host, port) = match spec.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (spec, default_port),
    };
    is_plausible_hostname(host).then(|| TrustedPeer::from_hostname(host, port))
}

/// Cheap character-set check to reject text that was never a hostname.
fn is_plausible_hostname(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// Projects node events into the app's `ChainStatus`, deduping repeated emissions.
struct FilterStatusTracker {
    peers: u32,
    progress: f32,
    chain_height: u32,
    connected: bool,
    last_emitted: Option<ChainStatus>,
    sink: Box<dyn Sink<ChainStatus>>,
}

impl Default for FilterStatusTracker {
    fn default() -> Self {
        Self {
            peers: 0,
            progress: 0.0,
            chain_height: 0,
            connected: false,
            last_emitted: None,
            sink: Box::new(()),
        }
    }
}

impl FilterStatusTracker {
    fn set_sink(&mut self, sink: Box<dyn Sink<ChainStatus>>) {
        self.sink = sink;
        let status = self.project();
        self.last_emitted = Some(status.clone());
        self.sink.send(status);
    }

    fn handshake(&mut self) {
        self.peers = self.peers.saturating_add(1);
        self.emit();
    }

    fn connections_met(&mut self) {
        self.connected = true;
        self.emit();
    }

    fn needs_connections(&mut self) {
        self.connected = false;
        self.peers = 0;
        self.emit();
    }

    fn progress(&mut self, progress: Progress) {
        self.progress = progress.fraction_complete();
        self.chain_height = progress.chain_height();
        self.emit();
    }

    fn project(&self) -> ChainStatus {
        ChainStatus {
            state: match (self.connected, self.peers) {
                (true, _) => ChainStatusState::Connected,
                (false, 0) => ChainStatusState::Idle,
                (false, _) => ChainStatusState::Connecting,
            },
            detail: ChainStatusDetail::CompactFilters(FilterStatus {
                peers: self.peers,
                progress: self.progress,
                chain_height: self.chain_height,
            }),
        }
    }

    fn emit(&mut self) {
        let status = self.project();
        if self.last_emitted.as_ref() == Some(&status) {
            return;
        }
        self.last_emitted = Some(status.clone());
        self.sink.send(status);
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
    fn block_on<T>(&self, f: impl std::future::Future<Output = T>) -> T {
        self.runtime.block_on(f)
    }

    fn requester(&self) -> Result<Requester> {
        self.requester
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow!("the compact filter node is not running"))
    }
}

impl ChainBackend for FilterClient {
    fn monitor_keychain(&self, _keychain: KeychainId, _next_index: u32) {
        let _ = self.req_sender.unbounded_send(Message::RefreshWatchSet);
    }

    fn broadcast(&self, transaction: bitcoin::Transaction) -> Result<bitcoin::Txid> {
        let txid = transaction.compute_txid();
        let requester = self.requester()?;
        self.block_on(async move { requester.submit_package(transaction).await })
            .map(|_wtxid| txid)
            .map_err(|err| anyhow!("broadcasting {txid}: {err:?}"))
    }

    /// Average fee rate over blocks below the tip, floored at `broadcast_min_feerate`.
    fn estimate_fee(&self, target_blocks: &[usize]) -> Result<BTreeMap<usize, bitcoin::FeeRate>> {
        if target_blocks.is_empty() {
            return Ok(BTreeMap::new());
        }
        let requester = self.requester()?;
        let deepest = target_blocks.iter().copied().max().unwrap_or(1).max(1);
        self.block_on(async move {
            let floor = requester.broadcast_min_feerate().await.ok();
            let tip = requester
                .chain_tip()
                .await
                .map_err(|err| anyhow!("asking for the chain tip: {err:?}"))?;
            let mut samples = Vec::with_capacity(deepest);
            for depth in 0..deepest as u32 {
                let Some(height) = tip.height.checked_sub(depth) else {
                    break;
                };
                let header = match requester.get_header(height).await {
                    Ok(Some(header)) => header,
                    Ok(None) => break,
                    Err(err) => {
                        event!(Level::DEBUG, height, ?err, "no header for fee sampling");
                        break;
                    }
                };
                match requester.average_fee_rate(header.header.block_hash()).await {
                    Ok(rate) => samples.push(rate),
                    Err(err) => {
                        event!(Level::DEBUG, height, ?err, "no fee rate for block");
                    }
                }
            }
            if samples.is_empty() {
                return Ok(BTreeMap::new());
            }
            let mut estimates = BTreeMap::new();
            for target in target_blocks.iter().copied() {
                let window = target.clamp(1, samples.len());
                let total: u64 = samples[..window]
                    .iter()
                    .map(|rate| rate.to_sat_per_kwu())
                    .sum();
                let mean = bitcoin::FeeRate::from_sat_per_kwu(total / window as u64);
                estimates.insert(target, floor.map_or(mean, |floor| mean.max(floor)));
            }
            Ok(estimates)
        })
    }

    fn set_status_sink(&self, sink: Box<dyn Sink<ChainStatus>>) {
        let _ = self.req_sender.unbounded_send(Message::SetStatusSink(sink));
    }

    /// No-op: the node keeps a rotating set of peers, and a rescan belongs behind its own action.
    fn reconnect(&self) {}
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
            .worker_threads(2)
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
        let seed = seed_state(&tip, self.config.birthday, RESEED_DEPTH);
        event!(
            Level::INFO,
            network = self.config.network.to_string(),
            watched_scripts = watch.scripts().count(),
            "starting compact filter node"
        );
        let rt = self.runtime.clone();
        let mut req_recv = self.req_recv;
        let mut builder = bip157::Builder::new(self.config.network)
            .required_peers(self.config.required_peers)
            .chain_state(seed)
            .response_timeout(Duration::from_secs(10))
            .add_peers(self.config.trusted_peers.clone());
        if self.config.whitelist_only {
            builder = builder.whitelist_only();
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
        let mut status = FilterStatusTracker::default();
        let node_handle = rt.spawn(async move { node.run().await });
        rt.block_on(async move {
            loop {
                tokio::select! {
                    event = event_rx.recv() => {
                        let Some(event) = event else { break };
                        match event {
                            Event::ChainUpdate(changes) => {
                                tip = apply_header_changes(tip, &changes);
                            }
                            Event::IndexedFilter(filter) => {
                                if !filter.contains_any(watch.scripts()) {
                                    continue;
                                }
                                let hash = filter.block_hash();
                                let height = filter.height();
                                match requester.get_block(hash).await {
                                    Ok(block) => {
                                        if let Some(mut update) =
                                            watch.scan_block(&block.block, block.height)
                                        {
                                            tip = connect(tip, BlockId { height, hash });
                                            update.chain_update = Some(tip.clone());
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
                            Some(Info::Progress(progress)) => status.progress(progress),
                            Some(Info::ConnectionsMet) => status.connections_met(),
                            Some(Info::SuccessfulHandshake) => status.handshake(),
                            Some(Info::BlockReceived(_)) => {}
                            None => break,
                        }
                    }
                    warning = warn_rx.recv() => {
                        match warning {
                            Some(Warning::NeedConnections { .. }) => status.needs_connections(),
                            Some(warning) => {
                                event!(Level::DEBUG, %warning, "compact filter node warning");
                            }
                            None => break,
                        }
                    }
                    message = next_message(&mut req_recv) => {
                        match message {
                            Some(Message::RefreshWatchSet) => {
                                let (_fresh_tip, fresh_watch) = snapshot(&super_wallet);
                                watch = fresh_watch;
                            }
                            Some(Message::SetStatusSink(sink)) => {
                                status.set_sink(sink);
                            }
                            None => break,
                        }
                    }
                }
            }
        });
        node_handle.abort();
        Ok(())
    }
}

async fn next_message(rx: &mut mpsc::UnboundedReceiver<Message>) -> Option<Message> {
    use futures::StreamExt;
    rx.next().await
}

/// Snapshot the wallet's tip and match set.
fn snapshot<SW>(super_wallet: &SW) -> (CheckPoint, WatchSet)
where
    SW: Deref<Target = sync::Mutex<CoordSuperWallet>>,
{
    let wallet = super_wallet.lock().expect("must lock");
    let tip = wallet.chain_tip();
    let owned = wallet.owned_outpoints().collect::<Vec<_>>();
    let watch = WatchSet::from_indexer(&wallet.tx_graph.index, MATCH_LOOKAHEAD, owned);
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
    fn a_synced_wallet_resumes_just_below_its_tip() {
        let tip = chain(&[(0, 0), (100, 1), (105, 2), (110, 3)]);
        let ChainState::Checkpoint(seed) = seed_state(&tip, None, RESEED_DEPTH) else {
            panic!("a wallet with history resumes from a checkpoint")
        };
        assert_eq!(seed.height, 100);
        assert_eq!(seed.hash, hash(1));
    }

    #[test]
    fn a_missing_rewind_height_resumes_lower_never_higher() {
        let tip = chain(&[(0, 0), (90, 1), (110, 3)]);
        let ChainState::Checkpoint(seed) = seed_state(&tip, None, RESEED_DEPTH) else {
            panic!("expected a checkpoint")
        };
        assert!(seed.height <= 100, "never resumes above the rewind target");
        assert_eq!(seed.height, 90);
    }

    #[test]
    fn a_fresh_wallet_starts_at_its_birthday() {
        let tip = chain(&[(0, 0)]);
        let birthday = HashCheckpoint::new(800_000, hash(9));
        let ChainState::Checkpoint(seed) = seed_state(&tip, Some(birthday), RESEED_DEPTH) else {
            panic!("expected a checkpoint")
        };
        assert_eq!(seed.height, 800_000);
    }

    #[test]
    fn a_fresh_wallet_with_no_birthday_starts_from_genesis() {
        let tip = chain(&[(0, 0)]);
        let ChainState::Checkpoint(seed) = seed_state(&tip, None, RESEED_DEPTH) else {
            panic!("expected a checkpoint")
        };
        assert_eq!(seed.height, 0);
    }

    /// Live end-to-end check against signet peers; ignored by default because it takes minutes.
    #[test]
    #[ignore = "needs signet peers and several minutes"]
    fn syncs_a_signet_wallet_against_real_peers() {
        use crate::bitcoin::chain_sync::{ChainClient, ElectrumConfig};
        use crate::bitcoin::wallet::CoordSuperWallet;
        use crate::persist::Persisted;
        use crate::settings::ElectrumEnabled;
        use frostsnap_core::schnorr_fun::fun::Point;
        use frostsnap_core::MasterAppkey;
        use std::sync::Mutex;
        use std::time::Instant;

        const NETWORK: bitcoin::Network = bitcoin::Network::Signet;
        const BUDGET: Duration = Duration::from_secs(1800);

        let db = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
        let (idle_electrum, _electrum_handler) = {
            let trusted = {
                let mut conn = db.lock().unwrap();
                Persisted::new(&mut *conn, NETWORK).unwrap()
            };
            ChainClient::new(
                bitcoin::constants::genesis_block(bitcoin::params::Params::new(NETWORK))
                    .block_hash(),
                ElectrumConfig {
                    enabled: ElectrumEnabled::None,
                    primary: String::new(),
                    backup: String::new(),
                },
                trusted,
                db.clone(),
            )
        };
        let master_appkey =
            MasterAppkey::derive_from_rootkey(Point::random(&mut rand::thread_rng()));
        let mut wallet = CoordSuperWallet::load_or_init(db, NETWORK, idle_electrum).unwrap();
        wallet.list_addresses(master_appkey);
        let wallet = Arc::new(Mutex::new(wallet));
        let config = FilterConfig::new(NETWORK);
        let (client, handler) = new(config).expect("builds a runtime");
        let node = std::thread::spawn({
            let wallet = wallet.clone();
            move || handler.run(wallet, |_, _| {})
        });
        let started = Instant::now();
        let mut reached = 0;
        while started.elapsed() < BUDGET {
            std::thread::sleep(Duration::from_secs(5));
            reached = wallet.lock().unwrap().chain_tip().height();
            println!("[{:>4}s] wallet tip {reached}", started.elapsed().as_secs());
            if reached > 200_000 {
                break;
            }
        }
        drop(client);
        let _ = node.join();
        assert!(
            reached > 200_000,
            "the wallet only reached height {reached} in {}s — the node did not follow signet",
            started.elapsed().as_secs()
        );
    }

    #[test]
    fn peers_are_parsed_in_every_form_a_user_might_type() {
        for spec in [
            "10.0.0.1:8333",
            "10.0.0.1",
            "[2001:db8::1]:8333",
            "::1",
            "node.example:8333",
            "node.example",
            "  10.0.0.1:8333  ",
        ] {
            assert!(parse_peer(spec, 8333).is_some(), "should parse: {spec}");
        }
    }

    #[test]
    fn nonsense_peers_are_rejected_rather_than_guessed_at() {
        for spec in [
            "",
            "   ",
            ":8333",
            "node.example:not-a-port",
            "not a peer at all",
            "http://node.example",
        ] {
            assert!(parse_peer(spec, 8333).is_none(), "should not parse: {spec}");
        }
    }

    #[test]
    fn a_peer_without_a_port_takes_the_network_default() {
        let peer = parse_peer("10.0.0.1", 38333).expect("parses");
        assert_eq!(peer.port(), Some(38333));
    }

    #[test]
    fn each_network_has_its_own_p2p_port() {
        assert_eq!(default_p2p_port(bitcoin::Network::Bitcoin), 8333);
        assert_eq!(default_p2p_port(bitcoin::Network::Signet), 38333);
        assert_eq!(default_p2p_port(bitcoin::Network::Regtest), 18444);
        assert_eq!(default_p2p_port(bitcoin::Network::Testnet), 18333);
    }

    #[test]
    fn mainnet_starts_at_taproot_activation() {
        let birthday = default_birthday(bitcoin::Network::Bitcoin);
        assert_eq!(birthday.height, 709_631);
        let signet = default_birthday(bitcoin::Network::Signet);
        assert_eq!(signet.height, 0, "no taproot checkpoint outside mainnet");
    }

    #[test]
    fn settings_carry_through_to_the_node_configuration() {
        let mut settings = crate::settings::Settings::default();
        let mut mutations = vec![];
        settings.set_filter_peers(
            bitcoin::Network::Signet,
            vec!["10.0.0.1:38333".into(), "not a peer at all".into()],
            &mut mutations,
        );
        settings.set_filter_required_peers(bitcoin::Network::Signet, 4, &mut mutations);
        settings.set_filter_whitelist_only(bitcoin::Network::Signet, true, &mut mutations);
        let config = FilterConfig::from_settings(&settings, bitcoin::Network::Signet);
        assert_eq!(config.required_peers, 4);
        assert!(config.whitelist_only);
        assert_eq!(config.trusted_peers.len(), 1);
    }

    /// One peer is not the quorum the node was told to reach, so it must not read as connected.
    /// Reporting "connected" off a single handshake would tell the user their wallet is up to date
    /// while the node is still waiting to trust what it hears.
    #[test]
    fn a_single_handshake_is_connecting_not_connected() {
        let mut status = FilterStatusTracker::default();
        assert_eq!(status.project().state, ChainStatusState::Idle);

        status.handshake();
        assert_eq!(status.project().state, ChainStatusState::Connecting);

        status.connections_met();
        assert_eq!(status.project().state, ChainStatusState::Connected);
    }

    #[test]
    fn losing_peers_stops_claiming_a_count_we_no_longer_know() {
        let mut status = FilterStatusTracker::default();
        status.handshake();
        status.handshake();
        status.connections_met();

        status.needs_connections();
        let projected = status.project();
        assert_eq!(projected.state, ChainStatusState::Idle);
        match projected.detail {
            ChainStatusDetail::CompactFilters(filters) => assert_eq!(filters.peers, 0),
            other => panic!("expected compact filter detail, got {other:?}"),
        }
    }

    /// The node reports progress continuously through a long download; forwarding every one would
    /// be a stream of identical statuses.
    #[test]
    fn an_unchanged_status_is_not_re_emitted() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Log(Arc<Mutex<Vec<ChainStatus>>>);
        impl crate::Sink<ChainStatus> for Log {
            fn send(&self, status: ChainStatus) {
                self.0.lock().unwrap().push(status);
            }
        }

        let log = Log(Default::default());
        let mut status = FilterStatusTracker::default();
        status.set_sink(Box::new(log.clone()));
        assert_eq!(
            log.0.lock().unwrap().len(),
            1,
            "a new sink sees the current status"
        );

        status.connections_met();
        status.connections_met();
        assert_eq!(log.0.lock().unwrap().len(), 2, "only the change was sent");
    }

    #[test]
    fn a_connected_block_extends_the_chain() {
        let tip = chain(&[(0, 0), (1, 1)]);
        let next = apply_header_changes(tip, &BlockHeaderChanges::Connected(header(2, 1)));
        assert_eq!(next.height(), 2);
    }

    #[test]
    fn a_reorg_replaces_the_blocks_it_supersedes() {
        let tip = chain(&[(0, 0), (1, 1), (2, 2), (3, 3)]);
        let changes = BlockHeaderChanges::Reorganized {
            accepted: vec![header(2, 1), header(3, 0xb2)],
            reorganized: vec![header(2, 1), header(3, 2)],
        };
        let next = apply_header_changes(tip, &changes);
        assert_eq!(next.height(), 3);
        let heights = next.iter().map(|cp| cp.height()).collect::<Vec<_>>();
        assert_eq!(heights, vec![3, 2, 1, 0]);
        assert_ne!(next.hash(), hash(3), "the old tip is gone");
    }

    #[test]
    fn a_fork_that_was_not_selected_is_ignored() {
        let tip = chain(&[(0, 0), (1, 1)]);
        let before = tip.hash();
        let after = apply_header_changes(tip, &BlockHeaderChanges::ForkAdded(header(2, 9)));
        assert_eq!(after.hash(), before);
        assert_eq!(after.height(), 1);
    }
}
