use anyhow::Result;
use bdk_chain::{bitcoin, rusqlite_impl::migrate_schema};
use rusqlite::params;
use rustls::RootCertStore;
use rustls_pki_types::CertificateDer;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::verifier::CertificateExt;

use crate::persist::Persist;

#[derive(Clone, Debug)]
pub struct TrustedCertificate {
    pub certificate: CertificateDer<'static>,
    pub added_at: i64, // Unix timestamp
}

#[derive(Debug, Clone)]
pub struct TrustedCertificates {
    /// The network this certificate store is for
    network: bitcoin::Network,
    /// Maps server_url -> trusted certificate
    certificates: HashMap<String, TrustedCertificate>,
}

#[derive(Debug, Clone)]
pub enum CertificateMutation {
    Add {
        network: bitcoin::Network,
        certificate: CertificateDer<'static>,
        server_url: String,
    },
    Remove {
        network: bitcoin::Network,
        server_url: String,
    },
}

impl TrustedCertificates {
    pub fn apply_mutation(&mut self, mutation: &CertificateMutation) -> bool {
        match mutation {
            CertificateMutation::Add {
                network: _, // Already filtered by network during load
                certificate,
                server_url,
            } => {
                let added_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                self.certificates.insert(
                    server_url.clone(),
                    TrustedCertificate {
                        certificate: certificate.clone(),
                        added_at,
                    },
                );
                true
            }
            CertificateMutation::Remove {
                network: _,
                server_url,
            } => self.certificates.remove(server_url).is_some(),
        }
    }

    fn mutate(&mut self, mutation: CertificateMutation, mutations: &mut Vec<CertificateMutation>) {
        if self.apply_mutation(&mutation) {
            tracing::debug!("Certificate store mutation: {:?}", mutation);
            mutations.push(mutation);
        }
    }

    pub fn add_certificate(
        &mut self,
        cert: CertificateDer<'static>,
        server_url: String,
        mutations: &mut Vec<CertificateMutation>,
    ) {
        // HashMap will automatically replace any existing entry
        self.mutate(
            CertificateMutation::Add {
                network: self.network,
                certificate: cert,
                server_url,
            },
            mutations,
        );
    }

    pub fn remove_certificate(
        &mut self,
        server_url: String,
        mutations: &mut Vec<CertificateMutation>,
    ) {
        self.mutate(
            CertificateMutation::Remove {
                network: self.network,
                server_url,
            },
            mutations,
        );
    }

    pub fn create_combined_cert_store(&self) -> RootCertStore {
        // Start with standard PKI certificates from webpki-roots
        let mut store = RootCertStore::empty();
        store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        // Add user-trusted certificates
        for (server_url, trusted_cert) in &self.certificates {
            if let Err(e) = store.add(trusted_cert.certificate.clone()) {
                tracing::warn!(
                    "Failed to add trusted certificate for {}: {:?}",
                    server_url,
                    e
                );
            }
        }

        store
    }

    /// Find a certificate by its SHA256 fingerprint (raw hex, no colons)
    pub fn find_certificate_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Option<(&str, &TrustedCertificate)> {
        self.certificates
            .iter()
            .find(|(_, cert)| cert.certificate.sha256_fingerprint() == fingerprint)
            .map(|(url, cert)| (url.as_str(), cert))
    }

    pub fn get_all_certificates(&self) -> Vec<(String, TrustedCertificate)> {
        self.certificates
            .iter()
            .map(|(url, cert)| (url.clone(), cert.clone()))
            .collect()
    }

    pub fn get_certificate_for_server(&self, server_url: &str) -> Option<&CertificateDer<'_>> {
        self.certificates.get(server_url).map(|tc| &tc.certificate)
    }

    #[cfg(test)]
    pub fn new_for_test(network: bitcoin::Network) -> Self {
        Self {
            network,
            certificates: HashMap::new(),
        }
    }
}

// Pre-trusted certificate for electrum.frostsn.app
const ELECTRUM_FROSTSN_APP_CERT: &[u8] = include_bytes!("certs/electrum.frostsn.app.der");
const ELECTRUM_FROSTSN_APP_URL: &str = "electrum.frostsn.app";

const SCHEMA_NAME: &str = "frostsnap_electrum_tofu";
const MIGRATIONS: &[&str] = &[
    // Version 0 - initial schema
    "CREATE TABLE IF NOT EXISTS fs_trusted_certificates (
        network TEXT NOT NULL,
        server_url TEXT NOT NULL,
        certificate BLOB NOT NULL,
        added_at INTEGER NOT NULL,
        PRIMARY KEY (network, server_url)
    )",
];

impl Persist<rusqlite::Connection> for TrustedCertificates {
    type Update = Vec<CertificateMutation>;
    type LoadParams = bitcoin::Network;

    fn migrate(conn: &mut rusqlite::Connection) -> Result<()> {
        let db_tx = conn.transaction()?;
        migrate_schema(&db_tx, SCHEMA_NAME, MIGRATIONS)?;

        // Check if we need to add pre-trusted certificate (only on first run)
        let needs_init: i64 = db_tx.query_row(
            "SELECT COUNT(*) FROM fs_trusted_certificates WHERE network = 'bitcoin' AND server_url = ?1",
            params![ELECTRUM_FROSTSN_APP_URL],
            |row| row.get(0),
        ).unwrap_or(0);

        if needs_init == 0 {
            tracing::info!("First time initialization - adding pre-trusted certificate for electrum.frostsn.app");
            let electrum_cert = CertificateDer::from(ELECTRUM_FROSTSN_APP_CERT.to_vec());
            let added_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

            // Add the certificate for bitcoin mainnet since electrum.frostsn.app:50002 is used for mainnet
            db_tx.execute(
                "INSERT INTO fs_trusted_certificates (network, server_url, certificate, added_at) VALUES (?1, ?2, ?3, ?4)",
                params!["bitcoin", ELECTRUM_FROSTSN_APP_URL, electrum_cert.as_ref(), added_at],
            )?;
        }

        db_tx.commit()?;
        Ok(())
    }

    fn load(conn: &mut rusqlite::Connection, network: Self::LoadParams) -> Result<Self>
    where
        Self: Sized,
    {
        let mut stmt = conn.prepare(
            "SELECT server_url, certificate, added_at FROM fs_trusted_certificates WHERE network = ?1",
        )?;

        let cert_iter = stmt.query_map([network.to_string()], |row| {
            let server_url: String = row.get(0)?;
            let cert_blob: Vec<u8> = row.get(1)?;
            let added_at: i64 = row.get(2)?;

            Ok((
                server_url,
                TrustedCertificate {
                    certificate: CertificateDer::from(cert_blob),
                    added_at,
                },
            ))
        })?;

        let mut trusted_certs = TrustedCertificates {
            network,
            certificates: HashMap::new(),
        };

        for cert_result in cert_iter {
            match cert_result {
                Ok((server_url, cert)) => {
                    tracing::debug!(
                        "Loaded trusted certificate for {} on network {} added at {}",
                        server_url,
                        network,
                        cert.added_at
                    );
                    // Directly insert to preserve the original added_at timestamp
                    trusted_certs.certificates.insert(server_url, cert);
                }
                Err(e) => {
                    tracing::warn!("Failed to load trusted certificate: {:?}", e);
                }
            }
        }

        tracing::info!(
            "Loaded {} trusted certificates for {} network",
            trusted_certs.certificates.len(),
            network
        );

        Ok(trusted_certs)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        for mutation in update {
            match mutation {
                CertificateMutation::Add {
                    network,
                    certificate,
                    server_url,
                } => {
                    let added_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

                    tracing::info!(
                        "Adding trusted certificate for {} on {} network",
                        server_url,
                        network
                    );

                    conn.execute(
                        "INSERT OR REPLACE INTO fs_trusted_certificates
                         (network, server_url, certificate, added_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            network.to_string(),
                            server_url,
                            certificate.as_ref(),
                            added_at
                        ],
                    )?;
                }
                CertificateMutation::Remove {
                    network,
                    server_url,
                } => {
                    tracing::info!(
                        "Removing trusted certificate for {} on {} network",
                        server_url,
                        network
                    );

                    let rows_affected = conn.execute(
                        "DELETE FROM fs_trusted_certificates WHERE network = ?1 AND server_url = ?2",
                        params![network.to_string(), server_url],
                    )?;

                    if rows_affected == 0 {
                        tracing::warn!(
                            "Attempted to remove certificate for {} but it was not found",
                            server_url
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn create_test_cert() -> CertificateDer<'static> {
        // This is a dummy certificate for testing
        CertificateDer::from(vec![0x30, 0x82, 0x01, 0x0a, 0x02, 0x82, 0x01, 0x01])
    }

    #[test]
    fn test_persist_and_load() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let mut conn = rusqlite::Connection::open(temp_file.path())?;

        // Migrate and load store - it will have pre-trusted certificates
        TrustedCertificates::migrate(&mut conn)?;
        let mut store = TrustedCertificates::load(&mut conn, bitcoin::Network::Bitcoin)?;
        let initial_count = store.certificates.len();

        // Add a certificate
        let cert = create_test_cert();
        let mut mutations = Vec::new();
        store.add_certificate(
            cert.clone(),
            "test.example.com:443".to_string(),
            &mut mutations,
        );

        // Persist the mutation
        TrustedCertificates::persist_update(&mut conn, mutations)?;

        // Load again and verify
        let loaded_store = TrustedCertificates::load(&mut conn, bitcoin::Network::Bitcoin)?;
        assert_eq!(loaded_store.certificates.len(), initial_count + 1);
        assert!(loaded_store
            .certificates
            .contains_key("test.example.com:443"));

        Ok(())
    }

    #[test]
    fn test_remove_certificate() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let mut conn = rusqlite::Connection::open(temp_file.path())?;

        // Migrate and load, then add a certificate
        TrustedCertificates::migrate(&mut conn)?;
        let mut store = TrustedCertificates::load(&mut conn, bitcoin::Network::Bitcoin)?;
        let initial_count = store.certificates.len();
        let cert = create_test_cert();
        let mut mutations = Vec::new();
        store.add_certificate(
            cert.clone(),
            "test.example.com:443".to_string(),
            &mut mutations,
        );
        TrustedCertificates::persist_update(&mut conn, mutations)?;

        // Remove by server URL
        let mut mutations = Vec::new();
        store.remove_certificate("test.example.com:443".to_string(), &mut mutations);
        TrustedCertificates::persist_update(&mut conn, mutations)?;

        // Verify it's removed
        let loaded_store = TrustedCertificates::load(&mut conn, bitcoin::Network::Bitcoin)?;
        assert_eq!(loaded_store.certificates.len(), initial_count);

        Ok(())
    }

    #[test]
    fn test_pre_trusted_cert_only_inserted_once() -> Result<()> {
        let temp_file = NamedTempFile::new()?;

        // First initialization - should insert pre-trusted cert
        {
            let mut conn = rusqlite::Connection::open(temp_file.path())?;
            TrustedCertificates::migrate(&mut conn)?;
            let store1 = TrustedCertificates::load(&mut conn, bitcoin::Network::Bitcoin)?;
            assert_eq!(store1.certificates.len(), 1);
            assert!(store1.certificates.contains_key(ELECTRUM_FROSTSN_APP_URL));
        }

        // Second initialization - should NOT insert again
        {
            let mut conn = rusqlite::Connection::open(temp_file.path())?;
            TrustedCertificates::migrate(&mut conn)?;
            let store2 = TrustedCertificates::load(&mut conn, bitcoin::Network::Bitcoin)?;
            assert_eq!(store2.certificates.len(), 1);
            assert!(store2.certificates.contains_key(ELECTRUM_FROSTSN_APP_URL));

            // Verify there's still only one row in the database
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM fs_trusted_certificates", [], |row| {
                    row.get(0)
                })?;
            assert_eq!(count, 1);
        }

        Ok(())
    }

    #[test]
    fn test_apply_mutation() {
        let mut store = TrustedCertificates::new_for_test(bitcoin::Network::Bitcoin);
        let initial_count = store.certificates.len();
        let cert = create_test_cert();

        // Test add mutation
        let add_mutation = CertificateMutation::Add {
            network: bitcoin::Network::Bitcoin,
            certificate: cert.clone(),
            server_url: "test.example.com:443".to_string(),
        };

        assert!(store.apply_mutation(&add_mutation));
        assert_eq!(store.certificates.len(), initial_count + 1);
        assert!(store.certificates.contains_key("test.example.com:443"));

        // Test remove mutation
        let remove_mutation = CertificateMutation::Remove {
            network: bitcoin::Network::Bitcoin,
            server_url: "test.example.com:443".to_string(),
        };

        assert!(store.apply_mutation(&remove_mutation));
        assert_eq!(store.certificates.len(), initial_count);

        // Test removing non-existent certificate
        assert!(!store.apply_mutation(&remove_mutation));
    }
}
