//! TLS wraps the whole TCP connection (guide §4.4); the certificate is checked
//! against the host name with the system's trust store.

use std::io;
use std::sync::{Arc, OnceLock};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::{client::TlsStream, TlsConnector};

fn config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
            let verifier =
                rustls_platform_verifier::Verifier::new().with_provider(provider.clone());
            Arc::new(
                ClientConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()
                    .expect("ring supports TLS 1.2 and 1.3")
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(verifier))
                    .with_no_client_auth(),
            )
        })
        .clone()
}

pub async fn connect(host: &str, tcp: TcpStream) -> io::Result<TlsStream<TcpStream>> {
    let name = ServerName::try_from(host.to_string())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    TlsConnector::from(config())
        .connect(name, tcp)
        .await
        .map_err(|e| io::Error::new(e.kind(), format!("TLS: {e}")))
}
