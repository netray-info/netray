use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, Error as TlsError, SignatureScheme};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

/// Captured data from the TLS handshake verification callback.
#[derive(Debug, Default)]
struct CapturedData {
    ocsp_response: Option<Vec<u8>>,
}

/// Custom certificate verifier that accepts any certificate.
/// This is intentional -- tlsight is an inspection tool that must see broken certs.
/// Also captures the OCSP staple delivered during verification.
#[derive(Debug)]
struct AcceptAnyCert {
    captured: Arc<Mutex<CapturedData>>,
}

impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        if !ocsp_response.is_empty()
            && let Ok(mut captured) = self.captured.lock()
        {
            captured.ocsp_response = Some(ocsp_response.to_vec());
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Result of a successful TLS handshake.
pub struct HandshakeResult {
    pub version: Option<rustls::ProtocolVersion>,
    pub cipher_suite: Option<rustls::SupportedCipherSuite>,
    pub alpn: Option<Vec<u8>>,
    pub peer_certs: Option<Vec<CertificateDer<'static>>>,
    pub ocsp_response: Option<Vec<u8>>,
    pub key_exchange_group: Option<String>,
    pub handshake_ms: u32,
}

/// Protocols that require a STARTTLS upgrade before the TLS handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartTlsProtocol {
    Smtp,
    Imap,
    Ftp,
}

impl StartTlsProtocol {
    /// Detect STARTTLS protocol from port number.
    pub fn from_port(port: u16) -> Option<Self> {
        match port {
            25 | 587 => Some(Self::Smtp),
            143 => Some(Self::Imap),
            21 => Some(Self::Ftp),
            _ => None,
        }
    }
}

/// Perform the SMTP STARTTLS upgrade on an established TCP stream.
/// Sends EHLO, waits for 250, sends STARTTLS, waits for 220.
async fn smtp_starttls(stream: &mut tokio::net::TcpStream) -> io::Result<()> {
    let mut buf = [0u8; 1024];

    // Read server greeting (220 ...)
    let n = stream.read(&mut buf).await?;
    let greeting = String::from_utf8_lossy(&buf[..n]);
    if !greeting.starts_with("220") {
        return Err(io::Error::other(format!(
            "unexpected SMTP greeting: {}",
            greeting.trim()
        )));
    }

    // Send EHLO
    stream.write_all(b"EHLO tlsight.local\r\n").await?;

    // Read EHLO response (may be multi-line; read until non-'250-' line)
    let mut ehlo_ok = false;
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let resp = String::from_utf8_lossy(&buf[..n]);
        if resp.contains("250 ") || resp.contains("250\r\n") || resp.ends_with("250\r\n") {
            ehlo_ok = true;
            break;
        }
        if resp.starts_with("250-") {
            // multi-line, keep reading
            continue;
        }
        // Accept any 250 response that got through
        if resp.starts_with("250") {
            ehlo_ok = true;
            break;
        }
        break;
    }

    if !ehlo_ok {
        return Err(io::Error::other("EHLO failed"));
    }

    // Send STARTTLS
    stream.write_all(b"STARTTLS\r\n").await?;

    // Read STARTTLS response (220 Go ahead)
    let n = stream.read(&mut buf).await?;
    let resp = String::from_utf8_lossy(&buf[..n]);
    if !resp.starts_with("220") {
        return Err(io::Error::other(format!(
            "STARTTLS rejected: {}",
            resp.trim()
        )));
    }

    Ok(())
}

/// Perform a TLS handshake with a target IP and port.
pub async fn tls_handshake(
    ip: IpAddr,
    port: u16,
    sni: Option<&str>,
    timeout: Duration,
) -> Result<HandshakeResult, Box<dyn std::error::Error + Send + Sync>> {
    let captured = Arc::new(Mutex::new(CapturedData::default()));

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert {
            captured: Arc::clone(&captured),
        }))
        .with_no_client_auth();

    let server_name = match sni {
        Some(name) => {
            ServerName::try_from(name.to_owned()).map_err(|e| format!("invalid SNI: {e}"))?
        }
        None => ServerName::IpAddress(ip.into()),
    };

    let connector = TlsConnector::from(Arc::new(config));
    let start = Instant::now();
    let starttls = StartTlsProtocol::from_port(port);

    let tls_stream = tokio::time::timeout(timeout, async {
        let mut tcp = TcpStream::connect((ip, port)).await?;
        tcp.set_nodelay(true)?;

        // STARTTLS upgrade before TLS handshake
        if let Some(proto) = starttls {
            match proto {
                StartTlsProtocol::Smtp => {
                    smtp_starttls(&mut tcp).await?;
                }
                // IMAP and FTP are stubs — not yet implemented
                StartTlsProtocol::Imap | StartTlsProtocol::Ftp => {}
            }
        }

        let tls = connector.connect(server_name, tcp).await?;
        Ok::<_, io::Error>(tls)
    })
    .await
    .map_err(|_| "handshake timed out")?
    .map_err(|e| format!("connection failed: {e}"))?;

    let handshake_ms = start.elapsed().as_millis() as u32;

    let (_, conn) = tls_stream.get_ref();
    let version = conn.protocol_version();
    let cipher_suite = conn.negotiated_cipher_suite();
    let alpn = conn.alpn_protocol().map(|p| p.to_vec());
    let peer_certs = conn.peer_certificates().map(|certs| {
        certs
            .iter()
            .map(|c| CertificateDer::from(c.as_ref().to_vec()))
            .collect()
    });
    let key_exchange_group = conn
        .negotiated_key_exchange_group()
        .map(|g| format_kx_group(g.name()));

    let ocsp_response = captured.lock().ok().and_then(|c| c.ocsp_response.clone());

    Ok(HandshakeResult {
        version,
        cipher_suite,
        alpn,
        peer_certs,
        ocsp_response,
        key_exchange_group,
        handshake_ms,
    })
}

fn format_kx_group(group: rustls::NamedGroup) -> String {
    match group {
        rustls::NamedGroup::X25519 => "X25519".to_string(),
        rustls::NamedGroup::secp256r1 => "P-256".to_string(),
        rustls::NamedGroup::secp384r1 => "P-384".to_string(),
        rustls::NamedGroup::secp521r1 => "P-521".to_string(),
        rustls::NamedGroup::X448 => "X448".to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::net::TcpListener;

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    /// Generates a self-signed cert+key for "localhost" using rcgen.
    fn make_self_signed() -> (
        rustls::pki_types::CertificateDer<'static>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ) {
        use rcgen::CertifiedKey;
        let CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let cert_der = rustls::pki_types::CertificateDer::from(cert.der().to_vec());
        let key_der =
            rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();
        (cert_der, key_der)
    }

    /// Spawns a TLS server on 127.0.0.1:0. Returns the bound port.
    /// The server accepts one connection and immediately closes after the handshake.
    async fn spawn_tls_server() -> u16 {
        ensure_crypto_provider();
        let (cert_der, key_der) = make_self_signed();

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(server_config));
            if let Ok((stream, _)) = listener.accept().await {
                let _ = acceptor.accept(stream).await;
                // Just close after handshake
            }
        });

        port
    }

    /// Spawns a TLS server that hangs after TCP accept (never completes handshake).
    async fn spawn_hanging_server() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            if let Ok((_stream, _)) = listener.accept().await {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });
        port
    }

    #[tokio::test]
    async fn successful_handshake_returns_peer_certs() {
        ensure_crypto_provider();
        let port = spawn_tls_server().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let result = tls_handshake(ip, port, Some("localhost"), Duration::from_secs(5)).await;

        if let Err(ref e) = result {
            panic!("handshake should succeed: {e}");
        }
        let res = result.unwrap();
        assert!(res.peer_certs.is_some(), "should have peer certs");
        assert!(
            !res.peer_certs.unwrap().is_empty(),
            "peer certs should be non-empty"
        );
    }

    #[tokio::test]
    async fn successful_handshake_returns_tls_version() {
        ensure_crypto_provider();
        let port = spawn_tls_server().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let result = tls_handshake(ip, port, Some("localhost"), Duration::from_secs(5)).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(res.version.is_some());
    }

    #[tokio::test]
    async fn successful_handshake_returns_cipher_suite() {
        ensure_crypto_provider();
        let port = spawn_tls_server().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let result = tls_handshake(ip, port, Some("localhost"), Duration::from_secs(5)).await;

        assert!(result.is_ok());
        let res = result.unwrap();
        assert!(res.cipher_suite.is_some());
    }

    #[tokio::test]
    async fn timeout_fires_when_server_hangs() {
        ensure_crypto_provider();
        let port = spawn_hanging_server().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let result = tls_handshake(ip, port, Some("localhost"), Duration::from_millis(100)).await;

        assert!(result.is_err(), "should timeout");
        let err = result.err().unwrap().to_string();
        assert!(
            err.contains("timed out"),
            "error should mention timeout: {err}"
        );
    }

    #[tokio::test]
    async fn connection_refused_returns_error() {
        ensure_crypto_provider();
        // Port 1 is never open
        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let result = tls_handshake(ip, 1, Some("localhost"), Duration::from_secs(2)).await;

        assert!(result.is_err(), "should fail for refused connection");
    }

    #[tokio::test]
    async fn handshake_ms_is_populated() {
        ensure_crypto_provider();
        let port = spawn_tls_server().await;
        tokio::time::sleep(Duration::from_millis(10)).await;

        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let result = tls_handshake(ip, port, Some("localhost"), Duration::from_secs(5)).await;

        assert!(result.is_ok());
        assert!(result.unwrap().handshake_ms < 1000);
    }
}
