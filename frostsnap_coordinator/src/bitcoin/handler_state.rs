use bdk_chain::bitcoin::BlockHash;
use std::{
    sync::{self, Arc},
    time::Duration,
};

use crate::persist::Persisted;
use crate::settings::ElectrumEnabled;

use super::{
    chain_sync::{ChainStatusState, ConnectionResult, Message},
    status_tracker::StatusTracker,
    tofu::{
        connection::{Conn, ElectrumUrls},
        trusted_certs::TrustedCertificates,
        verifier::TofuError,
    },
};

/// State needed for message handling and connection attempts.
pub(super) struct HandlerState {
    pub genesis_hash: BlockHash,
    pub enabled: ElectrumEnabled,
    pub status: StatusTracker,
    pub urls: ElectrumUrls,
    pub trusted_certificates: Persisted<TrustedCertificates>,
    pub db: Arc<sync::Mutex<rusqlite::Connection>>,
    pub started: bool,
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
        Self {
            genesis_hash,
            enabled: ElectrumEnabled::default(),
            status: StatusTracker::new(&url),
            urls: ElectrumUrls { url, backup_url },
            trusted_certificates,
            db,
            started: false,
        }
    }

    pub fn should_connect(&self) -> bool {
        self.started && self.enabled != ElectrumEnabled::None
    }

    pub fn set_disconnected(&mut self) {
        self.status
            .update(&self.urls.url, ChainStatusState::Disconnected);
    }

    /// Get a connection by connecting fresh.
    /// Returns Some(connection) if successful, None if all servers fail.
    pub async fn get_connection(&mut self) -> Option<Conn> {
        self.try_connect().await.map(|(conn, _url)| conn)
    }

    /// Try to establish a new connection.
    /// Returns Some((connection, url)) if successful, None if all servers fail.
    pub async fn try_connect(&mut self) -> Option<(Conn, String)> {
        let urls_to_try: Vec<&str> = match self.enabled {
            ElectrumEnabled::All => vec![&self.urls.url, &self.urls.backup_url],
            ElectrumEnabled::PrimaryOnly => vec![&self.urls.url],
            ElectrumEnabled::None => return None,
        };
        for url in urls_to_try {
            self.status.update(url, ChainStatusState::Connecting);
            tracing::info!("Connecting to {}.", url);

            match Conn::new(
                self.genesis_hash,
                url,
                Self::CONNECT_TIMEOUT,
                &mut self.trusted_certificates,
            )
            .await
            {
                Ok(conn) => {
                    self.status.update(url, ChainStatusState::Connected);
                    tracing::info!("Connection established with {}.", url);
                    return Some((conn, url.to_string()));
                }
                Err(err) => {
                    self.status.update(url, ChainStatusState::Disconnected);
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
                        let currently_on_backup =
                            self.status.current().electrum_url == self.urls.backup_url;
                        if request.is_backup {
                            self.urls.backup_url = request.url.clone();
                        } else {
                            self.urls.url = request.url.clone();
                        }
                        let _ = response.send(Ok(ConnectionResult::Success));
                        // Reconnect if we changed the server we're currently connected to
                        !request.is_backup || currently_on_backup
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
                self.urls.url = primary;
                self.urls.backup_url = backup;
                true
            }
            Message::SetEnabled(new_enabled) => {
                let old_enabled = self.enabled;
                self.enabled = new_enabled;
                tracing::info!(
                    msg = "SetEnabled",
                    old = ?old_enabled,
                    new = ?new_enabled,
                );
                if new_enabled == ElectrumEnabled::None {
                    self.status
                        .update(&self.urls.url.clone(), ChainStatusState::Idle);
                }
                false
            }
        }
    }
}
