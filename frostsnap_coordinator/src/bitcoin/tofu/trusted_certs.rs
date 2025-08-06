use anyhow::Result;
use frostsnap_core::hex;
use rustls::RootCertStore;
use rustls_pki_types::CertificateDer;
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::persist::Persist;

#[derive(Clone, Debug)]
pub struct TrustedCertificate {
    pub certificate: CertificateDer<'static>,
    pub added_at: i64, // Unix timestamp
}

#[derive(Default, Debug, Clone)]
pub struct TrustedCertificates {
    /// Maps server_url -> trusted certificate
    certificates: HashMap<String, TrustedCertificate>,
}

#[derive(Debug, Clone)]
pub enum CertificateMutation {
    Add {
        certificate: CertificateDer<'static>,
        server_url: String,
    },
    Remove {
        server_url: String,
    },
}

impl TrustedCertificates {
    pub fn apply_mutation(&mut self, mutation: &CertificateMutation) -> bool {
        match mutation {
            CertificateMutation::Add {
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
                    }
                );
                true
            }
            CertificateMutation::Remove { server_url } => {
                self.certificates.remove(server_url).is_some()
            }
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
                certificate: cert,
                server_url,
            },
            mutations,
        );
    }

    pub fn remove_certificate(&mut self, server_url: String, mutations: &mut Vec<CertificateMutation>) {
        self.mutate(CertificateMutation::Remove { server_url }, mutations);
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

    pub fn find_certificate_by_fingerprint(&self, fingerprint: &str) -> Option<(&str, &TrustedCertificate)> {
        self.certificates.iter()
            .find(|(_, cert)| {
                let cert_fingerprint = hex::encode(&Sha256::digest(&cert.certificate));
                cert_fingerprint == fingerprint
            })
            .map(|(url, cert)| (url.as_str(), cert))
    }

    pub fn get_all_certificates(&self) -> Vec<(String, TrustedCertificate)> {
        self.certificates.iter()
            .map(|(url, cert)| (url.clone(), cert.clone()))
            .collect()
    }

    pub fn is_certificate_trusted(&self, cert: &CertificateDer) -> bool {
        let fingerprint = hex::encode(&Sha256::digest(cert));
        self.certificates.values().any(|trusted| {
            let trusted_fingerprint = hex::encode(&Sha256::digest(&trusted.certificate));
            trusted_fingerprint == fingerprint
        })
    }
    
    pub fn get_certificate_for_server(&self, server_url: &str) -> Option<&CertificateDer> {
        tracing::debug!("Looking for certificate for '{}', have {} stored certs", server_url, self.certificates.len());
        for (key, _) in &self.certificates {
            tracing::debug!("  - Stored cert for: '{}'", key);
        }
        self.certificates.get(server_url).map(|tc| &tc.certificate)
    }
}

// Pre-trusted certificate for electrum.frostsn.app
const ELECTRUM_FROSTSN_APP_CERT: &[u8] = include_bytes!("certs/electrum.frostsn.app.der");
const ELECTRUM_FROSTSN_APP_URL: &str = "electrum.frostsn.app";

impl Persist<rusqlite::Connection> for TrustedCertificates {
    type Update = Vec<CertificateMutation>;
    type InitParams = ();

    fn initialize(conn: &mut rusqlite::Connection, _: Self::InitParams) -> Result<Self>
    where
        Self: Sized,
    {
        // Check if table exists
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='fs_trusted_certificates'",
            [],
            |row| row.get(0),
        ).unwrap_or(0) > 0;
        
        // Create table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS fs_trusted_certificates (
                server_url TEXT PRIMARY KEY,
                certificate BLOB NOT NULL,
                added_at INTEGER NOT NULL
            )",
            [],
        )?;
        
        // If table was just created, add pre-trusted certificate
        if !table_exists {
            tracing::info!("First time initialization - adding pre-trusted certificate for electrum.frostsn.app");
            let electrum_cert = CertificateDer::from(ELECTRUM_FROSTSN_APP_CERT.to_vec());
            let added_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
            
            conn.execute(
                "INSERT INTO fs_trusted_certificates (server_url, certificate, added_at) VALUES (?1, ?2, ?3)",
                params![ELECTRUM_FROSTSN_APP_URL, electrum_cert.as_ref(), added_at],
            )?;
        }

        let mut stmt = conn.prepare(
            "SELECT server_url, certificate, added_at FROM fs_trusted_certificates",
        )?;

        let cert_iter = stmt.query_map([], |row| {
            let server_url: String = row.get(0)?;
            let cert_blob: Vec<u8> = row.get(1)?;
            let added_at: i64 = row.get(2)?;

            Ok((server_url, TrustedCertificate {
                certificate: CertificateDer::from(cert_blob),
                added_at,
            }))
        })?;

        let mut trusted_certs = TrustedCertificates::default();

        for cert_result in cert_iter {
            match cert_result {
                Ok((server_url, cert)) => {
                    tracing::debug!(
                        "Loaded trusted certificate for {} added at {}",
                        server_url,
                        cert.added_at
                    );
                    trusted_certs.apply_mutation(&CertificateMutation::Add {
                        certificate: cert.certificate,
                        server_url,
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to load trusted certificate: {:?}", e);
                }
            }
        }

        tracing::info!("Loaded {} trusted certificates", trusted_certs.certificates.len());

        Ok(trusted_certs)
    }

    fn persist_update(conn: &mut rusqlite::Connection, update: Self::Update) -> Result<()> {
        for mutation in update {
            match mutation {
                CertificateMutation::Add {
                    certificate,
                    server_url,
                } => {
                    // Calculate fingerprint
                    let fingerprint = hex::encode(&Sha256::digest(&certificate));
                    let added_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

                    tracing::info!(
                        "Adding trusted certificate for {} with fingerprint {}",
                        server_url,
                        fingerprint
                    );

                    conn.execute(
                        "INSERT OR REPLACE INTO fs_trusted_certificates 
                         (server_url, certificate, added_at) 
                         VALUES (?1, ?2, ?3)",
                        params![server_url, certificate.as_ref(), added_at],
                    )?;
                }
                CertificateMutation::Remove { server_url } => {
                    tracing::info!("Removing trusted certificate for {}", server_url);

                    let rows_affected = conn.execute(
                        "DELETE FROM fs_trusted_certificates WHERE server_url = ?1",
                        params![server_url],
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

        // Initialize store - it will have pre-trusted certificates
        let mut store = TrustedCertificates::initialize(&mut conn, ())?;
        let initial_count = store.certificates.len();

        // Add a certificate
        let cert = create_test_cert();
        let mut mutations = Vec::new();
        store.add_certificate(cert.clone(), "test.example.com:443".to_string(), &mut mutations);

        // Persist the mutation
        TrustedCertificates::persist_update(&mut conn, mutations)?;

        // Load again and verify
        let loaded_store = TrustedCertificates::initialize(&mut conn, ())?;
        assert_eq!(loaded_store.certificates.len(), initial_count + 1);
        assert!(loaded_store.certificates.contains_key("test.example.com:443"));

        Ok(())
    }

    #[test]
    fn test_remove_certificate() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let mut conn = rusqlite::Connection::open(temp_file.path())?;

        // Initialize and add a certificate
        let mut store = TrustedCertificates::initialize(&mut conn, ())?;
        let initial_count = store.certificates.len();
        let cert = create_test_cert();
        let mut mutations = Vec::new();
        store.add_certificate(cert.clone(), "test.example.com:443".to_string(), &mut mutations);
        TrustedCertificates::persist_update(&mut conn, mutations)?;

        // Remove by server URL
        let mut mutations = Vec::new();
        store.remove_certificate("test.example.com:443".to_string(), &mut mutations);
        TrustedCertificates::persist_update(&mut conn, mutations)?;

        // Verify it's removed
        let loaded_store = TrustedCertificates::initialize(&mut conn, ())?;
        assert_eq!(loaded_store.certificates.len(), initial_count);

        Ok(())
    }

    #[test]
    fn test_is_certificate_trusted() {
        let mut store = TrustedCertificates::default();
        let cert = create_test_cert();
        
        assert!(!store.is_certificate_trusted(&cert));
        
        let mut mutations = Vec::new();
        store.add_certificate(cert.clone(), "test.example.com:443".to_string(), &mut mutations);
        
        assert!(store.is_certificate_trusted(&cert));
        assert_eq!(mutations.len(), 1);
    }

    #[test]
    fn test_pre_trusted_cert_only_inserted_once() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        
        // First initialization - should insert pre-trusted cert
        {
            let mut conn = rusqlite::Connection::open(temp_file.path())?;
            let store1 = TrustedCertificates::initialize(&mut conn, ())?;
            assert_eq!(store1.certificates.len(), 1);
            assert!(store1.certificates.contains_key(ELECTRUM_FROSTSN_APP_URL));
        }
        
        // Second initialization - should NOT insert again
        {
            let mut conn = rusqlite::Connection::open(temp_file.path())?;
            let store2 = TrustedCertificates::initialize(&mut conn, ())?;
            assert_eq!(store2.certificates.len(), 1);
            assert!(store2.certificates.contains_key(ELECTRUM_FROSTSN_APP_URL));
            
            // Verify there's still only one row in the database
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM fs_trusted_certificates",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 1);
        }
        
        Ok(())
    }

    #[test]
    fn test_apply_mutation() {
        let mut store = TrustedCertificates::default();
        let initial_count = store.certificates.len();
        let cert = create_test_cert();
        
        // Test add mutation
        let add_mutation = CertificateMutation::Add {
            certificate: cert.clone(),
            server_url: "test.example.com:443".to_string(),
        };
        
        assert!(store.apply_mutation(&add_mutation));
        assert_eq!(store.certificates.len(), initial_count + 1);
        assert!(store.certificates.contains_key("test.example.com:443"));
        
        // Test remove mutation
        let remove_mutation = CertificateMutation::Remove { 
            server_url: "test.example.com:443".to_string() 
        };
        
        assert!(store.apply_mutation(&remove_mutation));
        assert_eq!(store.certificates.len(), initial_count);
        
        // Test removing non-existent certificate
        assert!(!store.apply_mutation(&remove_mutation));
    }
}