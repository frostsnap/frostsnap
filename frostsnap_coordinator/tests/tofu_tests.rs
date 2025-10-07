use frostsnap_coordinator::bitcoin::tofu::trusted_certs::TrustedCertificates;
use frostsnap_coordinator::persist::Persist;
use rusqlite::Connection;
use rustls_pki_types::ServerName;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

#[tokio::test]
async fn test_electrum_frostsn_app_ssl_connection() {
    // electrum.frostsn.app uses a self-signed certificate, so it requires TOFU
    // This test verifies that the pre-trusted certificate is properly loaded

    // Create a temporary database
    let temp_file = NamedTempFile::new().unwrap();
    let mut conn = Connection::open(temp_file.path()).unwrap();

    // Migrate and load TrustedCertificates - this should add the pre-trusted cert
    TrustedCertificates::migrate(&mut conn).unwrap();
    let trusted_certs =
        TrustedCertificates::load(&mut conn, bdk_chain::bitcoin::Network::Bitcoin).unwrap();

    // Verify that electrum.frostsn.app is pre-trusted
    assert!(
        trusted_certs
            .get_certificate_for_server("electrum.frostsn.app")
            .is_some(),
        "electrum.frostsn.app should be pre-trusted"
    );
}

#[tokio::test]
async fn test_ssl_connection_with_letsencrypt_ca() {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let addr = "blockstream.info:700";
    let stream = match TcpStream::connect(addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping test - could not connect to {}: {:?}", addr, e);
            return;
        }
    };

    let connector = TlsConnector::from(Arc::new(config));
    let dnsname = ServerName::try_from("blockstream.info").unwrap();

    let _tls_stream = connector
        .connect(dnsname, stream)
        .await
        .unwrap_or_else(|e| {
            panic!("TLS handshake failed for blockstream.info: {:?}", e);
        });
}

#[tokio::test]
async fn test_emzy_electrum_server() {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let addr = "electrum.emzy.de:50002";
    let stream = match TcpStream::connect(addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping test - could not connect to {}: {:?}", addr, e);
            return;
        }
    };

    let connector = TlsConnector::from(Arc::new(config));
    let dnsname = ServerName::try_from("electrum.emzy.de").unwrap();

    match connector.connect(dnsname, stream).await {
        Ok(_) => {
            println!(
                "Successfully connected to {} (note: would need certificate in store)",
                addr
            );
        }
        Err(e) => {
            println!(
                "Expected failure - Emzy's self-signed cert not in our store: {:?}",
                e
            );
        }
    }
}
