//! In-process Tor for Electrum connections, via arti (the Tor Project's Rust client).
//!
//! One `TorClient` per process, created on first use and bootstrapped on demand. `.onion` hosts
//! always go through it, and so does everything else when the user asks for Tor. Arti keeps its
//! state and directory cache (about 10 MB) under [`set_data_dir`], which must be called before
//! the first connection.

use anyhow::{anyhow, Result};
use arti_client::config::{BoolOrAuto, TorClientConfigBuilder};
use arti_client::{BootstrapBehavior, DataStream, IsolationToken, StreamPrefs, TorClient};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{ready, Context, Poll};
use tokio::sync::OnceCell;
use tor_rtcompat::PreferredRuntime;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static CLIENT: OnceCell<Arc<TorClient<PreferredRuntime>>> = OnceCell::const_new();

/// Where arti keeps its state and cache. Returns false if it was already set.
pub fn set_data_dir(dir: impl Into<PathBuf>) -> bool {
    DATA_DIR.set(dir.into()).is_ok()
}

pub fn is_onion(host: &str) -> bool {
    host.trim_end_matches('.')
        .rsplit('.')
        .next()
        .is_some_and(|tld| tld.eq_ignore_ascii_case("onion"))
}

/// The process-wide Tor client, created (not yet bootstrapped) on first call.
pub async fn client() -> Result<Arc<TorClient<PreferredRuntime>>> {
    CLIENT
        .get_or_try_init(|| async {
            let base = DATA_DIR
                .get()
                .ok_or_else(|| anyhow!("Tor data directory not set"))?;
            let mut builder =
                TorClientConfigBuilder::from_directories(base.join("state"), base.join("cache"));
            // The data directory is private to the app, but arti's permission audit also wants
            // every ancestor owned by us or root, which fails for app storage on Android
            // (owned by the `system` user).
            builder.storage().permissions().dangerously_trust_everyone();
            let config = builder.build().map_err(|e| anyhow!("tor config: {e}"))?;
            let runtime = PreferredRuntime::current()?;
            let client = TorClient::with_runtime(runtime)
                .config(config)
                .bootstrap_behavior(BootstrapBehavior::OnDemand)
                .create_unbootstrapped()
                .map_err(|e| anyhow!("tor client: {e}"))?;
            tracing::info!(dir = %base.display(), "Tor client created");
            Ok::<_, anyhow::Error>(client)
        })
        .await
        .cloned()
}

/// Open a stream to `host:port` through Tor. The first call also bootstraps, which can take
/// tens of seconds on a cold cache. Each connection gets its own circuit.
pub async fn connect(host: &str, port: u16) -> Result<AutoFlush<DataStream>> {
    let client = client().await?;
    let mut prefs = StreamPrefs::new();
    prefs.connect_to_onion_services(BoolOrAuto::Explicit(true));
    prefs.set_isolation(IsolationToken::new());
    tracing::info!(host, port, "Connecting via Tor");
    client
        .connect_with_prefs((host, port), &prefs)
        .await
        .map(AutoFlush::new)
        .map_err(|e| anyhow!("tor connect to {host}:{port}: {e}"))
}

/// Flushes after every write. Arti's `DataStream` holds written bytes until a cell is full or
/// `poll_flush` runs, and the Electrum client (ours and bdk's) writes a request and then waits
/// for the reply without flushing, so over Tor the request would never leave the buffer.
pub struct AutoFlush<S> {
    inner: S,
    dirty: bool,
}

impl<S> AutoFlush<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            dirty: false,
        }
    }
}

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> tokio::io::AsyncRead
    for AutoFlush<S>
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // A flush that `poll_write` couldn't finish is still holding bytes. The writer may not
        // be polled again until the next request, but the reader is polled for as long as we
        // await the reply, so it finishes the flush.
        if self.dirty {
            if let Poll::Ready(res) = Pin::new(&mut self.inner).poll_flush(cx) {
                res?;
                self.dirty = false;
            }
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for AutoFlush<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.dirty {
            ready!(Pin::new(&mut self.inner).poll_flush(cx))?;
            self.dirty = false;
        }
        let n = ready!(Pin::new(&mut self.inner).poll_write(cx, buf))?;
        self.dirty = true;
        // Usually completes at once; if not, the next read, write or flush finishes it.
        if let Poll::Ready(res) = Pin::new(&mut self.inner).poll_flush(cx) {
            res?;
            self.dirty = false;
        }
        Poll::Ready(Ok(n))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        ready!(Pin::new(&mut self.inner).poll_flush(cx))?;
        self.dirty = false;
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};

    #[test]
    fn onion_detection_ignores_case_and_trailing_dot() {
        assert!(is_onion("abc.onion"));
        assert!(is_onion("abc.ONION."));
        assert!(!is_onion("onion.example.com"));
        assert!(!is_onion("abconion"));
    }

    /// `BufWriter` holds writes until flushed, like arti's `DataStream`, and the 2-byte pipe
    /// takes only half the request before the flush stalls. Nothing writes again, so only the
    /// read side waiting for the reply can push out the rest.
    #[tokio::test]
    async fn request_reaches_the_server_without_an_explicit_flush() {
        let (ours, mut theirs) = tokio::io::duplex(2);
        let mut stream = AutoFlush::new(BufWriter::new(ours));
        stream.write_all(b"ping").await.unwrap();

        let server = async {
            let mut req = [0u8; 4];
            theirs.read_exact(&mut req).await.unwrap();
            theirs.write_all(b"ok").await.unwrap();
            req
        };
        let mut reply = [0u8; 2];
        let (req, read) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(server, stream.read_exact(&mut reply))
        })
        .await
        .expect("request stuck in the write buffer");
        read.unwrap();
        assert_eq!(&req, b"ping");
        assert_eq!(&reply, b"ok");
    }
}
