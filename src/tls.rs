use crate::{config, entity};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::sync::Once;

pub fn ensure_crypto_provider() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("failed to install rustls crypto provider");
    });
}

pub fn build_client_config(
    protocol: &config::Protocol,
    ca_cert_path: Option<&str>,
) -> Result<rustls::ClientConfig, entity::GatewayError> {
    ensure_crypto_provider();
    let root_store = build_client_root_store(ca_cert_path)?;
    let mut client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    client_config.alpn_protocols = alpn_protocols(protocol);
    Ok(client_config)
}

pub fn build_client_root_store(
    ca_cert_path: Option<&str>,
) -> Result<rustls::RootCertStore, entity::GatewayError> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    if let Some(path) = ca_cert_path {
        for cert in load_certificates(path)? {
            root_store.add(cert).map_err(|err| {
                entity::GatewayError::InvalidRouteConfig(format!(
                    "failed to add tls certificate '{}': {}",
                    path, err
                ))
            })?;
        }
    }

    Ok(root_store)
}

pub fn build_server_config(
    server_tls: &config::ServerTlsConfig,
) -> Result<rustls::ServerConfig, entity::GatewayError> {
    ensure_crypto_provider();
    let cert_path = server_tls.cert_path.as_deref().ok_or_else(|| {
        entity::GatewayError::InvalidRouteConfig(
            "server tls requires a certificate path".to_string(),
        )
    })?;
    let key_path = server_tls.key_path.as_deref().ok_or_else(|| {
        entity::GatewayError::InvalidRouteConfig(
            "server tls requires a private key path".to_string(),
        )
    })?;

    let certs = load_certificates(cert_path)?;
    let key = load_private_key(key_path)?;

    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|err| {
            entity::GatewayError::InvalidRouteConfig(format!(
                "failed to build server tls config: {}",
                err
            ))
        })?;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(server_config)
}

pub fn load_certificates(path: &str) -> Result<Vec<CertificateDer<'static>>, entity::GatewayError> {
    let bytes = std::fs::read(path).map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "failed to read tls certificate '{}': {}",
            path, err
        ))
    })?;

    if bytes.starts_with(b"-----BEGIN") {
        let mut reader = std::io::BufReader::new(bytes.as_slice());
        let certs = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                entity::GatewayError::InvalidRouteConfig(format!(
                    "failed to parse PEM certificate '{}': {}",
                    path, err
                ))
            })?;

        if certs.is_empty() {
            return Err(entity::GatewayError::InvalidRouteConfig(format!(
                "tls certificate '{}' did not contain any certificates",
                path
            )));
        }

        Ok(certs)
    } else {
        Ok(vec![CertificateDer::from(bytes)])
    }
}

pub fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>, entity::GatewayError> {
    let bytes = std::fs::read(path).map_err(|err| {
        entity::GatewayError::InvalidRouteConfig(format!(
            "failed to read tls private key '{}': {}",
            path, err
        ))
    })?;

    if bytes.starts_with(b"-----BEGIN") {
        let mut reader = std::io::BufReader::new(bytes.as_slice());
        let key = rustls_pemfile::private_key(&mut reader).map_err(|err| {
            entity::GatewayError::InvalidRouteConfig(format!(
                "failed to parse PEM private key '{}': {}",
                path, err
            ))
        })?;

        key.ok_or_else(|| {
            entity::GatewayError::InvalidRouteConfig(format!(
                "tls private key '{}' did not contain a supported key",
                path
            ))
        })
    } else {
        Ok(PrivateKeyDer::from(PrivatePkcs8KeyDer::from(bytes)))
    }
}

fn alpn_protocols(protocol: &config::Protocol) -> Vec<Vec<u8>> {
    match protocol {
        config::Protocol::Http1 => vec![b"http/1.1".to_vec()],
        config::Protocol::H2 => vec![b"h2".to_vec()],
        config::Protocol::H2c => Vec::new(),
    }
}
