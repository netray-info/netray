// C2, C8: a connect error that says "this host cannot reach the target" is not a verdict on
// the target. Contract for the coder: in `tlsight::tls`,
//
//     pub fn error_code(err: &(dyn std::error::Error + Send + Sync + 'static)) -> &'static str
//
// maps the error `connect::tls_handshake` returns (a `Box<dyn Error + Send + Sync>`) to the
// code `inspect_ip` reports. `tls_handshake` must keep the original `io::Error` reachable by
// downcast (today it flattens it into a String), and `inspect_ip` must use this function.

use std::io;

use tlsight::tls::error_code;

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

#[cfg(target_os = "macos")]
const LOCAL: [(&str, i32); 3] = [
    ("ENETUNREACH", 51),
    ("EHOSTUNREACH", 65),
    ("EADDRNOTAVAIL", 49),
];
#[cfg(target_os = "linux")]
const LOCAL: [(&str, i32); 3] = [
    ("ENETUNREACH", 101),
    ("EHOSTUNREACH", 113),
    ("EADDRNOTAVAIL", 99),
];

#[test]
fn local_network_errors_are_not_tested_from_here() {
    for (name, errno) in LOCAL {
        let e: BoxErr = Box::new(io::Error::from_raw_os_error(errno));
        assert_eq!(error_code(&*e), "NOT_TESTED_FROM_HERE", "{name}");
    }
    for kind in [
        io::ErrorKind::NetworkUnreachable,
        io::ErrorKind::HostUnreachable,
        io::ErrorKind::AddrNotAvailable,
    ] {
        let e: BoxErr = Box::new(io::Error::from(kind));
        assert_eq!(error_code(&*e), "NOT_TESTED_FROM_HERE", "{kind:?}");
    }
}

#[test]
fn target_side_errors_stay_handshake_failed() {
    for kind in [io::ErrorKind::ConnectionRefused, io::ErrorKind::TimedOut] {
        let e: BoxErr = Box::new(io::Error::from(kind));
        assert_eq!(error_code(&*e), "HANDSHAKE_FAILED", "{kind:?}");
    }
    let e: BoxErr = "handshake timed out".into();
    assert_eq!(error_code(&*e), "HANDSHAKE_FAILED");
}
