use bdk_chain::bitcoin::BlockHash;
use std::{
    sync::{self, Arc},
    time::Duration,
};

use futures::{pin_mut, select_biased, FutureExt};
use tokio::sync::watch;

use crate::persist::Persisted;
use crate::settings::ElectrumEnabled;

use super::{
    chain_sync::{ConnectionResult, ElectrumConfig, Message},
    status_tracker::{ConnPhase, StatusTracker},
    tofu::{connection::Conn, trusted_certs::TrustedCertificates, verifier::TofuError},
};

/// Identifies the server a live connection is on. Carried by the connection state so failover
/// and reconnect decisions read it from the state — not back out of the status (output) layer.
#[derive(Clone, Debug)]
pub(super) struct ConnectedTo {
    pub(super) url: String,
    pub(super) on_backup: bool,
}

/// Outcome of [`HandlerState::establish`]: the connect attempt, raced ONLY against the config
/// (desired-target) watch. Benign `req_recv` messages are deliberately not watched there, so
/// they cannot cancel an in-flight connection / primary→backup failover — only a target change
/// (disable / url change) can. Backoff-on-failure is the caller's `Backoff` state, not here.
pub(super) enum Establish {
    /// A connection was established.
    Connected(Conn, ConnectedTo),
    /// The connect attempt produced no connection; the caller should back off and retry.
    Retry,
    /// The desired target (enabled/urls) changed; drop the attempt and re-evaluate.
    TargetChanged,
    /// The config channel closed → stop.
    Closed,
}

/// Whether a config change requires tearing down the live connection, given which server
/// we're on (`on_backup`) and the url we actually connected to. A connection is kept unless
/// the server it's on is no longer enabled, or *that* server's url changed — so changes to
/// the other slot (e.g. toggling the backup while on the primary) leave it untouched.
pub(super) fn reconnect_needed(
    on_backup: bool,
    connected_url: &str,
    config: &ElectrumConfig,
) -> bool {
    match config.enabled {
        // Nothing is enabled.
        ElectrumEnabled::None => true,
        // We're on the backup but only the primary is now enabled.
        ElectrumEnabled::PrimaryOnly if on_backup => true,
        // The server we're on is still enabled: reconnect only if *its* url changed.
        _ => {
            let current_url = if on_backup {
                &config.backup
            } else {
                &config.primary
            };
            connected_url != current_url
        }
    }
}

/// State needed for message handling and connection attempts.
pub(super) struct HandlerState {
    pub genesis_hash: BlockHash,
    pub status: StatusTracker,
    pub trusted_certificates: Persisted<TrustedCertificates>,
    pub db: Arc<sync::Mutex<rusqlite::Connection>>,
    pub started: bool,
    /// One-time preference for which server to try first
    prefer_backup: bool,
    config_rx: watch::Receiver<ElectrumConfig>,
}

impl HandlerState {
    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
    pub const RECONNECT_DELAY: Duration = Duration::from_secs(2);

    pub fn new(
        genesis_hash: BlockHash,
        config_rx: watch::Receiver<ElectrumConfig>,
        trusted_certificates: Persisted<TrustedCertificates>,
        db: Arc<sync::Mutex<rusqlite::Connection>>,
    ) -> Self {
        Self {
            genesis_hash,
            status: StatusTracker::new(config_rx.clone()),
            trusted_certificates,
            db,
            started: false,
            prefer_backup: false,
            config_rx,
        }
    }

    pub fn should_connect(&self) -> bool {
        self.started && self.config_rx.borrow().enabled != ElectrumEnabled::None
    }

    /// Re-emit status so subscribers observe updated urls after a config change.
    pub fn on_config_changed(&mut self) {
        self.status.refresh();
    }

    /// Drive one connect attempt — including the primary→backup failover inside
    /// `try_connect` — raced ONLY against the desired-target watch (`config_rx`).
    ///
    /// Crucially, this does NOT watch `req_recv`: a benign control message must never cancel
    /// an in-flight connection / failover. Only a target change (disable / url change) may,
    /// via `Establish::TargetChanged`. The caller acts on the result after the borrow ends.
    pub(super) async fn establish(
        &mut self,
        config_rx: &mut watch::Receiver<ElectrumConfig>,
    ) -> Establish {
        let work = async {
            match self.try_connect().await {
                Some((conn, info)) => Establish::Connected(conn, info),
                None => Establish::Retry,
            }
        }
        .fuse();
        pin_mut!(work);
        select_biased! {
            res = config_rx.changed().fuse() => match res {
                Ok(()) => Establish::TargetChanged,
                Err(_) => Establish::Closed,
            },
            done = work => done,
        }
    }

    /// Whether a config change requires reconnecting the connection on `connected_url`, given
    /// which server it's on (`on_backup`) — see [`reconnect_needed`].
    pub(super) fn reconnect_needed(&self, on_backup: bool, connected_url: &str) -> bool {
        reconnect_needed(on_backup, connected_url, &self.config_rx.borrow())
    }

    /// After the connection to the current server failed, prefer the OTHER server on the next
    /// attempt — so a server that connects but can't actually serve us is rotated away from.
    pub(super) fn prefer_other_server(&mut self, was_on_backup: bool) {
        self.prefer_backup = !was_on_backup;
    }

    /// Try to establish a new connection.
    /// Returns Some((connection, server)) if successful, None if all servers fail.
    pub(super) async fn try_connect(&mut self) -> Option<(Conn, ConnectedTo)> {
        let prefer_backup = std::mem::take(&mut self.prefer_backup);
        // Copy the config out so we don't hold the watch borrow across awaits.
        let config = self.config_rx.borrow().clone();

        let urls_to_try: Vec<(bool, String)> = match config.enabled {
            ElectrumEnabled::All if prefer_backup => {
                vec![(true, config.backup), (false, config.primary)]
            }
            ElectrumEnabled::All => {
                vec![(false, config.primary), (true, config.backup)]
            }
            ElectrumEnabled::PrimaryOnly => vec![(false, config.primary)],
            ElectrumEnabled::None => return None,
        };

        for (is_backup, url) in urls_to_try {
            self.status.set_phase(ConnPhase::Connecting {
                on_backup: is_backup,
            });
            tracing::info!("Connecting to {}.", url);

            match Conn::new(
                self.genesis_hash,
                &url,
                Self::CONNECT_TIMEOUT,
                &mut self.trusted_certificates,
            )
            .await
            {
                Ok(conn) => {
                    self.status.set_phase(ConnPhase::Connected {
                        on_backup: is_backup,
                    });
                    tracing::info!("Connection established with {}.", url);
                    return Some((
                        conn,
                        ConnectedTo {
                            url,
                            on_backup: is_backup,
                        },
                    ));
                }
                Err(err) => {
                    // Stay in the Connecting phase across the failover to the next server; a
                    // total failure settles to Disconnected in `backoff`.
                    tracing::error!(err = err.to_string(), url, "failed to connect",);
                }
            }
        }

        tracing::error!(
            reconnecting_in_secs = Self::RECONNECT_DELAY.as_secs_f32(),
            "Failed to connect to all Electrum servers"
        );
        None
    }

    /// Handle a single message.
    /// Returns true if the connection loop should be broken (to trigger reconnection).
    pub async fn handle_msg(&mut self, msg: Message) -> bool {
        match msg {
            Message::ChangeUrlReq(req) => {
                let (request, response) = req.into_tuple();
                tracing::info!(
                    msg = "ChangeUrlReq",
                    url = request.url,
                    is_backup = request.is_backup,
                );

                // Probe-only: validate the candidate server here, but the api owns the
                // config — on success it persists and writes the watch, which triggers a
                // reconnect via the config-change path. So nothing to reconnect here.
                match Conn::new(
                    self.genesis_hash,
                    &request.url,
                    Self::CONNECT_TIMEOUT,
                    &mut self.trusted_certificates,
                )
                .await
                {
                    Ok(_conn) => {
                        let _ = response.send(Ok(ConnectionResult::Success));
                    }
                    Err(TofuError::NotTrusted(cert)) => {
                        tracing::info!(
                            "Certificate not trusted for {}: {}",
                            request.url,
                            cert.fingerprint
                        );
                        let _ = response.send(Ok(ConnectionResult::CertificatePromptNeeded(cert)));
                    }
                    Err(TofuError::Other(e)) => {
                        tracing::error!("Failed to connect to {}: {}", request.url, e);
                        let _ = response.send(Ok(ConnectionResult::Failed(e.to_string())));
                    }
                }
                false
            }
            Message::SetStatusSink(new_sink) => {
                tracing::info!(msg = "SetStatusSink");
                self.status.set_sink(new_sink);
                false
            }
            Message::StartClient => {
                self.started = true;
                false
            }
            Message::Reconnect => {
                tracing::info!(msg = "Reconnect - forcing reconnection");
                true
            }
            Message::TrustCertificate {
                server_url,
                certificate_der,
            } => {
                tracing::info!(msg = "TrustCertificate", server_url = server_url);
                let cert = certificate_der.into();

                let hostname = match server_url.split_once("://") {
                    Some((_, addr)) => addr
                        .split_once(':')
                        .map(|(host, _)| host)
                        .unwrap_or(addr)
                        .to_string(),
                    None => server_url
                        .split_once(':')
                        .map(|(host, _)| host)
                        .unwrap_or(&server_url)
                        .to_string(),
                };

                tracing::info!("Storing certificate for hostname: {}", hostname);

                let mut db_guard = self.db.lock().unwrap();
                if let Err(e) =
                    self.trusted_certificates
                        .mutate2(&mut *db_guard, |trusted_certs, update| {
                            trusted_certs.add_certificate(cert, hostname, update);
                            Ok(())
                        })
                {
                    tracing::error!("Failed to trust certificate: {e:#}");
                }
                false
            }
            Message::ConnectTo { use_backup } => {
                tracing::info!(msg = "ConnectTo", use_backup);
                if use_backup && self.config_rx.borrow().enabled == ElectrumEnabled::PrimaryOnly {
                    tracing::warn!("Cannot connect to backup server when only primary is enabled");
                    return false;
                }
                self.prefer_backup = use_backup;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::chain_sync::ElectrumConfig;
    use bdk_chain::bitcoin::{consensus, constants::genesis_block, params::Params, Network};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::watch;

    /// A started handler plus the config `watch` sender (kept alive so the receiver passed to
    /// `establish` doesn't see a closed channel) and a receiver to drive `establish` with.
    fn make_handler(
        config: ElectrumConfig,
    ) -> (
        HandlerState,
        watch::Sender<ElectrumConfig>,
        watch::Receiver<ElectrumConfig>,
    ) {
        let genesis = genesis_block(Params::new(Network::Signet)).block_hash();
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let certs = Persisted::<TrustedCertificates>::new(&mut db, Network::Signet).unwrap();
        let (tx, rx) = watch::channel(config);
        let mut handler =
            HandlerState::new(genesis, rx.clone(), certs, Arc::new(sync::Mutex::new(db)));
        handler.started = true;
        (handler, tx, rx)
    }

    /// A url that always refuses fast (a port we bound then immediately freed).
    async fn refused_url() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("tcp://{addr}")
    }

    /// A server that accepts connections and never replies — connections to it hang until
    /// `Conn::new`'s timeout fires. Deterministically "broken but slow".
    async fn spawn_hanging_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((sock, _)) = listener.accept().await {
                held.push(sock); // keep the socket open; never respond
            }
        });
        format!("tcp://{addr}")
    }

    /// A minimal fake electrum server that answers the network check with the signet genesis
    /// header, so `Conn::new` succeeds against it. Deterministically "working".
    async fn spawn_fake_signet_server() -> String {
        let header_hex =
            consensus::encode::serialize_hex(&genesis_block(Params::new(Network::Signet)).header);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let header_hex = header_hex.clone();
                tokio::spawn(async move {
                    // Read the one request line.
                    let mut buf = Vec::new();
                    let mut b = [0u8; 1];
                    loop {
                        match sock.read(&mut b).await {
                            Ok(0) | Err(_) => return,
                            Ok(_) => {}
                        }
                        if b[0] == b'\n' {
                            break;
                        }
                        buf.push(b[0]);
                    }
                    // Echo the request id back with the genesis header as the result.
                    let req = String::from_utf8_lossy(&buf);
                    let id = req
                        .split("\"id\":")
                        .nth(1)
                        .and_then(|t| {
                            t.trim_start()
                                .split(|c: char| !c.is_ascii_digit())
                                .find(|s| !s.is_empty())
                        })
                        .unwrap_or("0");
                    let resp = format!(
                        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":\"{header_hex}\"}}\n"
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.flush().await;
                    // Hold the connection open briefly so the client can read the reply.
                    tokio::time::sleep(Duration::from_millis(500)).await;
                });
            }
        });
        format!("tcp://{addr}")
    }

    fn cfg(enabled: ElectrumEnabled, primary: &str, backup: &str) -> ElectrumConfig {
        ElectrumConfig {
            enabled,
            primary: primary.into(),
            backup: backup.into(),
        }
    }

    /// The reported bug: toggling/changing the *other* slot must not reconnect the live
    /// connection; only a change to the server we're on (or disabling it) should.
    #[test]
    fn reconnect_needed_only_when_current_server_is_affected() {
        use ElectrumEnabled::{All, None, PrimaryOnly};
        let (p, b) = ("tcp://p:1", "tcp://b:1");

        // On the PRIMARY: changes to the backup slot must NOT reconnect.
        assert!(!reconnect_needed(false, p, &cfg(All, p, b)));
        assert!(!reconnect_needed(false, p, &cfg(PrimaryOnly, p, b))); // disable backup
        assert!(!reconnect_needed(false, p, &cfg(All, p, "tcp://b:2"))); // backup url changed
                                                                         // On the PRIMARY: changes that affect it DO reconnect.
        assert!(reconnect_needed(false, p, &cfg(None, p, b))); // all off
        assert!(reconnect_needed(false, p, &cfg(All, "tcp://p:2", b))); // primary url changed

        // On the BACKUP (only reachable with All): symmetric.
        assert!(!reconnect_needed(true, b, &cfg(All, p, b)));
        assert!(!reconnect_needed(true, b, &cfg(All, "tcp://p:2", b))); // primary url changed → backup unaffected
        assert!(reconnect_needed(true, b, &cfg(PrimaryOnly, p, b))); // backup disabled
        assert!(reconnect_needed(true, b, &cfg(All, p, "tcp://b:2"))); // backup url changed
        assert!(reconnect_needed(true, b, &cfg(None, p, b))); // all off
    }

    /// The lazy/disabled invariant has a single home: `should_connect` (which `ConnLoop`'s
    /// idle gate and `connect` guard both delegate to). We connect only when started AND at
    /// least one server is enabled.
    #[test]
    fn should_connect_requires_started_and_enabled() {
        let (mut handler, tx, _rx) =
            make_handler(cfg(ElectrumEnabled::All, "tcp://p:1", "tcp://b:1"));
        assert!(handler.should_connect(), "started + enabled");

        tx.send_modify(|c| c.enabled = ElectrumEnabled::None);
        assert!(!handler.should_connect(), "disabled → never connect (lazy)");

        tx.send_modify(|c| c.enabled = ElectrumEnabled::PrimaryOnly);
        handler.started = false;
        assert!(
            !handler.should_connect(),
            "not started → never connect (lazy)"
        );
    }

    /// musdom's bug: with a broken primary and a working backup (`enabled = All`), establish
    /// must complete the primary→backup failover. `establish` does not watch `req_recv`, so a
    /// benign control message cannot abort it — only the result of the failover (here, the
    /// working backup) decides the outcome.
    #[tokio::test]
    async fn establish_fails_over_to_backup_when_primary_broken() {
        let primary = refused_url().await; // broken
        let backup = spawn_fake_signet_server().await; // working
        let (mut handler, _tx, mut config_rx) = make_handler(ElectrumConfig {
            enabled: ElectrumEnabled::All,
            primary,
            backup,
        });

        assert!(
            matches!(
                handler.establish(&mut config_rx).await,
                Establish::Connected(..)
            ),
            "establish should fail over to the working backup when the primary is broken"
        );
    }

    /// The complement: a change to the desired target (here, disabling) during an in-flight
    /// connect DOES interrupt it — so the fix doesn't over-correct into "nothing interrupts".
    #[tokio::test]
    async fn target_change_interrupts_in_flight_connect() {
        let primary = spawn_hanging_server().await; // slow-broken: keeps the connect in flight
        let backup = spawn_hanging_server().await; // also slow, so establish can't finish on its own
        let (mut handler, tx, mut config_rx) = make_handler(ElectrumConfig {
            enabled: ElectrumEnabled::All,
            primary,
            backup,
        });

        // Disable mid-connect, well before either server's connect timeout.
        tokio::spawn({
            let tx = tx.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                tx.send_modify(|c| c.enabled = ElectrumEnabled::None);
            }
        });

        assert!(
            matches!(
                handler.establish(&mut config_rx).await,
                Establish::TargetChanged
            ),
            "a target change (disable) should interrupt an in-flight connect"
        );
        drop(tx);
    }

    /// adam's bug: a server that connects (passes the probe) but fails its session must be
    /// rotated away from — the next attempt should prefer the OTHER server, not retry the
    /// failed one (which keeps passing the probe).
    #[tokio::test]
    async fn after_session_failure_rotates_to_the_other_server() {
        let primary = spawn_fake_signet_server().await; // passes the probe; would be tried first
        let backup = spawn_fake_signet_server().await; // also works
        let (mut handler, _tx, _rx) = make_handler(ElectrumConfig {
            enabled: ElectrumEnabled::All,
            primary,
            backup: backup.clone(),
        });

        // We were connected to the primary (on_backup = false) and its session failed.
        handler.prefer_other_server(false);

        // Despite the primary still being connectable, the next attempt rotates to the backup.
        let (_conn, info) = handler.try_connect().await.expect("should connect");
        assert_eq!(
            info.url, backup,
            "after a primary session failure the next connect should rotate to the backup"
        );
    }
}
