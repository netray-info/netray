//! `EvidencePath::parse` splits `<block>/<key>[@<row>]` at the first '@' after the '/'.
use netray_engine::EvidencePath;
use netray_model::evidence::BlockId;

#[test]
fn parse_splits_block_key_and_row() {
    let cases = [
        ("dns.caa/ttl", "dns.caa", "ttl", None),
        (
            "http.headers/value@date",
            "http.headers",
            "value",
            Some("date"),
        ),
        (
            "email.dnsbl/answer@192.0.2.1@zen.example.com",
            "email.dnsbl",
            "answer",
            Some("192.0.2.1@zen.example.com"),
        ),
        (
            "tls.chain@192.0.2.1:443/subject",
            "tls.chain@192.0.2.1:443",
            "subject",
            None,
        ),
    ];
    for (path, block, key, row) in cases {
        let (b, k, r) = EvidencePath::parse(path).unwrap_or_else(|_| panic!("{path:?} must parse"));
        assert_eq!(b, BlockId::parse(block).unwrap(), "{path}");
        assert_eq!(k, key, "{path}");
        assert_eq!(r, row, "{path}");
    }
}

#[test]
fn parse_refuses_paths_without_block_or_key() {
    for bad in ["", "dns.caa", "dns.caa/", "/ttl", "Bad/ttl"] {
        assert!(EvidencePath::parse(bad).is_err(), "{bad:?} must be refused");
    }
}

#[test]
fn parse_wildcard_key_yields_no_row() {
    let (b, k, r) = EvidencePath::parse("email.fcrdns/*").expect("wildcard key must parse");
    assert_eq!(b, BlockId::parse("email.fcrdns").unwrap());
    assert_eq!(k, "*");
    assert_eq!(r, None);
}

#[test]
fn parse_refuses_wildcard_key_with_row() {
    assert!(
        EvidencePath::parse("email.fcrdns/*@x").is_err(),
        "a `*` key with a row key must be refused"
    );
}
