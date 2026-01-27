use crate::config::TlsConfig;
use rustls::server::WebPkiClientVerifier;
use rustls::{pki_types, ServerConfig};
use std::sync::Arc;

#[cfg(feature = "http3")]
use quinn::crypto::rustls::QuicServerConfig;
#[cfg(feature = "http3")]
use quinn::ServerConfig as QuinnServerConfig;

pub fn load_certs(
    filename: &str,
) -> Result<Vec<pki_types::CertificateDer<'static>>, Box<dyn std::error::Error + Send + Sync>> {
    let contents = std::fs::read(filename)?;
    let mut certs = Vec::new();
    for pem in rustls_pki_types::pem::SliceIter::<pki_types::CertificateDer>::new(&contents) {
        certs.push(pem?);
    }
    if certs.is_empty() {
        return Err("No certificates found in file".into());
    }
    Ok(certs)
}

pub fn load_private_key(
    filename: &str,
) -> Result<pki_types::PrivateKeyDer<'static>, Box<dyn std::error::Error + Send + Sync>> {
    let contents = std::fs::read(filename)?;

    // Try PKCS8 first (most common)
    if let Some(key) =
        rustls_pki_types::pem::SliceIter::<pki_types::PrivatePkcs8KeyDer>::new(&contents).next()
    {
        return Ok(pki_types::PrivateKeyDer::Pkcs8(key?));
    }

    // Try SEC1 (EC keys)
    if let Some(key) =
        rustls_pki_types::pem::SliceIter::<pki_types::PrivateSec1KeyDer>::new(&contents).next()
    {
        return Ok(pki_types::PrivateKeyDer::Sec1(key?));
    }

    // Try PKCS1 (RSA keys)
    if let Some(key) =
        rustls_pki_types::pem::SliceIter::<pki_types::PrivatePkcs1KeyDer>::new(&contents).next()
    {
        return Ok(pki_types::PrivateKeyDer::Pkcs1(key?));
    }

    Err("No private key found in file".into())
}

pub fn create_tls_config(
    tls_config: &TlsConfig,
) -> Result<Arc<ServerConfig>, Box<dyn std::error::Error + Send + Sync>> {
    let certs = load_certs(&tls_config.server_cert)?;
    let key = load_private_key(&tls_config.server_key)?;

    // Configure client certificate verification
    let mut config = if tls_config.require_client_certs {
        let ca_cert = tls_config
            .ca_cert
            .as_ref()
            .ok_or("CA certificate is required when require_client_certs is true")?;
        let ca_certs = load_certs(ca_cert)?;

        let mut root_store = rustls::RootCertStore::empty();
        for cert in ca_certs {
            root_store.add(cert)?;
        }

        let client_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|e| format!("Failed to build client verifier: {}", e))?;

        ServerConfig::builder()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certs, key)?
    } else {
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?
    };

    config.alpn_protocols = vec![b"h2".to_vec()];

    Ok(Arc::new(config))
}

#[cfg(feature = "http3")]
pub fn create_quic_server_config(
    tls_config: &TlsConfig,
) -> Result<QuinnServerConfig, Box<dyn std::error::Error + Send + Sync>> {
    let certs = load_certs(&tls_config.server_cert)?;
    let key = load_private_key(&tls_config.server_key)?;

    // Configure client certificate verification
    let rustls_config = {
        let mut config = if tls_config.require_client_certs {
            let ca_cert = tls_config
                .ca_cert
                .as_ref()
                .ok_or("CA certificate is required when require_client_certs is true")?;
            let ca_certs = load_certs(ca_cert)?;

            let mut root_store = rustls::RootCertStore::empty();
            for cert in ca_certs {
                root_store.add(cert)?;
            }

            let client_verifier = WebPkiClientVerifier::builder(Arc::new(root_store))
                .build()
                .map_err(|e| format!("Failed to build client verifier: {}", e))?;

            ServerConfig::builder()
                .with_client_cert_verifier(client_verifier)
                .with_single_cert(certs, key)?
        } else {
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)?
        };
        config.alpn_protocols = vec![b"h3".to_vec()];
        config
    };

    let quic_config = QuicServerConfig::try_from(rustls_config)?;
    let server_config = QuinnServerConfig::with_crypto(Arc::new(quic_config));

    Ok(server_config)
}
