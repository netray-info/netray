use mhost::RecordType;
use mhost::resolver::{ResolverGroup, UniQuery};

/// Query the HTTPS DNS record (type 65) for a hostname and check if the `ech`
/// SVCB parameter is present, indicating ECH support is advertised.
///
/// Returns `false` on any lookup failure.
pub async fn has_ech_advertised(resolvers: &ResolverGroup, hostname: &str) -> bool {
    let hostname = hostname.trim_end_matches('.');

    let query = match UniQuery::new(hostname, RecordType::HTTPS) {
        Ok(q) => q,
        Err(_) => return false,
    };

    let lookups = match resolvers.lookup(query).await {
        Ok(l) => l,
        Err(_) => return false,
    };

    // lookups.https() returns Vec<&SVCB>; SvcParam key is a string in mhost.
    // ECH is advertised as svcparam key "ech" (SvcParamKey 5 in RFC 9460).
    lookups
        .https()
        .into_iter()
        .any(|svcb| svcb.svc_params().iter().any(|param| param.key() == "ech"))
}

/// Whether any HTTPS record in presentation form (`1 . alpn=h2 ech=...`) carries the `ech`
/// SvcParam.
pub fn ech_in_presentation(records: &[String]) -> bool {
    records.iter().any(|rr| {
        rr.split_whitespace()
            .any(|param| param.split_once('=').is_some_and(|(key, _)| key == "ech"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ech_param_is_found_in_presentation_form() {
        assert!(ech_in_presentation(&[
            "1 . alpn=h2".into(),
            "1 . alpn=h2,h3 ech=AEX+DQBB".into(),
        ]));
    }

    #[test]
    fn records_without_ech_param_are_not_ech() {
        assert!(!ech_in_presentation(&[
            "1 . alpn=h2 ipv4hint=192.0.2.1".into()
        ]));
        assert!(!ech_in_presentation(&[]));
    }
}
