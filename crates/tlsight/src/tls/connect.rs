use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    pub handshake_ms: u32,
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

    let tls_stream = tokio::time::timeout(timeout, async {
        let tcp = TcpStream::connect((ip, port)).await?;
        tcp.set_nodelay(true)?;
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

    let ocsp_response = captured.lock().ok().and_then(|c| c.ocsp_response.clone());

    Ok(HandshakeResult {
        version,
        cipher_suite,
        alpn,
        peer_certs,
        ocsp_response,
        handshake_ms,
    })
}
