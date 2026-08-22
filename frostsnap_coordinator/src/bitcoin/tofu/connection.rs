use anyhow::anyhow;
use bdk_chain::bitcoin::BlockHash;
use futures::{pin_mut, select, FutureExt, StreamExt};
use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{lookup_host, TcpStream};
use tokio_rustls::{client::TlsStream, TlsConnector};

use super::trusted_certs::{TrustKey, TrustedCertificates};
use super::verifier::{TofuCertVerifier, TofuError};
use crate::persist::Persisted;

/// RFC 8305 Happy Eyeballs: try IPv6 first, start IPv4 after CONNECTION_ATTEMPT_DELAY if needed
const CONNECTION_ATTEMPT_DELAY: Duration = Duration::from_millis(250);

async fn happy_eyeballs_connect(
    addr: impl tokio::net::ToSocketAddrs,
) -> std::io::Result<TcpStream> {
    let addrs: Vec<SocketAddr> = lookup_host(addr).await?.collect();
    if addrs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no addresses found",
        ));
    }

    let (mut ipv6, mut ipv4): (Vec<&SocketAddr>, Vec<&SocketAddr>) =
        addrs.iter().partition(|a| a.is_ipv6());

    //  Shuffle each family for load balancing
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    ipv6.shuffle(&mut rng);
    ipv4.shuffle(&mut rng);

    // Interleave: IPv6, IPv4, IPv6, IPv4, ...
    let mut sorted = Vec::with_capacity(addrs.len());
    let mut i6 = ipv6.into_iter();
    let mut i4 = ipv4.into_iter();
    loop {
        match (i6.next(), i4.next()) {
            (Some(v6), Some(v4)) => {
                sorted.push(*v6);
                sorted.push(*v4);
            }
            (Some(v6), None) => sorted.push(*v6),
            (None, Some(v4)) => sorted.push(*v4),
            (None, None) => break,
        }
    }

    use futures::stream::FuturesUnordered;

    let num_addrs = sorted.len() as u32;
    let mut pending: FuturesUnordered<_> = sorted
        .into_iter()
        .enumerate()
        .map(|(i, addr)| async move {
            tokio::time::sleep(CONNECTION_ATTEMPT_DELAY * i as u32).await;
            TcpStream::connect(addr).await
        })
        .collect();

    // Last connection starts at (n-1)*250ms, give it 4s to complete
    let total_timeout =
        CONNECTION_ATTEMPT_DELAY * num_addrs.saturating_sub(1) + Duration::from_secs(4);
    let deadline = tokio::time::Instant::now() + total_timeout;

    let mut last_err = None;
    loop {
        tokio::select! {
            biased;
            result = pending.next() => {
                match result {
                    Some(Ok(stream)) => return Ok(stream),
                    Some(Err(e)) => last_err = Some(e),
                    None => break,
                }
            }
            _ = tokio::time::sleep_until(deadline) => break,
        }
    }

    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "all connection attempts failed",
        )
    }))
}

type SplitConn<T> = (tokio::io::ReadHalf<T>, tokio::io::WriteHalf<T>);

pub enum Conn {
    Tcp(SplitConn<tokio::net::TcpStream>),
    Ssl(SplitConn<TlsStream<tokio::net::TcpStream>>),
}

impl Conn {
    pub async fn new(
        genesis_hash: BlockHash,
        url: &str,
        timeout: Duration,
        trusted_certificates: &mut Persisted<TrustedCertificates>,
    ) -> Result<Self, TofuError> {
        let connect_fut = async {
            let (is_ssl, socket_addr) = match url.split_once("://") {
                Some(("ssl", socket_addr)) => (true, socket_addr.to_owned()),
                Some(("tcp", socket_addr)) => (false, socket_addr.to_owned()),
                Some((unknown_scheme, _)) => {
                    return Err(TofuError::Other(anyhow!(
                        "unknown url scheme '{unknown_scheme}'"
                    )));
                }
                None => (false, url.to_owned()),
            };
            tracing::info!(url, "Connecting");
            if is_ssl {
                let host = host_from_url(&socket_addr);
                let stream = connect_with_tofu(&socket_addr, &host, trusted_certificates).await?;
                let (mut rh, mut wh) = tokio::io::split(stream);
                check_conn(&mut rh, &mut wh, genesis_hash)
                    .await
                    .map_err(TofuError::Other)
                    .inspect_err(|e| tracing::error!(url, "Network check failed: {e}"))?;
                Ok(Conn::Ssl((rh, wh)))
            } else {
                let sock = happy_eyeballs_connect(&socket_addr).await.map_err(|e| {
                    tracing::error!(url, "TCP connection failed: {e}");
                    TofuError::Other(e.into())
                })?;
                let (mut rh, mut wh) = tokio::io::split(sock);
                check_conn(&mut rh, &mut wh, genesis_hash)
                    .await
                    .map_err(TofuError::Other)
                    .inspect_err(|e| tracing::error!(url, "Network check failed: {e}"))?;
                Ok(Conn::Tcp((rh, wh)))
            }
        }
        .fuse();
        pin_mut!(connect_fut);

        let timeout_fut = tokio::time::sleep(timeout).fuse();
        pin_mut!(timeout_fut);

        let result = select! {
            conn_res = connect_fut => conn_res,
            _ = timeout_fut => Err(TofuError::Other(anyhow!("timed out after {timeout:?}"))),
        };

        // Attribute every failure to its server so the url travels with the error itself
        // (not just the log line), regardless of which path produced it.
        result.map_err(|err| match err {
            TofuError::Other(e) => TofuError::Other(e.context(format!("connecting to {url}"))),
            not_trusted => not_trusted,
        })
    }
}

/// The host part of an electrum url (with or without a `scheme://` prefix): the name TLS is
/// verified against, handed back as the `TrustKey` the trust store is keyed by so that every
/// caller lands on the same key. An IPv6 literal is bracketed in a url but not in a server name,
/// so `[2001:db8::1]:50002` has to come back as `2001:db8::1`: splitting on the first colon would
/// give `[2001`.
pub fn host_from_url(url: &str) -> TrustKey {
    let authority = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = match authority.strip_prefix('[') {
        Some(after_bracket) => after_bracket
            .split_once(']')
            .map(|(host, _)| host)
            .unwrap_or(after_bracket),
        None => authority
            .split_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority),
    };
    TrustKey::new(host)
}

/// Attempt to connect with TOFU support
async fn connect_with_tofu(
    socket_addr: &str,
    host: &TrustKey,
    trusted_certificates: &mut Persisted<TrustedCertificates>,
) -> Result<TlsStream<TcpStream>, TofuError> {
    // webpki roots only. TOFU certs must never go in here: webpki treats every root as an
    // unconstrained CA regardless of its basicConstraints, so a TOFU'd leaf would let whoever
    // holds its key mint a cert for any other host. TOFU trust is per-host exact match in
    // `TofuCertVerifier`, which runs before this verifier is ever consulted.
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|e| TofuError::Other(anyhow!("Failed to create verifier: {:?}", e)))?;

    let tofu_verifier = Arc::new(TofuCertVerifier::new(
        base_verifier,
        trusted_certificates.clone(),
    ));
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(tofu_verifier.clone())
        .with_no_client_auth();

    let dnsname = ServerName::try_from(host.as_str().to_owned())
        .map_err(|e| TofuError::Other(anyhow!("Invalid DNS name: {}", e)))?;

    let sock = happy_eyeballs_connect(socket_addr).await.map_err(|e| {
        tracing::error!("TCP connection failed to {}: {}", socket_addr, e);
        TofuError::Other(anyhow!("TCP connection failed: {}", e))
    })?;

    let connector = TlsConnector::from(Arc::new(config));

    match connector.connect(dnsname.clone(), sock).await {
        Ok(stream) => Ok(stream),
        Err(e) => {
            // Check if there's a TOFU error stored for this connection
            if let Some(tofu_error) = tofu_verifier.take_tofu_error(host) {
                tracing::info!(
                    "TLS connection rejected due to TOFU verification: {:?}",
                    tofu_error
                );
                Err(tofu_error)
            } else {
                // No TOFU error stored, return the rustls error
                tracing::error!("TLS handshake failed for {}: {}", host, e);

                // The error from connector.connect() is std::io::Error
                // We need to check if it contains a rustls error
                let error_msg = if let Some(inner) = e.get_ref() {
                    // Try to get more specific error information
                    let inner_str = inner.to_string();
                    if inner_str.contains("UnsupportedCertVersion") {
                        format!("{}'s X.509 certificate version is too old", host)
                    } else if inner_str.contains("UnknownIssuer") {
                        format!("{}'s certificate issuer unknown", host)
                    } else if inner_str.contains("invalid peer certificate") {
                        format!("{}'s certificate invalid: {}", host, inner_str)
                    } else {
                        format!("TLS handshake failed: {}", e)
                    }
                } else {
                    format!("TLS handshake failed: {}", e)
                };

                Err(TofuError::Other(anyhow!(error_msg)))
            }
        }
    }
}

/// Check that the connection actually connects to an Electrum server and the server is on the right
/// network.
async fn check_conn<R, W>(rh: R, mut wh: W, genesis_hash: BlockHash) -> anyhow::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use bdk_electrum_streaming::electrum_streaming_client as client;
    use client::request;
    use client::RawNotificationOrResponse;
    use client::Request;
    use futures::io::BufReader;
    use tokio_util::compat::TokioAsyncReadCompatExt;

    let req_id = rand::random::<u32>();
    let req = client::RawRequest::from_request(req_id, request::Header { height: 0 });
    client::io::tokio_write(&mut wh, req).await?;

    let mut read_stream = client::io::ReadStreamer::new(BufReader::new(rh.compat()));
    let raw_incoming = read_stream
        .next()
        .await
        .ok_or(anyhow!("failed to get response from server"))??;

    let raw_resp = match raw_incoming {
        RawNotificationOrResponse::Notification(_) => {
            return Err(anyhow!("Received unexpected notification from server"))
        }
        RawNotificationOrResponse::Response(raw_resp) => raw_resp,
    };

    if raw_resp.id != req_id {
        return Err(anyhow!(
            "Response id {} does not match request id {}",
            raw_resp.id,
            req_id
        ));
    }

    let raw_val = raw_resp
        .result
        .map_err(|err| anyhow!("Server responded with error: {err}"))?;

    let resp: <request::Header as Request>::Response = client::serde_json::from_value(raw_val)?;

    if genesis_hash != resp.header.block_hash() {
        return Err(anyhow!("Electrum server is on a different network"));
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct TargetServerReq {
    pub url: String,
    pub is_backup: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_from_url_strips_scheme_port_and_ipv6_brackets() {
        assert_eq!(
            host_from_url("ssl://[2001:db8::1]:50002").as_str(),
            "2001:db8::1"
        );
        assert_eq!(host_from_url("[2001:db8::1]:50002").as_str(), "2001:db8::1");
        assert_eq!(
            host_from_url("ssl://electrum.frostsn.app:50002").as_str(),
            "electrum.frostsn.app"
        );
        assert_eq!(host_from_url("192.0.2.1:50002").as_str(), "192.0.2.1");
        assert_eq!(
            host_from_url("electrum.frostsn.app").as_str(),
            "electrum.frostsn.app"
        );
    }

    #[tokio::test]
    #[ignore] // requires network
    async fn test_real_signet_server_via_our_stack() {
        use crate::persist::Persisted;
        use bdk_chain::bitcoin::{constants::genesis_block, params::Params, Network};

        // Defaults to the shipped signet primary; override with SIGNET_URL to vet other servers.
        let url = std::env::var("SIGNET_URL")
            .unwrap_or_else(|_| "tcp://signet.musdomworks.com:50001".into());
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let mut certs = Persisted::<TrustedCertificates>::new(&mut db, Network::Signet).unwrap();
        let genesis = genesis_block(Params::new(Network::Signet)).block_hash();

        match Conn::new(genesis, &url, Duration::from_secs(10), &mut certs).await {
            Ok(_) => println!(
                "OK: {url} connected, PKI-valid cert (no TOFU prompt), signet genesis matched"
            ),
            Err(TofuError::NotTrusted(c)) => {
                panic!(
                    "{url}: cert not PKI-valid, would TOFU-prompt (fingerprint {})",
                    c.fingerprint
                )
            }
            Err(TofuError::Other(e)) => panic!("{url}: FAILED via our stack: {e:?}"),
        }
    }

    #[tokio::test]
    #[ignore] // requires network
    async fn test_happy_eyeballs_blockstream() {
        let start = std::time::Instant::now();
        let stream = happy_eyeballs_connect("electrum.blockstream.info:50002")
            .await
            .expect("should connect");
        let elapsed = start.elapsed();
        println!("Connected in {:?}", elapsed);
        drop(stream);
    }
}
