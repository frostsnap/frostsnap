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

use super::trusted_certs::TrustedCertificates;
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
                let host = socket_addr
                    .clone()
                    .split_once(":")
                    .map(|(host, _)| host.to_string())
                    .unwrap_or(socket_addr.clone());

                let stream = connect_with_tofu(&socket_addr, &host, trusted_certificates).await?;
                let (mut rh, mut wh) = tokio::io::split(stream);
                check_conn(&mut rh, &mut wh, genesis_hash)
                    .await
                    .map_err(TofuError::Other)
                    .inspect_err(|e| tracing::error!("Network check failed: {:?}", e))?;
                Ok(Conn::Ssl((rh, wh)))
            } else {
                let sock = happy_eyeballs_connect(&socket_addr).await.map_err(|e| {
                    tracing::error!("TCP connection failed to {}: {}", socket_addr, e);
                    TofuError::Other(e.into())
                })?;
                let (mut rh, mut wh) = tokio::io::split(sock);
                check_conn(&mut rh, &mut wh, genesis_hash)
                    .await
                    .map_err(TofuError::Other)
                    .inspect_err(|e| tracing::error!("Network check failed: {:?}", e))?;
                Ok(Conn::Tcp((rh, wh)))
            }
        }
        .fuse();
        pin_mut!(connect_fut);

        let timeout_fut = tokio::time::sleep(timeout).fuse();
        pin_mut!(timeout_fut);

        select! {
            conn_res = connect_fut => conn_res,
            _ = timeout_fut => {
                tracing::error!("Connection to {} timed out after {:?}", url, timeout);
                Err(TofuError::Other(anyhow!("Timed out")))
            },
        }
    }
}

/// Attempt to connect with TOFU support
async fn connect_with_tofu(
    socket_addr: &str,
    host: &str,
    trusted_certificates: &mut Persisted<TrustedCertificates>,
) -> Result<TlsStream<TcpStream>, TofuError> {
    // Create combined certificate store with PKI roots and TOFU certs
    let root_store = trusted_certificates.create_combined_cert_store();
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

    let dnsname = ServerName::try_from(host.to_owned())
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
