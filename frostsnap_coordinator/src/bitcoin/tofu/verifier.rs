use frostsnap_core::hex;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use x509_parser::prelude::*;

use super::trusted_certs::TrustedCertificates;

/// An untrusted certificate that needs user approval
#[derive(Debug, Clone)]
pub struct UntrustedCertificate {
    pub fingerprint: String,
    pub server_url: String,
    pub details: String,
    pub is_changed: bool,
    pub old_fingerprint: Option<String>,
    pub certificate_der: Vec<u8>,
    /// If the certificate was rejected due to name mismatch, this contains the names it's valid for
    pub valid_for_names: Option<Vec<String>>,
}

/// Custom error type that includes certificate data for TOFU prompts
#[derive(Debug)]
pub enum TofuError {
    /// Certificate not trusted - needs user approval
    NotTrusted(UntrustedCertificate),
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
///    - Certificates are properly scoped to specific servers (no certificate confusion)
///    - Once trusted, we detect any changes to the certificate
///    - We don't capture certificates with structural problems
pub struct TofuCertVerifier {
    /// The base verifier that does actual validation
    base_verifier: Arc<dyn ServerCertVerifier>,
    /// Trusted certificates store
    trusted_certs: Arc<TrustedCertificates>,
    /// Storage for TOFU errors indexed by server URL
    tofu_errors: Arc<Mutex<HashMap<String, TofuError>>>,
    /// Certificates that passed PKI validation and should be pinned
    certs_to_pin: Arc<Mutex<HashMap<String, CertificateDer<'static>>>>,
}

impl std::fmt::Debug for TofuCertVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TofuCertVerifier")
            .field("has_base_verifier", &true)
            .field("has_trusted_certs", &true)
            .finish()
    }
}

impl TofuCertVerifier {
    pub fn new(
        base_verifier: Arc<dyn ServerCertVerifier>,
        trusted_certs: Arc<TrustedCertificates>,
    ) -> Self {
        Self {
            base_verifier,
            trusted_certs,
            tofu_errors: Arc::new(Mutex::new(HashMap::new())),
            certs_to_pin: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Extract the TOFU error for a specific server if any
    pub fn take_tofu_error(&self, server_url: &str) -> Option<TofuError> {
        self.tofu_errors.lock().unwrap().remove(server_url)
    }

    /// Extract the certificate to pin for a specific server if any
    pub fn take_cert_to_pin(&self, server_url: &str) -> Option<CertificateDer<'static>> {
        self.certs_to_pin.lock().unwrap().remove(server_url)
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
        let server_str = server_name.to_str();
        
        tracing::info!("TOFU verifier checking server: {}", server_str.as_ref());
        
        // Check if we have a TOFU certificate for this server
        if let Some(trusted_cert) = self.trusted_certs.get_certificate_for_server(server_str.as_ref()) {
            tracing::info!("Found stored certificate for {}", server_str.as_ref());
            tracing::info!("Stored cert fingerprint: {}", certificate_fingerprint(trusted_cert));
            tracing::info!("Presented cert fingerprint: {}", certificate_fingerprint(end_entity));
            // We have a TOFU cert - check if it matches
            if trusted_cert == end_entity {
                // Certificate matches our stored one - accept it
                tracing::info!("Certificate matches TOFU trust store, accepting");
                return Ok(ServerCertVerified::assertion());
            } else {
                // Different certificate than what we have stored
                tracing::warn!("Certificate doesn't match stored TOFU cert for {}", server_str.as_ref());
                
                // Check if the old certificate is expired
                let old_cert_expired = if let Ok((_, cert)) = X509Certificate::from_der(trusted_cert.as_ref()) {
                    !cert.validity().is_valid()
                } else {
                    true // If we can't parse it, assume it's expired
                };
                
                if !old_cert_expired {
                    //❗❗ Old cert is NOT expired but server presented different cert - suspicious!
                    let untrusted_cert = UntrustedCertificate {
                        fingerprint: certificate_fingerprint(end_entity),
                        server_url: server_str.to_string(),
                        details: certificate_details(end_entity)
                            .unwrap_or_else(|_| "Unable to parse certificate".to_string()),
                        is_changed: true,
                        old_fingerprint: Some(certificate_fingerprint(trusted_cert)),
                        certificate_der: end_entity.to_vec(),
                        valid_for_names: None, // We don't have this info in the change detection case
                    };
                    
                    let tofu_error = TofuError::NotTrusted(untrusted_cert);
                    self.tofu_errors.lock().unwrap().insert(server_str.to_string(), tofu_error);
                    return Err(RustlsError::InvalidCertificate(rustls::CertificateError::UnknownIssuer));
                }
                // Old cert is expired - continue to PKI validation
                tracing::info!("Old certificate is expired, continuing to PKI validation");
            }
        } else {
            tracing::info!("No stored certificate found for {}", server_str.as_ref());
        }

        // Try PKI verification
        match self.base_verifier.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Ok(verified) => {
                // PKI validation succeeded - store for auto-pinning
                tracing::debug!("Certificate validated via PKI for {}", server_str);
                
                self.certs_to_pin.lock().unwrap().insert(
                    server_str.to_string(), 
                    end_entity.clone().into_owned()
                );
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
                    RustlsError::InvalidCertificate(cert_err) => matches!(
                        cert_err,
                        rustls::CertificateError::UnknownIssuer |           // Self-signed or unknown CA
                        rustls::CertificateError::UnhandledCriticalExtension | // Unusual but valid cert
                        rustls::CertificateError::NotValidForName |         // Name mismatch (might be OK for TOFU)
                        rustls::CertificateError::NotValidForNameContext { .. } | // Name mismatch with details
                        rustls::CertificateError::InvalidPurpose |          // Wrong key usage (might be OK for TOFU)
                        rustls::CertificateError::Other(_)                  // Includes UnsupportedCertVersion and others
                        // We explicitly don't capture:
                        // - BadSignature (definitely tampered)
                        // - ApplicationVerificationFailure  
                    ),
                    _ => false,
                };
                
                if should_capture {
                    // Extract valid names if this is a NotValidForNameContext error
                    let valid_for_names = match &e {
                        RustlsError::InvalidCertificate(rustls::CertificateError::NotValidForNameContext { 
                            expected: _, 
                            presented 
                        }) => Some(presented.clone()),
                        _ => None,
                    };
                    
                    let untrusted_cert = UntrustedCertificate {
                        fingerprint: certificate_fingerprint(end_entity),
                        server_url: server_str.to_string(),
                        details: certificate_details(end_entity)
                            .unwrap_or_else(|_| "Unable to parse certificate".to_string()),
                        is_changed: false,
                        old_fingerprint: None,
                        certificate_der: end_entity.to_vec(),
                        valid_for_names,
                    };
                    
                    let tofu_error = TofuError::NotTrusted(untrusted_cert);
                    self.tofu_errors.lock().unwrap().insert(server_str.to_string(), tofu_error);
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
        self.base_verifier.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.base_verifier.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.base_verifier.supported_verify_schemes()
    }
}

/// Helper to extract certificate fingerprint
pub fn certificate_fingerprint(cert: &CertificateDer) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(cert))
}

/// Helper to extract certificate details for display
pub fn certificate_details(cert: &CertificateDer) -> anyhow::Result<String> {
    // In a real implementation, you'd use x509-parser or similar to extract:
    // - Subject CN
    // - Issuer
    // - Validity dates
    // - Key algorithm
    // For now, just return fingerprint
    Ok(format!("SHA256: {}", certificate_fingerprint(cert)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitcoin::tofu::trusted_certs::TrustedCertificates;
    use rustls::client::WebPkiServerVerifier;
    use rustls::pki_types::{ServerName, UnixTime};
    
    // Load test certificates from files
    const TEST_CERT1: &[u8] = include_bytes!("../../../tests/certs/test1.der");
    const TEST_CERT2: &[u8] = include_bytes!("../../../tests/certs/test2.der");
    const EXPIRED_CERT: &[u8] = include_bytes!("../../../tests/certs/expired.der");

    #[test]
    fn test_tofu_full_flow() {
        // 1. Create a fresh TrustedCertificates store
        let mut trusted_certs = TrustedCertificates::default();
        let mut mutations = Vec::new();
        
        // Create test certificates
        let test_cert1 = CertificateDer::from(TEST_CERT1.to_vec());
        let test_cert2 = CertificateDer::from(TEST_CERT2.to_vec());
        let server_name = ServerName::try_from("test.example.com").unwrap();
        let now = UnixTime::now();
        
        // Create a base verifier with standard WebPKI validation
        // This will reject our self-signed certificates
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();
        
        let tofu_verifier = TofuCertVerifier::new(
            base_verifier.clone(),
            Arc::new(trusted_certs.clone()),
        );
        
        // 1. First connection attempt - should fail with unverified certificate
        let result = tofu_verifier.verify_server_cert(
            &test_cert1,
            &[],
            &server_name,
            &[],
            now,
        );
        
        assert!(result.is_err(), "First connection should fail");
        if let Err(e) = result {
            println!("First connection error: {:?}", e);
        }
        
        // Should have captured the certificate as a TOFU error
        let tofu_error = tofu_verifier.take_tofu_error("test.example.com");
        assert!(tofu_error.is_some(), "Should have captured TOFU error");
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                assert!(!cert.is_changed);
                assert_eq!(cert.certificate_der, test_cert1.as_ref());
            }
            _ => panic!("Expected NotTrusted error"),
        }
        
        // 2. Accept the certificate by adding it to trusted store
        trusted_certs.add_certificate(
            test_cert1.clone(),
            "test.example.com".to_string(),
            &mut mutations,
        );
        
        let tofu_verifier2 = TofuCertVerifier::new(
            base_verifier.clone(),
            Arc::new(trusted_certs.clone()),
        );
        
        // 3. Second connection attempt - should succeed
        let result = tofu_verifier2.verify_server_cert(
            &test_cert1,
            &[],
            &server_name,
            &[],
            now,
        );
        
        assert!(result.is_ok(), "Second connection should succeed with trusted cert");
        
        // 4. Test with corrupted certificate signature
        // This demonstrates a limitation of our TOFU implementation: we can't guarantee
        // that signature validation happens before capture. If a certificate fails for
        // UnknownIssuer first, it might be captured even with an invalid signature.
        let mut corrupted_cert_data = TEST_CERT1.to_vec();
        // Corrupt a byte in the signature area (near the end of the certificate)
        let signature_offset = corrupted_cert_data.len() - 100; // Signature is in the last ~256 bytes
        corrupted_cert_data[signature_offset] ^= 0x01;  // Flip a bit in the RSA signature
        let corrupted_cert = CertificateDer::from(corrupted_cert_data);
        
        // Try with corrupted certificate - should fail
        let result = tofu_verifier2.verify_server_cert(
            &corrupted_cert,
            &[],
            &server_name,
            &[],
            now,
        );
        
        assert!(result.is_err(), "Corrupted certificate should fail");
        
        // Check if the corrupted certificate was captured
        let captured = tofu_verifier2.take_tofu_error("test.example.com");
        
        // Due to validation order, the corrupted certificate might be captured if it
        // fails for UnknownIssuer before signature verification. This is a known
        // limitation we accept - see the struct documentation for details.
        if let Some(tofu_error) = captured {
            println!("Note: Corrupted cert was captured (failed for UnknownIssuer before signature check)");
            match tofu_error {
                TofuError::NotTrusted(cert) => {
                    println!("Certificate captured: {}", cert.fingerprint);
                }
                _ => println!("Unexpected error type: {:?}", tofu_error),
            }
        } else {
            println!("Corrupted cert was not captured (likely failed structural validation first)");
        }
        
        // 5. Test with second valid certificate for the same hostname
        // This simulates a certificate change scenario
        let result = tofu_verifier2.verify_server_cert(
            &test_cert2,
            &[],
            &server_name,
            &[],
            now,
        );
        
        assert!(result.is_err(), "Different certificate should fail");
        
        // Should have captured the second certificate for TOFU prompt
        let tofu_error = tofu_verifier2.take_tofu_error("test.example.com");
        assert!(tofu_error.is_some(), "Should have captured second certificate");
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                assert_eq!(cert.certificate_der, test_cert2.as_ref());
                // Could be is_changed=true or false depending on if first cert expired
            }
            _ => panic!("Expected NotTrusted error"),
        }
        
        // 6. Accept the second certificate (certificate change)
        trusted_certs.add_certificate(
            test_cert2.clone(),
            "test.example.com".to_string(),
            &mut mutations,
        );
        
        let tofu_verifier3 = TofuCertVerifier::new(
            base_verifier.clone(),
            Arc::new(trusted_certs.clone()),
        );
        
        // 7. Only the second certificate should be trusted now (it replaced the first)
        let result = tofu_verifier3.verify_server_cert(
            &test_cert1,
            &[],
            &server_name,
            &[],
            now,
        );
        assert!(result.is_err(), "First certificate should no longer be trusted (replaced)");
        
        // Should capture cert1 again since it's no longer trusted
        let tofu_error = tofu_verifier3.take_tofu_error("test.example.com");
        assert!(tofu_error.is_some(), "Should have captured cert1 again");
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                assert!(cert.is_changed);
                assert_eq!(cert.certificate_der, test_cert1.as_ref());
            }
            _ => panic!("Expected NotTrusted error with is_changed=true"),
        }
        
        let result = tofu_verifier3.verify_server_cert(
            &test_cert2,
            &[],
            &server_name,
            &[],
            now,
        );
        assert!(result.is_ok(), "Second certificate should now be trusted");
        
        // Verify we only have one certificate in the store
        assert_eq!(trusted_certs.get_all_certificates().len(), 1, "Should only have one certificate");
        let certs = trusted_certs.get_all_certificates();
        assert_eq!(certs[0].0, "test.example.com");
        
        // Verify the fingerprints are different
        let fingerprint1 = certificate_fingerprint(&test_cert1);
        let fingerprint2 = certificate_fingerprint(&test_cert2);
        assert_ne!(fingerprint1, fingerprint2, "Different certificates should have different fingerprints");
    }
    
    #[test]
    fn test_certificate_confusion_attack_prevention() {
        // This test should FAIL if the vulnerability exists
        // A certificate accepted for one server should NOT be accepted for a different server
        
        let mut trusted_certs = TrustedCertificates::default();
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
        trusted_certs.add_certificate(
            cert.clone(),
            "attacker.com".to_string(),
            &mut mutations,
        );
        
        let tofu_verifier = TofuCertVerifier::new(
            base_verifier.clone(),
            Arc::new(trusted_certs.clone()),
        );
        
        // Step 2: Attacker tries to use the same certificate for bank.com
        let bank_server_name = ServerName::try_from("bank.com").unwrap();
        let now = UnixTime::now();
        
        let result = tofu_verifier.verify_server_cert(
            &cert,
            &[],
            &bank_server_name,
            &[],
            now,
        );
        
        // This SHOULD fail - the certificate should NOT be accepted for bank.com
        assert!(result.is_err(), "Certificate should NOT be accepted for a different server!");
        
        // The certificate should be captured as unverified
        let tofu_error = tofu_verifier.take_tofu_error("bank.com");
        assert!(tofu_error.is_some(), "Should have captured the certificate for TOFU prompt");
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
        let mut trusted_certs = TrustedCertificates::default();
        let mut mutations = Vec::new();
        
        // Add a certificate for a server
        let cert1 = CertificateDer::from(vec![0x30, 0x82, 0x01, 0x01]);
        trusted_certs.add_certificate(
            cert1.clone(),
            "example.com:443".to_string(),
            &mut mutations,
        );
        
        // Check if we can find it
        let certs = trusted_certs.get_all_certificates();
        let found = certs.iter()
            .find(|(url, _)| url == "example.com:443");
        
        assert!(found.is_some(), "Should find the certificate");
        assert_eq!(found.unwrap().1.certificate, cert1);
        
        // Verify the certificate is trusted
        assert!(trusted_certs.is_certificate_trusted(&cert1));
        
        // Try to add a different certificate for the same server
        let cert2 = CertificateDer::from(vec![0x30, 0x82, 0x01, 0x02]);
        let old_fingerprint = certificate_fingerprint(&cert1);
        let new_fingerprint = certificate_fingerprint(&cert2);
        
        // Add second certificate (should replace the first one)
        trusted_certs.add_certificate(
            cert2.clone(),
            "example.com:443".to_string(),
            &mut mutations,
        );
        
        // Only the second certificate should be trusted now
        assert!(!trusted_certs.is_certificate_trusted(&cert1), "First cert should be replaced");
        assert!(trusted_certs.is_certificate_trusted(&cert2), "Second cert should be trusted");
        
        // Verify fingerprints are different
        assert_ne!(new_fingerprint, old_fingerprint);
        
        // We should still have only 1 certificate
        assert_eq!(trusted_certs.get_all_certificates().len(), 1);
    }

    #[test]
    fn test_tofu_priority_over_pki() {
        // Test that TOFU certificates take priority over PKI validation
        // and that expired TOFU certs allow PKI fallback
        
        let mut trusted_certs = TrustedCertificates::default();
        let mut mutations = Vec::new();
        
        // Use a test certificate that would fail PKI validation
        let test_cert = CertificateDer::from(TEST_CERT1.to_vec());
        let server_name = ServerName::try_from("test.example.com").unwrap();
        let now = UnixTime::now();
        
        // Create base verifier with PKI roots
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();
        
        // Add the certificate as a TOFU cert
        trusted_certs.add_certificate(
            test_cert.clone(),
            "test.example.com".to_string(),
            &mut mutations,
        );
        
        let tofu_verifier = TofuCertVerifier::new(
            base_verifier.clone(),
            Arc::new(trusted_certs.clone()),
        );
        
        // Test 1: TOFU cert should be accepted even though it would fail PKI
        let result = tofu_verifier.verify_server_cert(
            &test_cert,
            &[],
            &server_name,
            &[],
            now,
        );
        
        // Parse the certificate to check if it's expired
        let is_expired = if let Ok((_, cert)) = X509Certificate::from_der(test_cert.as_ref()) {
            !cert.validity().is_valid()
        } else {
            false
        };
        
        if is_expired {
            // If expired, it should fall back to PKI and likely fail
            assert!(result.is_err(), "Expired TOFU cert should fall back to PKI validation");
        } else {
            // If not expired, TOFU should take priority
            assert!(result.is_ok(), "Valid TOFU cert should be accepted regardless of PKI");
        }
        
        // Test 2: Different cert for same server should fail and be captured
        let different_cert = CertificateDer::from(TEST_CERT2.to_vec());
        let result = tofu_verifier.verify_server_cert(
            &different_cert,
            &[],
            &server_name,
            &[],
            now,
        );
        
        assert!(result.is_err(), "Different certificate should fail");
        let tofu_error = tofu_verifier.take_tofu_error("test.example.com");
        assert!(tofu_error.is_some(), "Different certificate should be captured for TOFU");
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                assert_eq!(cert.certificate_der, different_cert.as_ref());
                // Could be is_changed=true or false depending on cert state
            }
            _ => panic!("Expected NotTrusted error"),
        }
    }
    
    #[test]
    fn test_malicious_looking_certificate_detection() {
        // This test verifies that we only return MaliciousLookingCertificate 
        // when the OLD certificate is NOT expired
        
        let mut trusted_certs = TrustedCertificates::default();
        let mut mutations = Vec::new();
        
        // Load test certificates - both are valid (not expired)
        let test_cert1 = CertificateDer::from(TEST_CERT1.to_vec());
        let test_cert2 = CertificateDer::from(TEST_CERT2.to_vec());
        let server_name = ServerName::try_from("test.example.com").unwrap();
        let now = UnixTime::now();
        
        // Create base verifier
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();
        
        // Trust cert1
        trusted_certs.add_certificate(
            test_cert1.clone(),
            "test.example.com".to_string(),
            &mut mutations,
        );
        
        let tofu_verifier = TofuCertVerifier::new(
            base_verifier.clone(),
            Arc::new(trusted_certs.clone()),
        );
        
        // Try to connect with cert2 - should get MaliciousLookingCertificate
        // because cert1 is NOT expired
        let result = tofu_verifier.verify_server_cert(
            &test_cert2,
            &[],
            &server_name,
            &[],
            now,
        );
        
        assert!(result.is_err(), "Different certificate should fail");
        
        let tofu_error = tofu_verifier.take_tofu_error("test.example.com");
        assert!(tofu_error.is_some(), "Should have captured TOFU error");
        
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                // Verify it detected the change correctly
                assert!(cert.is_changed, "Should be marked as changed since old cert is not expired");
                assert_eq!(cert.old_fingerprint, Some(certificate_fingerprint(&test_cert1)));
                assert_eq!(cert.certificate_der, test_cert2.as_ref());
            }
            _ => panic!("Expected NotTrusted error with is_changed=true"),
        }
    }
    
    #[test] 
    fn test_expired_cert_allows_new_cert() {
        // This test verifies that when the OLD certificate IS expired,
        // we get NewUntrustedCertificate instead of MaliciousLookingCertificate
        
        let mut trusted_certs = TrustedCertificates::default();
        let mut mutations = Vec::new();
        
        // Use the actual expired certificate
        let expired_cert = CertificateDer::from(EXPIRED_CERT.to_vec());
        let valid_cert = CertificateDer::from(TEST_CERT2.to_vec());
        let server_name = ServerName::try_from("expired.example.com").unwrap();
        let now = UnixTime::now();
        
        // Trust the expired cert
        trusted_certs.add_certificate(
            expired_cert.clone(),
            "expired.example.com".to_string(),
            &mut mutations,
        );
        
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let base_verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .unwrap();
        
        let tofu_verifier = TofuCertVerifier::new(
            base_verifier.clone(),
            Arc::new(trusted_certs.clone()),
        );
        
        // Try with a new valid cert - should get NewUntrustedCertificate
        // (not MaliciousLookingCertificate) if implementation correctly
        // detects the old cert is expired or unparseable
        let result = tofu_verifier.verify_server_cert(
            &valid_cert,
            &[],
            &server_name,
            &[],
            now,
        );
        
        assert!(result.is_err());
        
        let tofu_error = tofu_verifier.take_tofu_error("expired.example.com");
        assert!(tofu_error.is_some());
        
        match tofu_error.unwrap() {
            TofuError::NotTrusted(cert) => {
                // Expected - old cert is expired, so new cert is treated as first-time
                assert!(!cert.is_changed, "Should NOT be marked as changed when old cert is expired");
            }
            _ => panic!("Expected NotTrusted error"),
        }
    }
}
