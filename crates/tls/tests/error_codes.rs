// C2, C8: a connect error that says "this host cannot reach the target" is not a verdict on
// the target. Contract for the coder: in `netray_tls::tls`,
//
//     pub fn error_code(err: &(dyn std::error::Error + Send + Sync + 'static)) -> &'static str
//
// maps the error `connect::tls_handshake` returns (a `Box<dyn Error + Send + Sync>`) to the
// code `inspect_ip` reports. `tls_handshake` must keep the original `io::Error` reachable by
// downcast (today it flattens it into a String), and `inspect_ip` must use this function.

use std::io;

use netray_tls::tls::error_code;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

#[cfg(target_os = "macos")]
const LOCAL: [(&str, i32); 2] = [("ENETUNREACH", 51), ("EADDRNOTAVAIL", 49)];
// EHOSTUNREACH is not local: a remote router or the target's firewall produces it.
#[cfg(target_os = "macos")]
const REMOTE_HOST_UNREACH: i32 = 65;
#[cfg(target_os = "linux")]
const LOCAL: [(&str, i32); 2] = [("ENETUNREACH", 101), ("EADDRNOTAVAIL", 99)];
#[cfg(target_os = "linux")]
const REMOTE_HOST_UNREACH: i32 = 113;

#[test]
fn local_network_errors_are_not_tested_from_here() {
    for (name, errno) in LOCAL {
        let e: BoxErr = Box::new(io::Error::from_raw_os_error(errno));
        assert_eq!(error_code(&*e), "NOT_TESTED_FROM_HERE", "{name}");
    }
    for kind in [
        io::ErrorKind::NetworkUnreachable,
        io::ErrorKind::AddrNotAvailable,
    ] {
        let e: BoxErr = Box::new(io::Error::from(kind));
        assert_eq!(error_code(&*e), "NOT_TESTED_FROM_HERE", "{kind:?}");
    }
}

#[test]
fn target_side_errors_stay_handshake_failed() {
    for kind in [
        io::ErrorKind::ConnectionRefused,
        io::ErrorKind::TimedOut,
        io::ErrorKind::HostUnreachable,
    ] {
        let e: BoxErr = Box::new(io::Error::from(kind));
        assert_eq!(error_code(&*e), "HANDSHAKE_FAILED", "{kind:?}");
    }
    // EHOSTUNREACH on both macOS (65) and Linux (113): a remote verdict, not a local one.
    for errno in [65, 113, REMOTE_HOST_UNREACH] {
        let e: BoxErr = Box::new(io::Error::from_raw_os_error(errno));
        if e.downcast_ref::<io::Error>().map(|e| e.kind()) == Some(io::ErrorKind::HostUnreachable) {
            assert_eq!(error_code(&*e), "HANDSHAKE_FAILED", "errno {errno}");
        }
    }
    let e: BoxErr = "handshake timed out".into();
    assert_eq!(error_code(&*e), "HANDSHAKE_FAILED");
}

// Pinning test: connect.rs must keep the io::Error boxed so error_code can classify it.
#[tokio::test]
async fn pin_tls_handshake_keeps_io_error_boxed_for_refused_connection() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let err = match netray_tls::tls::connect::tls_handshake(
        "127.0.0.1".parse().unwrap(),
        port,
        None,
        std::time::Duration::from_secs(2),
    )
    .await
    {
        Ok(_) => panic!("handshake to a closed port must fail"),
        Err(e) => e,
    };
    let io_err = err
        .downcast_ref::<io::Error>()
        .expect("error must downcast to io::Error");
    assert_eq!(io_err.kind(), io::ErrorKind::ConnectionRefused);
    assert_eq!(error_code(&*err), "HANDSHAKE_FAILED");
}
