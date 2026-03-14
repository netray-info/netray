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
    lookups.https().into_iter().any(|svcb| {
        svcb.svc_params().iter().any(|param| param.key() == "ech")
    })
}
