use bdk_chain::bitcoin::BlockHash;
use std::{
    sync::{self, Arc},
    time::Duration,
};

use crate::persist::Persisted;
use crate::settings::ElectrumEnabled;

use super::{
    chain_sync::{ChainStatus, ChainStatusState, ConnectionResult, Message},
    status_tracker::StatusTracker,
    tofu::{connection::Conn, trusted_certs::TrustedCertificates, verifier::TofuError},
};

/// State needed for message handling and connection attempts.
pub(super) struct HandlerState {
    pub genesis_hash: BlockHash,
    pub status: StatusTracker,
    pub trusted_certificates: Persisted<TrustedCertificates>,
    pub db: Arc<sync::Mutex<rusqlite::Connection>>,
    pub started: bool,
    /// One-time preference for which server to try first
    prefer_backup: bool,
}

impl HandlerState {
    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
    pub const RECONNECT_DELAY: Duration = Duration::from_secs(2);

    pub fn new(
        genesis_hash: BlockHash,
        url: String,
        backup_url: String,
        trusted_certificates: Persisted<TrustedCertificates>,
        db: Arc<sync::Mutex<rusqlite::Connection>>,
    ) -> Self {
        let initial_status = ChainStatus {
            primary_url: url,
            backup_url,
            on_backup: false,
            state: ChainStatusState::Idle,
            enabled: ElectrumEnabled::default(),
        };
        Self {
            genesis_hash,
            status: StatusTracker::new(initial_status),
            trusted_certificates,
            db,
            started: false,
            prefer_backup: false,
        }
    }

    pub fn should_connect(&self) -> bool {
        self.started && self.status.enabled() != ElectrumEnabled::None
    }

    pub fn set_disconnected(&mut self) {
        self.status.set_state(ChainStatusState::Disconnected);
    }

    /// Get a connection by connecting fresh.
    /// Returns Some(connection) if successful, None if all servers fail.
    pub async fn get_connection(&mut self) -> Option<Conn> {
        self.try_connect().await.map(|(conn, _url)| conn)
    }

    /// Try to establish a new connection.
    /// Returns Some((connection, url)) if successful, None if all servers fail.
    pub async fn try_connect(&mut self) -> Option<(Conn, String)> {
        let prefer_backup = std::mem::take(&mut self.prefer_backup);
        let primary = self.status.primary_url().to_string();
        let backup = self.status.backup_url().to_string();

        let urls_to_try: Vec<(bool, String)> = match self.status.enabled() {
            ElectrumEnabled::All if prefer_backup => {
                vec![(true, backup), (false, primary)]
            }
            ElectrumEnabled::All => {
                vec![(false, primary), (true, backup)]
            }
            ElectrumEnabled::PrimaryOnly => vec![(false, primary)],
            ElectrumEnabled::None => return None,
        };

        for (is_backup, url) in urls_to_try {
            self.status
                .set_state_and_server(ChainStatusState::Connecting, is_backup);
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
                    self.status.set_state(ChainStatusState::Connected);
                    tracing::info!("Connection established with {}.", url);
                    return Some((conn, url));
                }
                Err(err) => {
                    self.status.set_state(ChainStatusState::Disconnected);
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

                let reconnect = match Conn::new(
                    self.genesis_hash,
                    &request.url,
                    Self::CONNECT_TIMEOUT,
                    &mut self.trusted_certificates,
                )
                .await
                {
                    Ok(_conn) => {
                        let primary = if request.is_backup {
                            self.status.primary_url().to_string()
                        } else {
                            request.url.clone()
                        };
                        let backup = if request.is_backup {
                            request.url.clone()
                        } else {
                            self.status.backup_url().to_string()
                        };
                        self.status.set_urls(primary, backup);
                        let _ = response.send(Ok(ConnectionResult::Success));
                        // Reconnect if we changed the server we're currently connected to
                        request.is_backup == self.status.on_backup()
                    }
                    Err(err) => {
                        match err {
                            TofuError::NotTrusted(cert) => {
                                tracing::info!(
                                    "Certificate not trusted for {}: {}",
                                    request.url,
                                    cert.fingerprint
                                );
                                let _ = response
                                    .send(Ok(ConnectionResult::CertificatePromptNeeded(cert)));
                            }
                            TofuError::Other(e) => {
                                tracing::error!("Failed to connect to {}: {}", request.url, e);
                                let _ = response.send(Ok(ConnectionResult::Failed(e.to_string())));
                            }
                        }
                        false
                    }
                };
                reconnect
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
                    tracing::error!("Failed to trust certificate: {:?}", e);
                }
                false
            }
            Message::SetUrls { primary, backup } => {
                tracing::info!(msg = "SetUrls", primary = %primary, backup = %backup);
                self.status.set_urls(primary, backup);
                true
            }
            Message::SetEnabled(new_enabled) => {
                let old_enabled = self.status.enabled();
                let on_backup = self.status.on_backup();
                self.status.set_enabled(new_enabled);
                tracing::info!(
                    msg = "SetEnabled",
                    old = ?old_enabled,
                    new = ?new_enabled,
                );

                // Break loop if we disabled the server we're currently on
                let should_reconnect = match new_enabled {
                    ElectrumEnabled::None => true,
                    ElectrumEnabled::PrimaryOnly if on_backup => true,
                    _ => false,
                };

                if new_enabled == ElectrumEnabled::None {
                    self.status.set_state(ChainStatusState::Idle);
                }
                should_reconnect
            }
            Message::ConnectTo { use_backup } => {
                tracing::info!(msg = "ConnectTo", use_backup);
                if use_backup && self.status.enabled() == ElectrumEnabled::PrimaryOnly {
                    tracing::warn!("Cannot connect to backup server when only primary is enabled");
                    return false;
                }
                self.prefer_backup = use_backup;
                true
            }
        }
    }
}
