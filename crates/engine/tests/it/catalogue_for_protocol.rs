//! `Catalogue::for_protocol` (V2 spec v2-inspectors, requirement 16): one protocol's checks, in
//! catalogue order, and no other protocol's.
use netray_engine::catalogue;
use netray_model::Protocol;

const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

#[test]
fn for_protocol_holds_exactly_that_protocols_checks_in_catalogue_order() {
    for protocol in PROTOCOLS {
        let expected: Vec<String> = catalogue()
            .entries()
            .iter()
            .filter(|e| e.protocol == protocol)
            .map(|e| e.id.to_string())
            .collect();
        let got = catalogue().for_protocol(protocol);
        let got_ids: Vec<String> = got.entries().iter().map(|e| e.id.to_string()).collect();
        assert_eq!(got_ids, expected, "{protocol:?}");
        assert!(
            got.entries().iter().all(|e| e.protocol == protocol),
            "{protocol:?} result holds another protocol's check"
        );
    }
    assert!(
        !catalogue().for_protocol(Protocol::Tls).entries().is_empty(),
        "the catalogue holds TLS checks"
    );
}
