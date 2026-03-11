use std::collections::HashSet;

use mhost::RecordType;
use mhost::resolver::{ResolverGroup, UniQuery};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaaLookup {
    pub records: Vec<CaaRecord>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaaRecord {
    pub tag: String,
    pub value: String,
    pub issuer_critical: bool,
}

impl CaaLookup {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Return values from all `issue` tags, stripped of parameters.
    pub fn issue_domains(&self) -> Vec<&str> {
        self.records
            .iter()
            .filter(|r| r.tag == "issue")
            .map(|r| {
                // Strip parameters: "letsencrypt.org; accounturi=..." → "letsencrypt.org"
                r.value.split(';').next().unwrap_or("").trim()
            })
            .collect()
    }

    pub fn issuewild_present(&self) -> bool {
        self.records.iter().any(|r| r.tag == "issuewild")
    }
}

/// Returns the parent domain by stripping the leftmost label, or `None` if
/// `domain` is already a TLD (single label).
///
/// Examples: `"www.example.com"` → `Some("example.com")`,
/// `"com"` → `None`.
fn parent_domain(domain: &str) -> Option<&str> {
    let dot = domain.find('.')?;
    Some(&domain[dot + 1..])
}

/// Fetch CAA records with RFC 8659 tree-climbing.
///
/// Walks up the domain tree: www.example.com → example.com → com.
/// Returns the first level that has CAA records, or empty if none found.
pub async fn lookup_caa(resolvers: &ResolverGroup, hostname: &str) -> CaaLookup {
    let mut domain = hostname.trim_end_matches('.');

    // Walk up the domain tree (RFC 8659 §3); stop before bare TLD
    loop {
        if let Some(lookup) = query_caa(resolvers, domain).await
            && !lookup.is_empty()
        {
            return lookup;
        }
        match parent_domain(domain) {
            Some(parent) if parent.contains('.') => domain = parent,
            _ => break,
        }
    }

    CaaLookup { records: vec![] }
}

async fn query_caa(resolvers: &ResolverGroup, domain: &str) -> Option<CaaLookup> {
    let query = UniQuery::new(domain, RecordType::CAA).ok()?;
    let lookups = resolvers.lookup(query).await.ok()?;

    // Deduplicate by (tag, value) across nameservers
    let mut seen = HashSet::new();
    let records: Vec<CaaRecord> = lookups
        .caa()
        .into_iter()
        .filter_map(|caa| {
            let key = (caa.tag().to_string(), caa.value().to_string());
            if seen.insert(key) {
                Some(CaaRecord {
                    tag: caa.tag().to_string(),
                    value: caa.value().to_string(),
                    issuer_critical: caa.issuer_critical(),
                })
            } else {
                None
            }
        })
        .collect();

    Some(CaaLookup { records })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parent_domain ---

    #[test]
    fn parent_domain_three_labels() {
        assert_eq!(parent_domain("www.example.com"), Some("example.com"));
    }

    #[test]
    fn parent_domain_four_labels() {
        assert_eq!(
            parent_domain("sub.sub.example.com"),
            Some("sub.example.com")
        );
    }

    #[test]
    fn parent_domain_two_labels() {
        assert_eq!(parent_domain("example.com"), Some("com"));
    }

    #[test]
    fn parent_domain_single_label_is_none() {
        assert_eq!(parent_domain("com"), None);
    }

    // --- CaaLookup helpers ---

    #[test]
    fn empty_lookup_is_empty() {
        let lookup = CaaLookup { records: vec![] };
        assert!(lookup.is_empty());
        assert!(lookup.issue_domains().is_empty());
        assert!(!lookup.issuewild_present());
    }

    #[test]
    fn issue_domains_extracts_values() {
        let lookup = CaaLookup {
            records: vec![
                CaaRecord {
                    tag: "issue".into(),
                    value: "letsencrypt.org".into(),
                    issuer_critical: false,
                },
                CaaRecord {
                    tag: "issue".into(),
                    value: "digicert.com; cansignhttpexchanges=yes".into(),
                    issuer_critical: false,
                },
                CaaRecord {
                    tag: "iodef".into(),
                    value: "mailto:admin@example.com".into(),
                    issuer_critical: false,
                },
            ],
        };
        let domains = lookup.issue_domains();
        assert_eq!(domains, vec!["letsencrypt.org", "digicert.com"]);
    }

    #[test]
    fn issuewild_detected() {
        let lookup = CaaLookup {
            records: vec![CaaRecord {
                tag: "issuewild".into(),
                value: "letsencrypt.org".into(),
                issuer_critical: false,
            }],
        };
        assert!(lookup.issuewild_present());
    }
}
