use mhost::RecordType;
use mhost::resolver::{ResolverGroup, UniQuery};

/// Query the HTTPS DNS record (type 65) for a hostname and check if the `ech`
/// SVCB parameter is present, indicating ECH support is advertised.
///
/// Returns `None` when the lookup failed or no nameserver answered definitively.
pub async fn has_ech_advertised(resolvers: &ResolverGroup, hostname: &str) -> Option<bool> {
    let hostname = hostname.trim_end_matches('.');

    let query = UniQuery::new(hostname, RecordType::HTTPS).ok()?;
    let lookups = resolvers.lookup(query).await.ok()?;
    if !super::answered(&lookups) {
        return None;
    }

    // lookups.https() returns Vec<&SVCB>; SvcParam key is a string in mhost.
    // ECH is advertised as svcparam key "ech" (SvcParamKey 5 in RFC 9460).
    Some(
        lookups
            .https()
            .into_iter()
            .any(|svcb| svcb.svc_params().iter().any(|param| param.key() == "ech")),
    )
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
