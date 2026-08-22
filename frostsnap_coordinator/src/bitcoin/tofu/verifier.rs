use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::trusted_certs::{TrustKey, TrustedCertificates};

/// Extension trait for certificate fingerprinting
pub trait CertificateExt {
    /// Get SHA256 fingerprint as hex string (no formatting)
    fn sha256_fingerprint(&self) -> String;
}

impl CertificateExt for CertificateDer<'_> {
    fn sha256_fingerprint(&self) -> String {
        use frostsnap_core::hex;
        use sha2::{Digest, Sha256};
        hex::encode(&Sha256::digest(self.as_ref()))
    }
}

/// An untrusted certificate that needs user approval
#[derive(Debug, Clone)]
pub struct UntrustedCertificate {
    pub fingerprint: String,
    pub server_url: String,
    pub is_changed: bool,
    pub old_fingerprint: Option<String>,
    pub certificate_der: Vec<u8>,
    /// If the certificate was rejected due to name mismatch, this contains the names it's valid for
    pub valid_for_names: Option<Vec<String>>,
}

/// Custom error type that includes certificate data for TOFU prompts
#[derive(Debug)]
pub enum TofuError {
    /// Certificate not trusted - needs user approval. Boxed because the certificate carries a DER
    /// blob, and an `Err` this large is paid for on every `Ok` return of every function in the chain.
    NotTrusted(Box<UntrustedCertificate>),
    /// Other connection error
    Other(anyhow::Error),
}

impl std::fmt::Display for TofuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TofuError::NotTrusted(cert) => {
                if cert.is_changed {
                    write!(f, "Certificate changed for {}", cert.server_url)
                } else {
                    write!(f, "Untrusted certificate for {}", cert.server_url)
                }
            }
            TofuError::Other(ref e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for TofuError {}

impl From<anyhow::Error> for TofuError {
    fn from(err: anyhow::Error) -> Self {
        TofuError::Other(err)
    }
}

/// A certificate verifier that captures certificates when validation fails
///
/// # TOFU Implementation Trade-offs
///
/// This implementation uses a pragmatic approach to TOFU (Trust On First Use) that has
/// some important security trade-offs:
///
/// 1. **Certificate Validation Order**: We rely on rustls's base verifier to validate
///    certificates. This means validation happens in a specific order (parsing, type
///    checking, signature verification, trust chain validation). We only capture
///    certificates that fail at the trust chain level (UnknownIssuer, etc.).
///
/// 2. **Signature Verification Limitation**: We CANNOT guarantee that captured certificates
///    have valid signatures. If a certificate fails for UnknownIssuer before signature
///    verification happens, we'll still capture it. This means a tampered certificate
///    could theoretically be presented for TOFU if it fails for other reasons first.
///
/// 3. **Why We Accept This**: In practice, this is a reasonable trade-off because:
///    - TOFU is already about accepting certificates outside the PKI trust model
///    - The real security comes from detecting when certificates CHANGE after first trust
///    - Implementing proper signature verification before capture would require
///      reimplementing significant parts of X.509 validation
///    - Most real-world attacks would produce certificates that fail validation entirely
///
/// 4. **What We DO Guarantee**:
///    - Certificates are properly scoped to specific servers (no certificate confusion). This
///      relies on TOFU certs never entering the PKI root store — see `connect_with_tofu`.
///    - Once trusted, we detect any changes to the certificate
///    - We don't capture certificates with structural problems
pub struct TofuCertVerifier {
    /// The base verifier that does actual validation
    base_verifier: Arc<dyn ServerCertVerifier>,
    /// Trusted certificates store
    trusted_certs: TrustedCertificates,
    /// Storage for TOFU errors indexed by server URL
    tofu_errors: Arc<Mutex<HashMap<TrustKey, TofuError>>>,
}

impl std::fmt::Debug for TofuCertVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TofuCertVerifier").finish()
    }
}

impl TofuCertVerifier {
    pub fn new(
        base_verifier: Arc<dyn ServerCertVerifier>,
        trusted_certs: TrustedCertificates,
    ) -> Self {
        Self {
            base_verifier,
            trusted_certs,
            tofu_errors: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Extract the TOFU error for a specific server if any
    pub fn take_tofu_error(&self, server_url: &TrustKey) -> Option<TofuError> {
        self.tofu_errors.lock().unwrap().remove(server_url)
    }
}

impl ServerCertVerifier for TofuCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let server_key = TrustKey::new(server_name.to_str());

        tracing::info!("TOFU verifier checking server: {}", server_key);

        // Check if we have a TOFU certificate for this server
        let previous_cert = self.trusted_certs.get_certificate_for_server(&server_key);

        if let Some(trusted_cert) = &previous_cert {
            if trusted_cert == &end_entity {
                tracing::info!("Certificate matches TOFU trust store, accepting");
                return Ok(ServerCertVerified::assertion());
            } else {
                // Certificate changed - this requires user approval, not PKI fallback
                tracing::warn!(
                    "Certificate for {} has changed - capturing for user approval",
                    server_key
                );

                let untrusted_cert = UntrustedCertificate {
                    fingerprint: end_entity.sha256_fingerprint(),
                    server_url: server_key.to_string(),
                    is_changed: true,
                    old_fingerprint: Some(trusted_cert.sha256_fingerprint()),
                    certificate_der: end_entity.to_vec(),
                    valid_for_names: None,
                };

                let tofu_error = TofuError::NotTrusted(Box::new(untrusted_cert));
                self.tofu_errors
                    .lock()
                    .unwrap()
                    .insert(server_key, tofu_error);

                return Err(RustlsError::General(
                    "Certificate changed - requires user approval".to_string(),
                ));
            }
        } else {
            tracing::info!("No stored certificate found for {}", server_key);
        }

        match self.base_verifier.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Ok(verified) => {
                tracing::debug!("Certificate validated via PKI for {}", server_key);
                Ok(verified)
            }
            Err(e) => {
                // Only capture certificates that fail due to trust issues, not structural issues
                // This is a best-effort filter - we try to capture certificates that are
                // structurally valid but not trusted by the PKI model.
                //
                // IMPORTANT: Due to validation order in rustls, we cannot guarantee that
                // certificates with invalid signatures won't be captured. If a certificate
                // fails for UnknownIssuer before signature verification, it will be captured.
                let should_capture = match &e {
                    RustlsError::InvalidCertificate(cert_err) => match cert_err {
                        rustls::CertificateError::UnknownIssuer |           // Self-signed or unknown CA
                        rustls::CertificateError::UnhandledCriticalExtension | // Unusual but valid cert
                        rustls::CertificateError::NotValidForName |         // Name mismatch (might be OK for TOFU)
                        rustls::CertificateError::NotValidForNameContext { .. } | // Name mismatch with details
                        rustls::CertificateError::InvalidPurpose => true,   // Wrong key usage (might be OK for TOFU)
                        rustls::CertificateError::Other(other_err) => {
                            // Check if it's UnsupportedCertVersion - if so, don't capture because we'll never be able to set up the connection anyway.
                            let err_str = format!("{:?}", other_err);
                            !err_str.contains("UnsupportedCertVersion")
                        }
                        // We explicitly don't capture:
                        // - BadSignature (definitely tampered)
                        // - ApplicationVerificationFailure
                        _ => false,
                    },
                    _ => false,
                };

                if should_capture {
                    let valid_for_names = match &e {
                        RustlsError::InvalidCertificate(
                            rustls::CertificateError::NotValidForNameContext {
                                expected: _,
                                presented,
                            },
                        ) => Some(presented.clone()),
                        _ => None,
                    };

                    let untrusted_cert = UntrustedCertificate {
                        fingerprint: end_entity.sha256_fingerprint(),
                        server_url: server_key.to_string(),
                        is_changed: previous_cert.is_some(),
                        old_fingerprint: previous_cert.map(|cert| cert.sha256_fingerprint()),
                        certificate_der: end_entity.to_vec(),
                        valid_for_names,
                    };

                    let tofu_error = TofuError::NotTrusted(Box::new(untrusted_cert));
                    self.tofu_errors
                        .lock()
                        .unwrap()
                        .insert(server_key, tofu_error);
                }

                Err(e)
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.base_verifier
            .verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.base_verifier
            .verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.base_verifier.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::tofu::trusted_certs::{TrustKey, TrustedCertificates};
    use rustls::client::WebPkiServerVerifier;
    use rustls::pki_types::{ServerName, UnixTime};

    const TEST_CERT1: &[u8] = include_bytes!("../../../tests/certs/test1.der");
    const TEST_CERT2: &[u8] = include_bytes!("../../../tests/certs/test2.der");

    /// A fixed verification time inside the `test1.der`/`test2.der` validity window
    /// (2025-07-25 .. 2026-07-25). These tests exercise TOFU handling of an
    /// *otherwise-valid* self-signed certificate, so the verifier's `now` is a test
    /// input and must not depend on the wall clock — otherwise the fixtures "expire"
    /// and the base verifier returns cert-expired instead of the unknown-issuer path
    /// the TOFU capture relies on. (Certificate-expiry behaviour, if ever needed,
    /// belongs in its own test with its own chosen time.)
    fn test_now() -> UnixTime {
        // 2026-01-01T00:00:00Z — inside the test fixtures' validity window
        UnixTime::since_unix_epoch(std::time::Duration::from_secs(1_767_225_600))
    }

    #[test]
    fn test_certificate_fingerprint_format() {
        const ELECTRUM_CERT: &[u8] = include_bytes!("../tofu/certs/electrum.frostsn.app.der");
        let cert = CertificateDer::from(ELECTRUM_CERT.to_vec());

        // Expected fingerprint from: openssl x509 -in electrum.frostsn.app.der -inform DER -noout -fingerprint -sha256
        const OPENSSL_FINGERPRINT: &str = "9F:32:AC:77:62:64:67:39:C2:FE:62:17:04:09:8F:DA:E4:94:49:BB:B1:E2:3A:FB:F8:ED:22:47:B6:F9:15:F9";
        let expected = OPENSSL_FINGERPRINT.replace(':', "").to_lowercase();

        assert_eq!(
            cert.sha256_fingerprint(),
            expected,
            "Fingerprint should be raw hex SHA256"
        );
    }

    #[test]
    fn test_tofu_first_use_capture() {
        let trusted_certs = TrustedCertificates::new_for_test(bdk_chain::bitcoin::Network::Bitcoin);
        let test_cert1 = CertificateDer::from(TEST_CERT1.to_vec());
        let server_name = ServerName::try_from("test.example.com").unwrap();
        let now = test_now();

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();

        let tofu_verifier = TofuCertVerifier::new(base_verifier.clone(), trusted_certs.clone());

        let result = tofu_verifier.verify_server_cert(&test_cert1, &[], &server_name, &[], now);
        assert!(result.is_err(), "First connection should fail");

        let tofu_error = tofu_verifier.take_tofu_error(&TrustKey::new("test.example.com"));
        assert!(tofu_error.is_some(), "Should have captured TOFU error");
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                assert!(!cert.is_changed);
                assert_eq!(cert.certificate_der, test_cert1.as_ref());
            }
            _ => panic!("Expected NotTrusted error"),
        }
    }

    #[test]
    fn test_tofu_trusted_certificate_accepted() {
        let mut trusted_certs =
            TrustedCertificates::new_for_test(bdk_chain::bitcoin::Network::Bitcoin);
        let mut mutations = Vec::new();

        let test_cert1 = CertificateDer::from(TEST_CERT1.to_vec());
        let server_name = ServerName::try_from("test.example.com").unwrap();
        let now = test_now();

        trusted_certs.add_certificate(
            test_cert1.clone(),
            TrustKey::new("test.example.com"),
            &mut mutations,
        );

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();

        let tofu_verifier = TofuCertVerifier::new(base_verifier, trusted_certs);

        let result = tofu_verifier.verify_server_cert(&test_cert1, &[], &server_name, &[], now);
        assert!(result.is_ok(), "Trusted certificate should be accepted");
    }

    /// Re-spelling a host name in different capitalisation must not get you past a pin. Both
    /// directions are checked: the mixed-case spelling being the one in the trust store, and it
    /// being the one we connect to.
    #[test]
    fn test_tofu_trust_is_case_insensitive() {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();

        let test_cert1 = CertificateDer::from(TEST_CERT1.to_vec());
        let now = test_now();
        let mut mutations = Vec::new();

        for (stored, connected_to) in [
            ("test.example.com", "Test.Example.Com"),
            ("Test.Example.Com", "test.example.com"),
        ] {
            let mut trusted_certs =
                TrustedCertificates::new_for_test(bdk_chain::bitcoin::Network::Bitcoin);
            trusted_certs.add_certificate(
                test_cert1.clone(),
                TrustKey::new(stored),
                &mut mutations,
            );

            let tofu_verifier = TofuCertVerifier::new(base_verifier.clone(), trusted_certs);
            let server_name = ServerName::try_from(connected_to).unwrap();

            let result = tofu_verifier.verify_server_cert(&test_cert1, &[], &server_name, &[], now);
            assert!(
                result.is_ok(),
                "certificate trusted for {stored} should be trusted for {connected_to}"
            );
        }
    }

    #[test]
    fn test_tofu_certificate_change_detection() {
        let mut trusted_certs =
            TrustedCertificates::new_for_test(bdk_chain::bitcoin::Network::Bitcoin);
        let mut mutations = Vec::new();

        let test_cert1 = CertificateDer::from(TEST_CERT1.to_vec());
        let test_cert2 = CertificateDer::from(TEST_CERT2.to_vec());
        let server_name = ServerName::try_from("test.example.com").unwrap();
        let now = test_now();

        trusted_certs.add_certificate(
            test_cert1.clone(),
            TrustKey::new("test.example.com"),
            &mut mutations,
        );

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();

        let tofu_verifier = TofuCertVerifier::new(base_verifier, trusted_certs);

        let result = tofu_verifier.verify_server_cert(&test_cert2, &[], &server_name, &[], now);
        assert!(result.is_err(), "Different certificate should fail");

        let tofu_error = tofu_verifier.take_tofu_error(&TrustKey::new("test.example.com"));
        assert!(
            tofu_error.is_some(),
            "Should have captured changed certificate"
        );
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                assert!(cert.is_changed, "Should be marked as changed");
                assert_eq!(cert.certificate_der, test_cert2.as_ref());
                assert_eq!(cert.old_fingerprint, Some(test_cert1.sha256_fingerprint()));
            }
            _ => panic!("Expected NotTrusted error"),
        }
    }

    #[test]
    fn test_certificate_confusion_attack_prevention() {
        // A certificate accepted for one server should NOT be accepted for a different server.
        // Only covers byte-for-byte reuse: TEST_CERT1 has no SAN for "bank.com" so it fails name
        // checking anyway, which is why this can't detect PKI root store leakage on its own.

        let mut trusted_certs =
            TrustedCertificates::new_for_test(bdk_chain::bitcoin::Network::Bitcoin);
        let mut mutations = Vec::new();

        // Load our test certificates
        let cert = CertificateDer::from(TEST_CERT1.to_vec());

        // Create base verifier
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();

        // Step 1: User accepts a certificate for attacker.com
        trusted_certs.add_certificate(cert.clone(), TrustKey::new("attacker.com"), &mut mutations);

        let tofu_verifier = TofuCertVerifier::new(base_verifier.clone(), trusted_certs.clone());

        // Step 2: Attacker tries to use the same certificate for bank.com
        let bank_server_name = ServerName::try_from("bank.com").unwrap();
        let now = test_now();

        let result = tofu_verifier.verify_server_cert(&cert, &[], &bank_server_name, &[], now);

        // This SHOULD fail - the certificate should NOT be accepted for bank.com
        assert!(
            result.is_err(),
            "Certificate should NOT be accepted for a different server!"
        );

        // The certificate should be captured as unverified
        let tofu_error = tofu_verifier.take_tofu_error(&TrustKey::new("bank.com"));
        assert!(
            tofu_error.is_some(),
            "Should have captured the certificate for TOFU prompt"
        );
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                assert!(!cert.is_changed); // First time seeing cert for bank.com
                assert_eq!(cert.server_url, "bank.com");
            }
            _ => panic!("Expected NotTrusted error for bank.com"),
        }
    }

    #[test]
    fn test_certificate_change_detection() {
        let mut trusted_certs =
            TrustedCertificates::new_for_test(bdk_chain::bitcoin::Network::Bitcoin);
        let mut mutations = Vec::new();

        // Add a certificate for a server
        let cert1 = CertificateDer::from(TEST_CERT1.to_vec());
        trusted_certs.add_certificate(
            cert1.clone(),
            TrustKey::new("example.com:443"),
            &mut mutations,
        );

        // Check if we can find it
        let certs = trusted_certs.get_all_certificates();
        let found = certs
            .iter()
            .find(|(url, _)| url.as_str() == "example.com:443");

        assert!(found.is_some(), "Should find the certificate");
        assert_eq!(found.unwrap().1.certificate, cert1);

        // Verify we can get the certificate back
        assert_eq!(
            trusted_certs.get_certificate_for_server(&TrustKey::new("example.com:443")),
            Some(&cert1)
        );

        // Try to add a different certificate for the same server
        let cert2 = CertificateDer::from(TEST_CERT2.to_vec());
        let old_fingerprint = cert1.sha256_fingerprint();
        let new_fingerprint = cert2.sha256_fingerprint();

        // Add second certificate (should replace the first one)
        trusted_certs.add_certificate(
            cert2.clone(),
            TrustKey::new("example.com:443"),
            &mut mutations,
        );

        // Only the second certificate should be stored for this server now
        assert_eq!(
            trusted_certs.get_certificate_for_server(&TrustKey::new("example.com:443")),
            Some(&cert2),
            "Server should now have cert2"
        );

        // Verify fingerprints are different
        assert_ne!(new_fingerprint, old_fingerprint);

        // We should still have only 1 certificate
        assert_eq!(trusted_certs.get_all_certificates().len(), 1);
    }

    #[test]
    fn test_unsupported_cert_version_not_captured() {
        // This test verifies that certificates with unsupported versions
        // (like X.509 v1) are not captured for TOFU and fail immediately

        // Load emzy's cert which has X.509 v1
        const EMZY_CERT: &[u8] = include_bytes!("../../../tests/certs/emzy.der");
        let emzy_cert = CertificateDer::from(EMZY_CERT.to_vec());
        let server_name = ServerName::try_from("electrum.emzy.de").unwrap();
        let now = test_now();

        let trusted_certs = TrustedCertificates::new_for_test(bdk_chain::bitcoin::Network::Bitcoin);

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();

        let tofu_verifier = TofuCertVerifier::new(base_verifier.clone(), trusted_certs);

        // Try to verify the cert - should fail with UnsupportedCertVersion
        let result = tofu_verifier.verify_server_cert(&emzy_cert, &[], &server_name, &[], now);

        assert!(result.is_err());
        let err = result.unwrap_err();

        // Check that it's an UnsupportedCertVersion error
        assert!(
            format!("{:?}", err).contains("UnsupportedCertVersion"),
            "Expected UnsupportedCertVersion error, got: {:?}",
            err
        );

        // Most importantly: verify NO TOFU error was captured
        let tofu_error = tofu_verifier.take_tofu_error(&TrustKey::new("electrum.emzy.de"));
        assert!(
            tofu_error.is_none(),
            "Should NOT capture TOFU error for unsupported cert version"
        );
    }
}
